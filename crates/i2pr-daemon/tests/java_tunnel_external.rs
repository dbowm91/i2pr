//! Plan 199 Phase A — M6 Java I2P public-client second-family closure lane.
//! Plan 200 — Java public-client publication observability and verified
//! bootstrap.
//!
//! Fail-closed driver against exact-pinned Java I2P 2.13.0 on loopback.
//!
//! - strict SSU2 controlled profile (loopback, non-advertised);
//! - reference RouterInfo verified + bootstrapped through the ordinary
//!   parser/signature/freshness path into the authoritative store;
//! - effective floodfill capability verified before dispatch;
//! - authenticated SSU2 session establishes via the daemon-owned runtime;
//! - Plan 200 §B: post-bootstrap positive proof that the Java main
//!   NetDB can serve the peer RouterInfo through an ordinary
//!   `DatabaseLookup`/`DatabaseStore` round-trip (in both
//!   directions). The bootstrap submission alone is no longer
//!   considered proof of network-visible installation;
//! - one reference public client destination created through Java's
//!   public I2PClient/I2PSession surface (transient; only public
//!   Destination facts recorded, never keys);
//! - Plan 200 §A: helper `READY` is decoupled from any
//!   `leaseset=published` claim; publication is observed independently
//!   through sanitized Java log facts and an ordinary i2pr
//!   `DatabaseLookup` against the publication router;
//! - one real one-hop outbound + inbound tunnel build accepted by the
//!   reference, with build replies routed through
//!   `ExploratoryBuildCoordinator::route_inbound_i2np` to real
//!   `Installed` pool + registry roles (real cryptographically derived
//!   keys on both directions, no synthetic roles, no `LocalZeroHop`);
//! - the reference Standard LeaseSet2 resolved through the real
//!   tunnel NetDB path, validated through the existing validators,
//!   and cached through the existing LeaseSet2 store;
//! - the i2pr destination's own Standard LeaseSet2 (real inbound
//!   lease) published through the controlled NetDB path;
//! - a bounded message from i2pr reaches the reference RAW session
//!   through existing ECIES/Garlic + the real outbound tunnel + the
//!   selected remote lease (payload digest equality, plaintext never
//!   logged);
//! - a reply from the reference reaches i2pr through its real
//!   inbound tunnel and the existing ECIES decrypt path, delivered
//!   to the owning destination queue with sibling isolation;
//! - direct transport explicitly rejected as a counted path;
//! - creator-side liveness first test green.
//!
//! Plan 199 §A.5 — the Streaming surface reuses the same wire: a
//! second driver `streaming_through_java` runs Direction A
//! (`StreamingManager::connect` i2pr -> Java) + Direction B
//! (Java `STREAM CONNECT` -> i2pr listener/accept). Same
//! digest-equality / close / sibling / cleanup rules as the i2pd
//! first-family driver; only the reference identifier + diagnostic SAM
//! port differ.
//!
//! Plan 199 failure policy: if Java fails where i2pd passes, the
//! driver stops at the first failing protocol boundary and records
//! `plan199-java-stop` so the harness can mark every install-dependent
//! row `blocked` (never `passed`, never silently skipped).
//!
//! Plan 200 — one terminal `P200-*` classification is emitted per
//! run (see `record_p200_classification`). The classification is the
//! earliest protocol/lifecycle boundary at which the public-client
//! publication chain stopped. Plan 201 will own the corrective.

#![forbid(unsafe_code)]

use std::io::Write as _;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use flate2::Compression;
use flate2::write::GzEncoder;
use i2pr_client::streaming::connection::ConnectionState;
use i2pr_client::streaming::manager::{
    ConnectOutcome, DEFAULT_ADVERTISED_MAX_PAYLOAD, ListenerOutcome, RemoteDestination,
    StreamingManager,
};
use i2pr_client::streaming::{StreamingConfig, transport::TransportSendRequest};
use i2pr_client::{
    DestinationDispatcher, DestinationIdentity, DestinationOutboundRole, DestinationRouting,
    DestinationRoutingConfig, EciesSessionConfig, EciesSessionManager, InboundLeaseSource,
    OutboundRequest, StreamingDestinationAdapter, build_signed_lease_set2,
    compose_outbound_delivery,
};
use i2pr_daemon::config::Config;
use i2pr_daemon::destination_tunnels::{
    DestinationTunnelCoordinator, LeaseStoreIngestOutcome, reply_path_for_inbound_route,
};
use i2pr_daemon::exploratory_build::{
    BuildCoordinatorOutcome, BuildDirection, BuildRequest, ExploratoryBuildCoordinator,
    PeerBuildMaterial,
};
use i2pr_daemon::inbound_dispatch::dispatch_inbound_tunnel_data;
use i2pr_daemon::outbound_lookup::deliver_outbound_cells;
use i2pr_daemon::router_i2np::{
    RouterDeliveryOutcome, RouterDeliveryRequest, Ssu2DaemonService, daemon_dial_target,
    generate_controlled_identity, verify_reference_router_info,
};
use i2pr_daemon::tunnel_liveness::{
    FIRST_LIVENESS_DELAY_MS, LivenessAction, LivenessConfig, TunnelLivenessScheduler,
};
use i2pr_netdb::{LookupPolicy, RouterHash, RouterInfoStoreConfig};
use i2pr_proto::{
    DatabaseLookupMessage, DatabaseStoreData, DatabaseStoreMessage, Date, DeferredPayload, Hash,
    I2npBody, I2npMessage, MAX_I2NP_PAYLOAD_SIZE, Mapping, ReplyEncryption, RouterAddress,
    RouterInfo,
};
use i2pr_runtime::{CancellationToken, ChildFailurePolicy, ChildScope, Ssu2PqKem};
use i2pr_transport::{Deadline, MAX_I2NP_MESSAGE_BYTES, PeerId};
use i2pr_tunnel::bridge::{BridgeHeader, ShortBuildI2npBridge};
use i2pr_tunnel::config::ExploratoryPoolConfig;
use i2pr_tunnel::identity::{TunnelDirection, TunnelId};
use i2pr_tunnel::short_record::HopRole;
use rand_chacha::ChaCha8Rng;
use rand_core::{OsRng, SeedableRng};

const DIAL_TIMEOUT: Duration = Duration::from_secs(20);
const WAIT_TIMEOUT: Duration = Duration::from_secs(60);
const POLL_INTERVAL: Duration = Duration::from_millis(500);
const ACCEPT_TIMEOUT: Duration = Duration::from_secs(30);
const DELIVERY_TIMEOUT: Duration = Duration::from_secs(10);
const SAM_TIMEOUT: Duration = Duration::from_secs(15);
const DATAGRAM_WAIT: Duration = Duration::from_secs(45);
const STREAM_WAIT: Duration = Duration::from_secs(45);
const SYN_ACK_WAIT: Duration = Duration::from_secs(45);

const OUTBOUND_CREATOR: u32 = 0x1500;
const INBOUND_CREATOR: u32 = 0x1600;
const OBEP_RECEIVE: u32 = 0x9501;
const OBEP_NEXT: u32 = 0x9502;
const IBGW_RECEIVE: u32 = 0x9601;
const IBGW_NEXT: u32 = 0x9602;

// Plan 217 §6.D — destination and Streaming drivers run against the
// same long-lived Java RouterContexts inside a single `run-java.sh`
// invocation. Stock Java retains tunnel/build state for the normal
// tunnel lifetime and rejects duplicate build/tunnel-id
// registrations. To prevent the streaming driver from inheriting
// tunnel/build identifiers the destination driver already submitted
// (which is the Plan 194 §11 streaming-side stop), the streaming
// driver declares its own disjoint namespace inside its function
// scope. The destination driver keeps the file-level `OUTBOUND_…` /
// `OBEP_…` / `IBGW_…` constants; the streaming driver uses the
// `STREAM_OUTBOUND_…` / `STREAM_OBEP_…` / `STREAM_IBGW_…` and
// `STREAM_MSG_*` values defined at the start of `streaming_through_java`.

const LOCAL_STREAM_PORT: u16 = 0;
const REMOTE_STREAM_PORT: u16 = 0;

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

fn append_evidence(dir: &Path, label: &str, value: &str) {
    let sanitized = value.replace(['\t', '\n'], " ");
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("driver-evidence.tsv"))
        .expect("open evidence file");
    use std::io::Write as _;
    writeln!(file, "{label}\t{sanitized}").expect("write evidence row");
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = i2pr_crypto::sha256(bytes);
    let mut out = String::with_capacity(64);
    for byte in digest.as_bytes() {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// Plan 199 helper control channel. The Java helper owns all I2P client and
/// Streaming operations; this channel carries only sanitized coordination
/// commands and hex-encoded test bytes over loopback.
struct ReferenceControl {
    endpoint: SocketAddr,
}

impl ReferenceControl {
    async fn connect(endpoint: SocketAddr) -> Self {
        Self { endpoint }
    }

    async fn command(&mut self, command: &str) -> String {
        use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _};
        let mut stream =
            tokio::time::timeout(SAM_TIMEOUT, tokio::net::TcpStream::connect(self.endpoint))
                .await
                .expect("reference control connect timeout")
                .expect("reference control connect");
        stream
            .write_all(format!("{command}\n").as_bytes())
            .await
            .expect("reference control write");
        let mut reader = tokio::io::BufReader::new(stream);
        let mut line = String::new();
        tokio::time::timeout(SAM_TIMEOUT, reader.read_line(&mut line))
            .await
            .expect("reference control response timeout")
            .expect("reference control response read");
        line.trim_end().to_owned()
    }

    async fn wait_raw(&mut self, timeout: Duration) -> Option<(usize, String)> {
        let response = self.command(&format!("WAIT {}", timeout.as_millis())).await;
        let mut fields = response.split_whitespace();
        if fields.next() != Some("RECEIVED") {
            return None;
        }
        Some((fields.next()?.parse().ok()?, fields.next()?.to_owned()))
    }

    // Legacy `send_raw()` remains for historical tests that still need
    // it; the P222 reverse path uses `send_raw_tracked()`.
    #[allow(dead_code)]
    async fn send_raw(&mut self, destination: &str, payload: &[u8]) -> bool {
        let response = self
            .command(&format!("SEND {destination} {} 0 0", hex_encode(payload)))
            .await;
        response.starts_with("SENT ")
    }

    /// Plan 222 WP D — tracked helper send using the public
    /// listener-enabled `I2PSession.sendMessage(..., SendMessageStatusListener)`
    /// path. Returns the helper-side nonce, payload length, and SHA-256
    /// digest on success. Strict parsing: missing nonce, malformed
    /// integer, or malformed digest yields `None` (Unknown diagnostic
    /// evidence), never a protocol failure.
    async fn send_raw_tracked(
        &mut self,
        destination: &str,
        payload: &[u8],
        from_port: u16,
        to_port: u16,
    ) -> Option<TrackedSend> {
        let response = self
            .command(&format!(
                "SEND_TRACKED {destination} {} {from_port} {to_port}",
                hex_encode(payload)
            ))
            .await;
        p222_parse_tracked_sent(&response)
    }

    /// Plan 222 WP D — polls the bounded helper-local nonce event map.
    /// Returns the ordered status events, or `None` for unknown nonce /
    /// parse failure (Unknown diagnostic evidence). Never synthesizes a
    /// status. Rejects more than 16 events.
    async fn send_status(&mut self, nonce: u64) -> Option<Vec<TrackedStatusEvent>> {
        let response = self.command(&format!("SEND_STATUS {nonce}")).await;
        p222_parse_tracked_status(&response, nonce)
    }

    /// Plan 223 WP B — read-only exact Destination observation via the
    /// helper `INSPECT_DEST` surface. Returns the raw `DEST_INFO` line, or
    /// `None` when unreachable (Unknown, never a protocol fact).
    async fn inspect_dest(&mut self, destination_b64: &str) -> Option<String> {
        let response = self
            .command(&format!("INSPECT_DEST {destination_b64}"))
            .await;
        if response.starts_with("DEST_INFO ") {
            Some(response)
        } else {
            None
        }
    }

    async fn write_stream(&mut self, id: usize, payload: &[u8]) -> bool {
        self.command(&format!("WRITE {id} {}", hex_encode(payload)))
            .await
            .starts_with("WROTE ")
    }

    async fn read_stream(&mut self, id: usize, size: usize) -> Option<Vec<u8>> {
        let response = self.command(&format!("READ {id} {size}")).await;
        let mut fields = response.split_whitespace();
        if fields.next() != Some("READ") {
            return None;
        }
        let observed: usize = fields.next()?.parse().ok()?;
        let bytes = hex_decode(fields.next()?)?;
        (observed == size && bytes.len() == size).then_some(bytes)
    }

    async fn stream_eof(&mut self, id: usize) -> bool {
        self.command(&format!("EOF {id}")).await == "EOF"
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn hex_decode(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&text[index..index + 2], 16).ok())
        .collect()
}

/// Decodes a Java/I2P destination token through the Plan 142 wire
/// codec (I2P alphabet `-~` with `=` padding, frozen against Java I2P
/// `SAM` source, i2pd `Base.cpp`, and i2plib vectors). The
/// filename-oriented `i2pr_netdb::base64` codec uses a different
/// alphabet and must never decode wire destinations.
fn decode_destination_b64(token: &str) -> Vec<u8> {
    i2pr_api::sam::base64::decode(token, 4096).expect("decode destination")
}

fn record_stop(dir: &Path, detail: &str) {
    append_evidence(dir, "plan199-java-stop", detail);
}

/// Plan 201 §A — inbound I2NP message decoder that tries the standard
/// 16-byte header first, then falls back to the 9-byte short-transport
/// header. The first-family i2pd external driver only had to handle
/// the short-transport form because Plan 161 i2pd sends reply bodies
/// in that form. Java I2P 2.13.0 sends `DatabaseStore` responses to
/// `DatabaseLookup` in the standard 16-byte form regardless of the
/// inbound payload size, so a probe that only tries
/// `decode_short_transport` misinterprets Java's reply header as the
/// first 9 bytes of the body and shifts every subsequent field, which
/// in turn produces the
/// `Truncated { offset: 387, needed: 49858, remaining: 59 }` shape the
/// first counted Plan 200 §B run surfaced.
///
/// This helper mirrors the production inbound decoder at
/// `crates/i2pr-daemon/src/router_i2np.rs::dispatch_router_i2np`
/// (standard first, short-transport fallback) and is the single
/// place the Plan 201 Branch A corrective repairs the inbound decode
/// gap for the second-family lane.
fn decode_inbound_i2np(bytes: &[u8]) -> Result<I2npMessage, i2pr_proto::CodecError> {
    match I2npMessage::decode_standard(bytes, MAX_I2NP_PAYLOAD_SIZE) {
        Ok(message) => Ok(message),
        Err(standard_err) => {
            match I2npMessage::decode_short_transport(bytes, MAX_I2NP_PAYLOAD_SIZE) {
                Ok(message) => Ok(message),
                Err(_) => Err(standard_err),
            }
        }
    }
}

fn gzip_member(bytes: &[u8]) -> Vec<u8> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(bytes).expect("gzip RouterInfo");
    encoder.finish().expect("finish RouterInfo gzip")
}

fn database_store_router_info_wire(key: Hash, router_info: &[u8], message_id: u32) -> Vec<u8> {
    let payload = DeferredPayload::new(gzip_member(router_info), usize::from(u16::MAX))
        .expect("deferred RouterInfo payload");
    let store = DatabaseStoreMessage {
        key,
        reply_token: 0,
        reply_tunnel_id: None,
        reply_gateway: None,
        data: DatabaseStoreData::RouterInfoCompressed(payload),
    };
    I2npMessage::new_short_transport(
        message_id,
        wall_secs().saturating_add(60).min(u64::from(u32::MAX)) as u32,
        I2npBody::DatabaseStore(Box::new(store)),
    )
    .expect("RouterInfo store message")
    .encode_short_transport_to_vec(MAX_I2NP_MESSAGE_BYTES)
    .expect("encode RouterInfo store message")
}

/// Plan 200 §B — build a short-transport DatabaseLookup for a RouterInfo
/// hash. Direct delivery (`reply_tunnel_id = None`, no tunnel
/// gateway) so Java replies on the same authenticated session that
/// received the lookup. The `from` field is the i2pr-side RouterHash so
/// the responder knows where to send the reply. Excluded peers is
/// empty — the lookup is the first probe against the target router.
fn database_lookup_router_info_wire(key: Hash, from: Hash, message_id: u32) -> Vec<u8> {
    let lookup = DatabaseLookupMessage {
        key,
        from,
        delivery_flag: false,
        reply_tunnel_id: None,
        lookup_type: 2, // RouterInfo lookup
        excluded_peers: Vec::new(),
        reply_encryption: ReplyEncryption::None,
    };
    I2npMessage::new_short_transport(
        message_id,
        wall_secs().saturating_add(60).min(u64::from(u32::MAX)) as u32,
        I2npBody::DatabaseLookup(Box::new(lookup)),
    )
    .expect("RouterInfo lookup message")
    .encode_short_transport_to_vec(MAX_I2NP_MESSAGE_BYTES)
    .expect("encode RouterInfo lookup message")
}

/// Plan 200 §B.2 — probe the peer's main NetDB through an ordinary
/// `DatabaseLookup` and pump inbound until a RouterInfo-bearing
/// `DatabaseStore` arrives whose key/identity match the expected
/// peer. Returns `true` only when the full round-trip evidence is
/// decoded and validated; the function never claims proof from a
/// `DeliveryStatus` alone.
///
/// Sanitized fact recorded:
/// - `p200-routerinfo-lookup-<from>-wants-<expected>`: response
///   observed (`true`/`false`), response type, key match, signature
///   match, identity match, identity-hash match, advertised SSU2
///   port match.
const P200_LOOKUP_TIMEOUT: Duration = Duration::from_secs(20);

#[allow(clippy::too_many_arguments)]
async fn probe_routerinfo_lookup(
    handle: &mut i2pr_daemon::router_i2np::Ssu2DaemonHandle,
    target_peer: i2pr_transport::PeerId,
    expected_hash: i2pr_proto::Hash,
    expected_ri: &[u8],
    lookup_message_id: u32,
    from_hash: i2pr_proto::Hash,
    evidence_dir: &Path,
    probe_label: &str,
) -> bool {
    let lookup_wire = database_lookup_router_info_wire(
        Hash::from_bytes(*expected_hash.as_bytes()),
        Hash::from_bytes(*from_hash.as_bytes()),
        lookup_message_id,
    );
    let request = RouterDeliveryRequest::new(target_peer, lookup_wire, DELIVERY_TIMEOUT)
        .expect("RouterInfo lookup request");
    let admission = handle
        .delivery()
        .deliver(request, &CancellationToken::new());
    if admission != RouterDeliveryOutcome::Accepted {
        append_evidence(
            evidence_dir,
            &format!("p200-routerinfo-lookup-{probe_label}"),
            &format!("admission=rejected outcome={admission:?}"),
        );
        return false;
    }
    append_evidence(
        evidence_dir,
        &format!("p200-routerinfo-lookup-{probe_label}-admitted"),
        "true",
    );

    let deadline = tokio::time::Instant::now() + P200_LOOKUP_TIMEOUT;
    let mut pump_errors: u64 = 0;
    let mut pump_others: u64 = 0;
    let mut observed = false;
    while tokio::time::Instant::now() < deadline && !observed {
        let next = tokio::time::timeout(POLL_INTERVAL, handle.next_inbound()).await;
        let Ok(Some(inbound)) = next else {
            continue;
        };
        let message = match decode_inbound_i2np(&inbound.bytes) {
            Ok(message) => message,
            Err(_) => {
                pump_errors += 1;
                continue;
            }
        };
        let store = match message.body() {
            I2npBody::DatabaseStore(store) => store,
            _ => {
                pump_others += 1;
                continue;
            }
        };
        if store.key != Hash::from_bytes(*expected_hash.as_bytes()) {
            pump_others += 1;
            continue;
        }
        let compressed = match &store.data {
            DatabaseStoreData::RouterInfoCompressed(payload) => payload,
            _ => {
                pump_others += 1;
                continue;
            }
        };
        let payload_bytes = compressed.as_bytes();
        // Plan 201 §A — Java I2P 2.13.0 stores the RouterInfo in the
        // gzipped `RouterInfoCompressed` form exactly the same way i2pd
        // 2.61.0 does. `RouterInfo::decode` expects uncompressed bytes,
        // so the probe must gunzip the payload before parsing. The
        // i2pd first-family driver worked because it went through the
        // Plan 184 inbound dispatcher, which decompresses on the way in;
        // this probe is a lower-level black-box read that has to do the
        // decompression itself.
        let mut decoder = flate2::read::GzDecoder::new(payload_bytes);
        let mut decompressed = Vec::with_capacity(payload_bytes.len() * 4);
        if std::io::Read::read_to_end(&mut decoder, &mut decompressed).is_err() {
            pump_errors += 1;
            continue;
        }
        let decoded = match RouterInfo::decode(
            &decompressed,
            i2pr_runtime::constants::MAX_ESTABLISHMENT_ROUTER_INFO_BYTES,
        ) {
            Ok(router_info) => router_info,
            Err(_) => {
                pump_errors += 1;
                continue;
            }
        };
        let identity_hash = decoded
            .router_identity()
            .hash()
            .expect("RouterInfo identity hash");
        let identity_match = identity_hash.as_bytes() == expected_hash.as_bytes();
        let advertised_ssu2_count = decoded
            .addresses()
            .iter()
            .filter(|addr| addr.transport_style() == "SSU2")
            .count();
        let has_ssu2 = advertised_ssu2_count >= 1;
        let payload_len = decoded.encode_to_vec(65535).map(|b| b.len()).unwrap_or(0);
        let expected_len = expected_ri.len();
        let payload_match = payload_len == expected_len;
        append_evidence(
            evidence_dir,
            &format!("p200-routerinfo-lookup-{probe_label}"),
            &format!(
                "response_observed=true key_match=true identity_match={identity_match} ssu2_addresses={advertised_ssu2_count} decoded_payload_match={payload_match} decoded_payload_len={payload_len} expected_payload_len={expected_len} pump_errors={pump_errors} pump_others={pump_others}"
            ),
        );
        observed = identity_match && has_ssu2;
    }
    if !observed {
        append_evidence(
            evidence_dir,
            &format!("p200-routerinfo-lookup-{probe_label}"),
            &format!(
                "response_observed=false key_match=false pump_errors={pump_errors} pump_others={pump_others}"
            ),
        );
    }
    observed
}

/// Plan 200 §11 — emit exactly one terminal classification
/// `P200-{A..H}` per run. The earliest observed boundary wins. The
/// caller passes a `phase_results` slice of `(boundary_label,
/// observed_passed)` pairs in the order they were tested; the first
/// non-passing boundary determines the classification. If everything
/// passes, the classification is `P200-H-publication-path-passed`.
fn record_p200_classification(evidence_dir: &Path, phase_results: &[(&str, bool)]) -> &'static str {
    let classification =
        if let Some((boundary, _)) = phase_results.iter().find(|(_, passed)| !passed) {
            match *boundary {
                "java-main-netdb-a-knows-b" => "P200-A-router-a-missing-router-b",
                "java-main-netdb-b-knows-a" => "P200-B-router-b-missing-router-a",
                "java-client-ls2-not-created" => "P200-C-client-ls2-not-created-or-current",
                "java-client-tunnel-publication-path" => {
                    "P200-D-client-tunnel-publication-path-unavailable"
                }
                "java-floodfill-candidate" => "P200-E-no-eligible-floodfill-candidate",
                "java-store-emitted" => "P200-F-store-sent-no-ack",
                "java-network-visible-leaseset" => "P200-G-store-acked-remote-lookup-fails",
                _ => "P200-H-publication-path-passed",
            }
        } else {
            "P200-H-publication-path-passed"
        };
    let detail = phase_results
        .iter()
        .map(|(label, passed)| format!("{label}={passed}"))
        .collect::<Vec<_>>()
        .join(" ");
    append_evidence(
        evidence_dir,
        "p200-classification",
        &format!("{classification} {detail}"),
    );
    classification
}

// ---- Plan 220 — corrected diagnostic attribution ---------------------------
// Plan 219's J219-{A..J} attribution is superseded (D220-1..D220-9):
// pre-bootstrap facts fed the classifier, the Router B hash came
// from a raw `[16:386]` RouterInfo slice plus standard Base64, the
// stored-B-`f` fact checked presence only, selector output was
// synthesized from PeerManager membership, client-NetDB/OCMOSJ
// facts were hard-coded defaults, and forward `reference-received`
// was misused as reverse dispatch evidence.
//
// Plan 220 replaces that path with tri-state facts observed by the
// driver itself at its authoritative epoch
// (`post-driver-bootstrap/pre-reverse-send` for Java state,
// `post-reverse-send` for dispatch), protocol-correct hex
// RouterHash identity with a Rust/Java cross-check, exact
// stored-RouterInfo evidence, a read-only same-package
// FloodfillPeerSelector probe distinct from PeerManager
// membership, and directionally correct dispatch evidence.
// Unknown state stays Unknown; it is never converted to false,
// zero, or a root-cause classification.

/// Plan 220 authoritative Java-state observation epoch: after the
/// driver's own A/B RouterInfo DatabaseStore bootstrap (which
/// includes the B→A submission Plan 219 claimed was unexercised)
/// and before the helper reverse SEND begins.
const P220_EPOCH_AUTHORITATIVE: &str = "post-driver-bootstrap/pre-reverse-send";
/// Plan 220 dispatch observation epoch: after the reverse-send wait
/// window expires.
const P220_EPOCH_DISPATCH: &str = "post-reverse-send";

/// Plan 220 tri-state observation. `Known` carries a value observed
/// at the authoritative epoch; `Unknown` carries the static reason
/// it could not be observed. Unknown MUST NEVER be converted to
/// false, zero, or a root-cause classification.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum P220Observed<T> {
    Known(T),
    Unknown(&'static str),
}

impl<T> P220Observed<T> {
    fn as_ref(&self) -> P220Observed<&T> {
        match self {
            Self::Known(value) => P220Observed::Known(value),
            Self::Unknown(reason) => P220Observed::Unknown(reason),
        }
    }
}

fn p220_bytes_to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

/// Parses one `P220-EV <key>=<value> [...]` response line into its
/// key/value pairs. Double-quoted values may contain spaces
/// (capabilities strings). Returns an empty map for error lines or
/// unparseable input; callers record Unknown on an empty map.
fn p220_parse_kv(line: &str) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    let body = match line.strip_prefix("P220-EV ") {
        Some(rest) => rest,
        None => return map,
    };
    let mut key = String::new();
    let mut value = String::new();
    let mut in_key = true;
    let mut in_quotes = false;
    let mut has_pair = false;
    let mut flush = |key: &mut String, value: &mut String, has_pair: &mut bool| {
        if *has_pair && !key.is_empty() {
            map.insert(std::mem::take(key), std::mem::take(value));
        }
        *has_pair = false;
    };
    for ch in body.chars() {
        if in_key {
            if ch == '=' {
                in_key = false;
                has_pair = true;
            } else if ch == ' ' {
                key.clear();
                has_pair = false;
            } else {
                key.push(ch);
            }
        } else if in_quotes {
            if ch == '"' {
                in_quotes = false;
            } else {
                value.push(ch);
            }
        } else if ch == '"' && value.is_empty() {
            in_quotes = true;
        } else if ch == ' ' {
            flush(&mut key, &mut value, &mut has_pair);
            in_key = true;
        } else {
            value.push(ch);
        }
    }
    flush(&mut key, &mut value, &mut has_pair);
    map
}

/// Bounded read-only query against one controlled-router P220
/// diagnostic port (loopback only). Any connection, write, read,
/// timeout, or UTF-8 failure yields `None` so the caller records
/// Unknown — a missing response is never a protocol fact.
async fn p220_query_diagnostic(port: u16, command: &str) -> Option<String> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let stream = tokio::time::timeout(Duration::from_secs(5), tokio::net::TcpStream::connect(addr))
        .await
        .ok()?
        .ok()?;
    let (reader, mut writer) = stream.into_split();
    let mut reader = tokio::io::BufReader::new(reader);
    tokio::time::timeout(
        Duration::from_secs(5),
        writer.write_all(format!("{command}\n").as_bytes()),
    )
    .await
    .ok()?
    .ok()?;
    let mut line = Vec::new();
    let read = tokio::time::timeout(Duration::from_secs(5), reader.read_until(b'\n', &mut line))
        .await
        .ok()?
        .ok()?;
    if read == 0 || read > 65536 {
        return None;
    }
    String::from_utf8(line)
        .ok()
        .map(|s| s.trim_end().to_owned())
}

/// Plan 220 authoritative facts. Java-state fields are observed at
/// [`P220_EPOCH_AUTHORITATIVE`]; dispatch fields at
/// [`P220_EPOCH_DISPATCH`]. Client-NetDB / OCMOSJ per-message
/// fields have no read-only observation path within the test
/// constraints, so the driver records them Unknown with an
/// explicit reason instead of defaulting them false (D220-6).
#[derive(Clone, Debug)]
struct P220Facts {
    rust_b_hash_hex: String,
    java_b_self_hex: P220Observed<String>,
    java_b_self_b64: P220Observed<String>,
    hash_match: P220Observed<bool>,
    b_live_has_f: P220Observed<bool>,
    b_live_sha256: P220Observed<String>,
    b_live_published: P220Observed<u64>,
    a_stored_present: P220Observed<bool>,
    a_stored_identity_match: P220Observed<bool>,
    a_stored_sha256: P220Observed<String>,
    a_stored_published: P220Observed<u64>,
    a_stored_has_f: P220Observed<bool>,
    a_main_router_count: P220Observed<u64>,
    a_peermanager_b_indexed: P220Observed<bool>,
    selector_observable: P220Observed<bool>,
    selector_kbucket_size: P220Observed<u64>,
    selector_count: P220Observed<u64>,
    selector_contains_b: P220Observed<bool>,
    client_lookup_started: P220Observed<bool>,
    client_lookup_peer_selected: P220Observed<bool>,
    client_lookup_succeeded: P220Observed<bool>,
    target_leaseset_present: P220Observed<bool>,
    target_lease_selected: P220Observed<bool>,
    outbound_client_tunnel_selected: P220Observed<bool>,
    garlic_constructed: P220Observed<bool>,
    tunnel_dispatch_submitted: P220Observed<bool>,
    forward_i2pr_to_java_received: P220Observed<bool>,
    reverse_java_send_admitted: P220Observed<bool>,
    reverse_i2pr_tunneldata_observed: P220Observed<bool>,
    reverse_i2pr_payload_recovered: P220Observed<bool>,
}

/// Plan 220 terminal outcome: either a correctly observed
/// root-cause boundary, a typed observability gap at the first
/// stage that cannot be observed within the test constraints, or
/// the qualified reverse-delivery pass.
#[derive(Clone, Debug, PartialEq, Eq)]
enum P220Terminal {
    CorrectedAttribution(&'static str),
    ObservabilityGap(&'static str),
    ReverseDeliveryPassed,
}

impl P220Terminal {
    fn token(&self) -> String {
        match self {
            Self::CorrectedAttribution(boundary) => {
                format!("P220-CORRECTED-ATTRIBUTION {boundary}")
            }
            Self::ObservabilityGap(stage) => format!("P220-OBSERVABILITY-GAP-{stage}"),
            Self::ReverseDeliveryPassed => "P220-REVERSE-DELIVERY-PASSED".to_owned(),
        }
    }
}

impl P220Facts {
    /// Derives exactly one terminal outcome. A root-cause boundary
    /// fires only when every earlier boundary is `Known(pass)` and
    /// the current boundary is `Known(fail)`; the first `Unknown`
    /// stage emits its observability gap instead (Plan 220 §11).
    fn derive(&self) -> P220Terminal {
        // Stage HASH — protocol-correct identity cross-check. A
        // mismatch is a diagnostic failure, never "A lacks B".
        match self.hash_match.as_ref() {
            P220Observed::Unknown(_) => {
                return P220Terminal::ObservabilityGap("HASH");
            }
            P220Observed::Known(false) => {
                return P220Terminal::ObservabilityGap("HASH");
            }
            P220Observed::Known(true) => {}
        }
        // Stage A-STORED-RI — exact stored-RouterInfo evidence.
        match self.b_live_has_f.as_ref() {
            P220Observed::Unknown(_) => {
                return P220Terminal::ObservabilityGap("A-STORED-RI");
            }
            P220Observed::Known(false) => {
                return P220Terminal::CorrectedAttribution("B-LIVE-RI-NOT-F");
            }
            P220Observed::Known(true) => {}
        }
        match self.a_stored_present.as_ref() {
            P220Observed::Unknown(_) => {
                return P220Terminal::ObservabilityGap("A-STORED-RI");
            }
            P220Observed::Known(false) => {
                return P220Terminal::CorrectedAttribution("A-LACKS-B-RI");
            }
            P220Observed::Known(true) => {}
        }
        match self.a_stored_identity_match.as_ref() {
            P220Observed::Known(true) => {}
            _ => return P220Terminal::ObservabilityGap("A-STORED-RI"),
        }
        match self.a_stored_has_f.as_ref() {
            P220Observed::Unknown(_) => {
                return P220Terminal::ObservabilityGap("A-STORED-RI");
            }
            P220Observed::Known(false) => {
                return P220Terminal::CorrectedAttribution("A-STORED-B-RI-NOT-F");
            }
            P220Observed::Known(true) => {}
        }
        // Stage PEERMANAGER — membership is observed; the
        // banlist/explicit-ignore state has no public read-only
        // accessor, so an absent B cannot be distinguished from a
        // banned B and stays a gap rather than a root cause.
        match self.a_peermanager_b_indexed.as_ref() {
            P220Observed::Unknown(_) => {
                return P220Terminal::ObservabilityGap("PEERMANAGER");
            }
            P220Observed::Known(false) => {
                return P220Terminal::ObservabilityGap("PEERMANAGER");
            }
            P220Observed::Known(true) => {}
        }
        // Stage SELECTOR — actual FloodfillPeerSelector output from
        // the same-package probe, never synthesized from PeerManager
        // membership.
        match self.selector_observable.as_ref() {
            P220Observed::Known(true) => {}
            _ => return P220Terminal::ObservabilityGap("SELECTOR"),
        }
        match self.selector_contains_b.as_ref() {
            P220Observed::Unknown(_) => {
                return P220Terminal::ObservabilityGap("SELECTOR");
            }
            P220Observed::Known(false) => {
                return P220Terminal::CorrectedAttribution("SELECTOR-EXCLUDES-B");
            }
            P220Observed::Known(true) => {}
        }
        // Reverse-delivery fast path: a digest-matched payload
        // recovered through the real inbound tunnel proves the
        // client-NetDB/OCMOSJ/dispatch chain passed on this run, so
        // downstream Unknowns are moot. This inference runs from
        // the observed outcome, never from a forward-direction row.
        if matches!(
            self.reverse_i2pr_payload_recovered.as_ref(),
            P220Observed::Known(true)
        ) {
            return P220Terminal::ReverseDeliveryPassed;
        }
        // Stage CLIENT-NETDB — per-message Java lookup state has no
        // read-only observation path within the test constraints.
        for stage in [
            self.client_lookup_started.as_ref(),
            self.client_lookup_peer_selected.as_ref(),
            self.client_lookup_succeeded.as_ref(),
            self.target_leaseset_present.as_ref(),
        ] {
            match stage {
                P220Observed::Known(true) => {}
                P220Observed::Known(false) => {
                    return P220Terminal::CorrectedAttribution("CLIENT-NETDB-LOOKUP-FAILED");
                }
                P220Observed::Unknown(_) => {
                    return P220Terminal::ObservabilityGap("CLIENT-NETDB");
                }
            }
        }
        // Stage OCMOSJ — same observability bound as CLIENT-NETDB.
        for stage in [
            self.target_lease_selected.as_ref(),
            self.outbound_client_tunnel_selected.as_ref(),
            self.garlic_constructed.as_ref(),
            self.tunnel_dispatch_submitted.as_ref(),
        ] {
            match stage {
                P220Observed::Known(true) => {}
                P220Observed::Known(false) => {
                    return P220Terminal::CorrectedAttribution("OCMOSJ-DISPATCH-NOT-SUBMITTED");
                }
                P220Observed::Unknown(_) => {
                    return P220Terminal::ObservabilityGap("OCMOSJ");
                }
            }
        }
        // Directionally correct dispatch tail: the helper admitted
        // the reverse send but no Java-side dispatch was observed
        // and no i2pr payload arrived.
        match self.reverse_java_send_admitted.as_ref() {
            P220Observed::Known(true) => {}
            P220Observed::Known(false) => {
                return P220Terminal::CorrectedAttribution("REVERSE-SEND-NOT-ADMITTED");
            }
            P220Observed::Unknown(_) => {
                return P220Terminal::ObservabilityGap("OCMOSJ");
            }
        }
        match self.reverse_i2pr_payload_recovered.as_ref() {
            P220Observed::Known(true) => P220Terminal::ReverseDeliveryPassed,
            // Payload absent with dispatch unobserved: the OCMOSJ
            // gap is the honest terminal — forward
            // `reference-received` MUST NOT satisfy this branch.
            _ => P220Terminal::ObservabilityGap("OCMOSJ"),
        }
    }

    fn detail(&self, terminal: &P220Terminal) -> String {
        fn flag<T: std::fmt::Debug>(observed: &P220Observed<T>) -> String {
            match observed {
                P220Observed::Known(value) => format!("known({value:?})"),
                P220Observed::Unknown(reason) => format!("unknown({reason})"),
            }
        }
        format!(
            "{} java_state_epoch={} dispatch_epoch={} rust_b_hash_hex={} java_b_self_hex={} hash_match={} \
             b_live_has_f={} b_live_sha256={} b_live_published={} a_stored_present={} a_stored_identity_match={} \
             a_stored_sha256={} a_stored_published={} a_stored_has_f={} a_main_router_count={} \
             a_peermanager_b_indexed={} selector_observable={} selector_kbucket_size={} selector_count={} \
             selector_contains_b={} client_lookup_started={} client_lookup_peer_selected={} \
             client_lookup_succeeded={} target_leaseset_present={} target_lease_selected={} \
             outbound_client_tunnel_selected={} garlic_constructed={} tunnel_dispatch_submitted={} \
             forward_i2pr_to_java_received={} reverse_java_send_admitted={} \
             reverse_i2pr_tunneldata_observed={} reverse_i2pr_payload_recovered={}",
            terminal.token(),
            P220_EPOCH_AUTHORITATIVE,
            P220_EPOCH_DISPATCH,
            self.rust_b_hash_hex,
            flag(&self.java_b_self_hex),
            flag(&self.hash_match),
            flag(&self.b_live_has_f),
            flag(&self.b_live_sha256),
            flag(&self.b_live_published),
            flag(&self.a_stored_present),
            flag(&self.a_stored_identity_match),
            flag(&self.a_stored_sha256),
            flag(&self.a_stored_published),
            flag(&self.a_stored_has_f),
            flag(&self.a_main_router_count),
            flag(&self.a_peermanager_b_indexed),
            flag(&self.selector_observable),
            flag(&self.selector_kbucket_size),
            flag(&self.selector_count),
            flag(&self.selector_contains_b),
            flag(&self.client_lookup_started),
            flag(&self.client_lookup_peer_selected),
            flag(&self.client_lookup_succeeded),
            flag(&self.target_leaseset_present),
            flag(&self.target_lease_selected),
            flag(&self.outbound_client_tunnel_selected),
            flag(&self.garlic_constructed),
            flag(&self.tunnel_dispatch_submitted),
            flag(&self.forward_i2pr_to_java_received),
            flag(&self.reverse_java_send_admitted),
            flag(&self.reverse_i2pr_tunneldata_observed),
            flag(&self.reverse_i2pr_payload_recovered),
        )
    }
}

/// Collects the authoritative Java-state facts at
/// [`P220_EPOCH_AUTHORITATIVE`]: after the driver's own A/B
/// RouterInfo bootstrap, before the helper reverse SEND begins.
/// `rust_b_hex` is the protocol-derived Router B hash (validated
/// RouterInfo identity); `target_dest_hex` is the i2pr destination
/// hash the reverse lookup must resolve (selector routing key).
async fn p220_collect_authoritative(
    a_port: u16,
    b_port: u16,
    rust_b_hex: &str,
    target_dest_hex: &str,
) -> P220Facts {
    let unknown = || P220Facts {
        rust_b_hash_hex: rust_b_hex.to_owned(),
        java_b_self_hex: P220Observed::Unknown("snapshot-query-failed"),
        java_b_self_b64: P220Observed::Unknown("snapshot-query-failed"),
        hash_match: P220Observed::Unknown("snapshot-query-failed"),
        b_live_has_f: P220Observed::Unknown("snapshot-query-failed"),
        b_live_sha256: P220Observed::Unknown("snapshot-query-failed"),
        b_live_published: P220Observed::Unknown("snapshot-query-failed"),
        a_stored_present: P220Observed::Unknown("not-queried"),
        a_stored_identity_match: P220Observed::Unknown("not-queried"),
        a_stored_sha256: P220Observed::Unknown("not-queried"),
        a_stored_published: P220Observed::Unknown("not-queried"),
        a_stored_has_f: P220Observed::Unknown("not-queried"),
        a_main_router_count: P220Observed::Unknown("not-queried"),
        a_peermanager_b_indexed: P220Observed::Unknown("not-queried"),
        selector_observable: P220Observed::Unknown("not-queried"),
        selector_kbucket_size: P220Observed::Unknown("not-queried"),
        selector_count: P220Observed::Unknown("not-queried"),
        selector_contains_b: P220Observed::Unknown("not-queried"),
        client_lookup_started: P220Observed::Unknown("no-read-only-per-message-observation"),
        client_lookup_peer_selected: P220Observed::Unknown("no-read-only-per-message-observation"),
        client_lookup_succeeded: P220Observed::Unknown("no-read-only-per-message-observation"),
        target_leaseset_present: P220Observed::Unknown("no-read-only-per-message-observation"),
        target_lease_selected: P220Observed::Unknown("no-read-only-per-message-observation"),
        outbound_client_tunnel_selected: P220Observed::Unknown(
            "no-read-only-per-message-observation",
        ),
        garlic_constructed: P220Observed::Unknown("no-read-only-per-message-observation"),
        tunnel_dispatch_submitted: P220Observed::Unknown("no-read-only-per-message-observation"),
        forward_i2pr_to_java_received: P220Observed::Unknown("dispatch-epoch-not-reached"),
        reverse_java_send_admitted: P220Observed::Unknown("dispatch-epoch-not-reached"),
        reverse_i2pr_tunneldata_observed: P220Observed::Unknown("dispatch-epoch-not-reached"),
        reverse_i2pr_payload_recovered: P220Observed::Unknown("dispatch-epoch-not-reached"),
    };
    let mut facts = unknown();

    // Router B self snapshot: the protocol-derived Rust hash must
    // equal Java's own RouterHash view, and the query target used
    // against A below is that same value (D220-2 cross-check).
    if let Some(line) = p220_query_diagnostic(b_port, "P220-SNAPSHOT").await {
        let kv = p220_parse_kv(&line);
        let present = kv.get("self_ri_present").is_some_and(|v| v == "true");
        if present {
            if let Some(hex) = kv.get("self_router_hash_hex") {
                facts.java_b_self_hex = P220Observed::Known(hex.clone());
                facts.hash_match = P220Observed::Known(hex == rust_b_hex);
            }
            if let Some(b64) = kv.get("self_router_hash_b64") {
                facts.java_b_self_b64 = P220Observed::Known(b64.clone());
            }
            if let Some(has_f) = kv.get("self_has_floodfill_capability") {
                facts.b_live_has_f = P220Observed::Known(has_f == "true");
            }
            if let Some(sha) = kv.get("self_routerinfo_sha256") {
                facts.b_live_sha256 = P220Observed::Known(sha.clone());
            }
            if let Some(published) = kv
                .get("self_published_seconds")
                .and_then(|v| v.parse::<u64>().ok())
            {
                facts.b_live_published = P220Observed::Known(published);
            }
        } else {
            facts.java_b_self_hex = P220Observed::Unknown("b-self-ri-absent");
            facts.hash_match = P220Observed::Unknown("b-self-ri-absent");
            facts.b_live_has_f = P220Observed::Unknown("b-self-ri-absent");
        }
    }

    // Router A view of Router B: exact stored-RouterInfo evidence
    // (D220-4). Presence alone never implies `f`.
    if let Some(line) = p220_query_diagnostic(a_port, &format!("P220-STORED-RI {rust_b_hex}")).await
    {
        let kv = p220_parse_kv(&line);
        let present = kv.get("present").is_some_and(|v| v == "true");
        facts.a_stored_present = P220Observed::Known(present);
        if present {
            let identity_match = kv.get("stored_identity_match").is_some_and(|v| v == "true")
                && kv
                    .get("stored_router_hash_hex")
                    .is_some_and(|v| v == rust_b_hex);
            facts.a_stored_identity_match = P220Observed::Known(identity_match);
            facts.a_stored_sha256 = kv
                .get("routerinfo_sha256")
                .map_or(P220Observed::Unknown("stored-sha-absent"), |v| {
                    P220Observed::Known(v.clone())
                });
            facts.a_stored_published = kv
                .get("published_seconds")
                .and_then(|v| v.parse::<u64>().ok())
                .map_or(
                    P220Observed::Unknown("stored-published-absent"),
                    P220Observed::Known,
                );
            facts.a_stored_has_f = kv
                .get("has_floodfill_capability")
                .map_or(P220Observed::Unknown("stored-caps-absent"), |v| {
                    P220Observed::Known(v == "true")
                });
        } else {
            facts.a_stored_identity_match = P220Observed::Unknown("a-lacks-b-record");
            facts.a_stored_sha256 = P220Observed::Unknown("a-lacks-b-record");
            facts.a_stored_published = P220Observed::Unknown("a-lacks-b-record");
            facts.a_stored_has_f = P220Observed::Unknown("a-lacks-b-record");
        }
    }

    // Router A PeerManager `f` membership vs the actual selector
    // output (D220-5): two distinct observations, never one
    // synthesized from the other.
    if let Some(line) = p220_query_diagnostic(a_port, "P220-PEERS-FLOODFILL").await {
        let kv = p220_parse_kv(&line);
        let peers: Vec<&str> = kv
            .iter()
            .filter(|(k, _)| k.starts_with("peer_") && k.ends_with("_hex"))
            .map(|(_, v)| v.as_str())
            .collect();
        if kv.contains_key("count") {
            facts.a_peermanager_b_indexed = P220Observed::Known(peers.contains(&rust_b_hex));
        } else {
            facts.a_peermanager_b_indexed = P220Observed::Unknown("peermanager-query-failed");
        }
    }
    if let Some(line) = p220_query_diagnostic(a_port, "P220-MAIN-ROUTER-COUNT").await {
        let kv = p220_parse_kv(&line);
        facts.a_main_router_count = kv.get("count").and_then(|v| v.parse::<u64>().ok()).map_or(
            P220Observed::Unknown("main-router-count-query-failed"),
            P220Observed::Known,
        );
    }
    if let Some(line) = p220_query_diagnostic(
        a_port,
        &format!("P220-SELECTOR {target_dest_hex} {rust_b_hex}"),
    )
    .await
    {
        let kv = p220_parse_kv(&line);
        let observable = kv.get("observable").is_some_and(|v| v == "true");
        facts.selector_observable = P220Observed::Known(observable);
        if observable {
            facts.selector_kbucket_size = kv
                .get("selector_input_kbucket_size")
                .and_then(|v| v.parse::<u64>().ok())
                .map_or(
                    P220Observed::Unknown("selector-kbucket-size-absent"),
                    P220Observed::Known,
                );
            facts.selector_count = kv
                .get("selector_count")
                .and_then(|v| v.parse::<u64>().ok())
                .map_or(
                    P220Observed::Unknown("selector-count-absent"),
                    P220Observed::Known,
                );
            facts.selector_contains_b = kv
                .get("selector_contains_b")
                .map_or(P220Observed::Unknown("selector-membership-absent"), |v| {
                    P220Observed::Known(v == "true")
                });
        } else {
            facts.selector_kbucket_size = P220Observed::Unknown("selector-unavailable");
            facts.selector_count = P220Observed::Unknown("selector-unavailable");
            facts.selector_contains_b = P220Observed::Unknown("selector-unavailable");
        }
    }
    facts
}

/// Emits exactly one `p220-classification` row plus the supporting
/// Plan 220 §23 evidence rows (hash cross-check, epoch timeline,
/// exact A-stored-B evidence, PeerManager-vs-selector evidence,
/// directional dispatch evidence). Returns the terminal token.
fn record_p220_classification(evidence_dir: &Path, facts: &P220Facts) -> String {
    let terminal = facts.derive();
    append_evidence(
        evidence_dir,
        "p220-routerhash-crosscheck",
        &format!(
            "rust_b_hash_hex={} java_b_self_hex={:?} hash_match={:?} epoch={}",
            facts.rust_b_hash_hex,
            facts.java_b_self_hex,
            facts.hash_match,
            P220_EPOCH_AUTHORITATIVE,
        ),
    );
    append_evidence(
        evidence_dir,
        "p220-epoch-timeline",
        &format!(
            "java_state_epoch={} dispatch_epoch={} classifier_consumes_only_authoritative_or_newer=true",
            P220_EPOCH_AUTHORITATIVE, P220_EPOCH_DISPATCH,
        ),
    );
    append_evidence(
        evidence_dir,
        "p220-a-stored-b-evidence",
        &format!(
            "present={:?} identity_match={:?} stored_sha256={:?} stored_published={:?} stored_has_f={:?} \
             live_sha256={:?} live_published={:?} live_has_f={:?}",
            facts.a_stored_present,
            facts.a_stored_identity_match,
            facts.a_stored_sha256,
            facts.a_stored_published,
            facts.a_stored_has_f,
            facts.b_live_sha256,
            facts.b_live_published,
            facts.b_live_has_f,
        ),
    );
    append_evidence(
        evidence_dir,
        "p220-peermanager-vs-selector",
        &format!(
            "peermanager_b_indexed={:?} selector_observable={:?} selector_kbucket_size={:?} \
             selector_count={:?} selector_contains_b={:?} selector_synthesized_from_peermanager=false",
            facts.a_peermanager_b_indexed,
            facts.selector_observable,
            facts.selector_kbucket_size,
            facts.selector_count,
            facts.selector_contains_b,
        ),
    );
    append_evidence(
        evidence_dir,
        "p220-dispatch-direction",
        &format!(
            "forward_i2pr_to_java_received={:?} reverse_java_send_admitted={:?} \
             reverse_i2pr_tunneldata_observed={:?} reverse_i2pr_payload_recovered={:?} \
             forward_receipt_satisfies_reverse=false",
            facts.forward_i2pr_to_java_received,
            facts.reverse_java_send_admitted,
            facts.reverse_i2pr_tunneldata_observed,
            facts.reverse_i2pr_payload_recovered,
        ),
    );
    let detail = facts.detail(&terminal);
    append_evidence(
        evidence_dir,
        "p220-classification",
        &format!("{} {detail}", terminal.token()),
    );
    terminal.token()
}

// Plan 219 historical note: the superseded terminal
// attribution and its typed-fact classifier (backed by the
// removed production surface) are replaced by the Plan 220 P220
// module above (D220-1..D220-9). The old attribution is retained
// history only and MUST NOT feed any classifier.

/// Plan 199 §A.2 bootstrap-only probe. It runs before either public Java
/// helper is started, breaking the otherwise circular dependency where a
/// one-hop public client waits for the publication router while the router
/// waits for that client to become ready.
///
/// Plan 200 §B extends this probe with **positive post-bootstrap proof**
/// that the receiving Java main NetDB can serve the peer RouterInfo
/// through an ordinary `DatabaseLookup` round-trip in both
/// directions. The bootstrap submission alone is no longer considered
/// proof of network-visible installation.
#[tokio::test]
#[ignore = "Plan 199: requires the exact-pinned dual Java router environment"]
async fn bootstrap_java_router_peers() {
    let publication_ri = std::fs::read(env_path("JAVA_PUBLICATION_ROUTER_INFO"))
        .expect("read publication RouterInfo");
    let service_ri =
        std::fs::read(env_path("JAVA_SERVICE_ROUTER_INFO")).expect("read service RouterInfo");
    // Plan 201 Branch C/D corrective — Router C is the tunnel participant
    // that stock Java needs to build 1-hop client tunnels.
    let tunnel_participant_ri = std::fs::read(env_path("JAVA_TUNNEL_PARTICIPANT_ROUTER_INFO"))
        .expect("read tunnel-participant RouterInfo");
    let publication_endpoint: SocketAddr = env_value("JAVA_PUBLICATION_SSU2_ENDPOINT")
        .parse()
        .expect("publication endpoint");
    let service_endpoint: SocketAddr = env_value("JAVA_SERVICE_SSU2_ENDPOINT")
        .parse()
        .expect("service endpoint");
    let tunnel_participant_endpoint: SocketAddr =
        env_value("JAVA_TUNNEL_PARTICIPANT_SSU2_ENDPOINT")
            .parse()
            .expect("tunnel-participant endpoint");
    let bind: SocketAddr = env_value("I2PR_SSU2_BIND").parse().expect("bind");
    assert!(bind.ip().is_loopback());
    assert!(publication_endpoint.ip().is_loopback());
    assert!(service_endpoint.ip().is_loopback());
    assert!(tunnel_participant_endpoint.ip().is_loopback());
    let evidence_dir = env_path("EVIDENCE_DIR");
    std::fs::create_dir_all(&evidence_dir).expect("evidence dir");
    let (publication_hash, publication_ssu2) =
        verify_reference_router_info(&publication_ri).expect("verify publication RouterInfo");
    let (service_hash, service_ssu2) =
        verify_reference_router_info(&service_ri).expect("verify service RouterInfo");
    let (tunnel_participant_hash, tunnel_participant_ssu2) =
        verify_reference_router_info(&tunnel_participant_ri)
            .expect("verify tunnel-participant RouterInfo");
    let publication_material = publication_ssu2
        .address_material()
        .expect("publication keys");
    let service_material = service_ssu2.address_material().expect("service keys");
    let tunnel_participant_material = tunnel_participant_ssu2
        .address_material()
        .expect("tunnel-participant keys");
    let publication_target = daemon_dial_target(
        publication_hash,
        publication_endpoint,
        i2pr_runtime::Ssu2PublicKey::new(*publication_material.static_public_key().as_bytes())
            .expect("publication static key"),
        i2pr_runtime::IntroKey::new(*publication_material.intro_key().as_bytes()),
    )
    .expect("publication target");
    let service_target = daemon_dial_target(
        service_hash,
        service_endpoint,
        i2pr_runtime::Ssu2PublicKey::new(*service_material.static_public_key().as_bytes())
            .expect("service static key"),
        i2pr_runtime::IntroKey::new(*service_material.intro_key().as_bytes()),
    )
    .expect("service target");
    let tunnel_participant_target = daemon_dial_target(
        tunnel_participant_hash,
        tunnel_participant_endpoint,
        i2pr_runtime::Ssu2PublicKey::new(
            *tunnel_participant_material.static_public_key().as_bytes(),
        )
        .expect("tunnel-participant static key"),
        i2pr_runtime::IntroKey::new(*tunnel_participant_material.intro_key().as_bytes()),
    )
    .expect("tunnel-participant target");
    let bundle = i2pr_crypto::RouterIdentityBundle::generate(&mut rand_core::OsRng)
        .expect("bootstrap identity");
    let identity = generate_controlled_identity(&bundle, "127.0.0.1", bind.port())
        .expect("bootstrap identity material");
    let config_text = format!(
        "schema_version = 1\n[router]\ndata_dir = \"./state\"\n[ssu2]\nenabled = true\nbind_ipv4 = \"127.0.0.1\"\nport = {}\n",
        bind.port()
    );
    let config = Config::parse(&config_text).expect("bootstrap profile");
    let service = Ssu2DaemonService::new(&config.ssu2, identity).expect("bootstrap service");
    let token = CancellationToken::new();
    let scope = ChildScope::for_test(&token, ChildFailurePolicy::FailParent);
    let mut handle = service
        .start(&scope, &config.ssu2)
        .await
        .expect("bootstrap daemon");
    handle
        .dial(publication_target, DIAL_TIMEOUT, &CancellationToken::new())
        .await
        .expect("bootstrap publication session");
    handle
        .dial(service_target, DIAL_TIMEOUT, &CancellationToken::new())
        .await
        .expect("bootstrap service session");
    // Plan 201 Branch C/D corrective — also dial Router C.
    handle
        .dial(
            tunnel_participant_target,
            DIAL_TIMEOUT,
            &CancellationToken::new(),
        )
        .await
        .expect("bootstrap tunnel-participant session");
    let publication_wire = database_store_router_info_wire(
        Hash::from_bytes(*service_hash.as_bytes()),
        &service_ri,
        0x51A7_1201,
    );
    let service_wire = database_store_router_info_wire(
        Hash::from_bytes(*publication_hash.as_bytes()),
        &publication_ri,
        0x51A7_1202,
    );
    // Plan 201 Branch C/D corrective — send A's and B's RouterInfo to C,
    // and C's RouterInfo to A and B. Router C is a tunnel participant
    // (not floodfill) but still needs to know A and B for tunnel builds.
    let c_to_a_wire = database_store_router_info_wire(
        Hash::from_bytes(*publication_hash.as_bytes()),
        &publication_ri,
        0x51A7_1203,
    );
    let c_to_b_wire = database_store_router_info_wire(
        Hash::from_bytes(*service_hash.as_bytes()),
        &service_ri,
        0x51A7_1204,
    );
    let a_to_c_wire = database_store_router_info_wire(
        Hash::from_bytes(*tunnel_participant_hash.as_bytes()),
        &tunnel_participant_ri,
        0x51A7_1205,
    );
    let b_to_c_wire = database_store_router_info_wire(
        Hash::from_bytes(*tunnel_participant_hash.as_bytes()),
        &tunnel_participant_ri,
        0x51A7_1206,
    );
    for (peer, wire) in [
        (PeerId::from_hash(publication_hash), publication_wire),
        (PeerId::from_hash(service_hash), service_wire),
        (PeerId::from_hash(tunnel_participant_hash), c_to_a_wire),
        (PeerId::from_hash(tunnel_participant_hash), c_to_b_wire),
        (PeerId::from_hash(publication_hash), a_to_c_wire),
        (PeerId::from_hash(service_hash), b_to_c_wire),
    ] {
        let request = RouterDeliveryRequest::new(peer, wire, DELIVERY_TIMEOUT)
            .expect("bootstrap delivery request");
        assert_eq!(
            handle
                .delivery()
                .deliver(request, &CancellationToken::new()),
            RouterDeliveryOutcome::Accepted
        );
    }
    append_evidence(
        &evidence_dir,
        "java-router-peer-bootstrap-completed",
        "ordinary-authenticated-i2np-databasestore-three-routers",
    );

    // Plan 200 §B + Plan 201 Branch C/D — post-bootstrap positive lookup
    // proof in all directions. We use the i2pr-side RouterHash as the
    // lookup requester (`from`) so the responder knows where to send
    // the reply. Direct delivery (`reply_tunnel_id = None`) keeps the
    // probe on the existing authenticated session.
    let i2pr_router_hash = bundle.identity().hash().expect("i2pr router hash");
    let a_knows_b = probe_routerinfo_lookup(
        &mut handle,
        PeerId::from_hash(publication_hash),
        service_hash,
        &service_ri,
        0x51A7_1301,
        i2pr_router_hash,
        &evidence_dir,
        "a-knows-b",
    )
    .await;
    let b_knows_a = probe_routerinfo_lookup(
        &mut handle,
        PeerId::from_hash(service_hash),
        publication_hash,
        &publication_ri,
        0x51A7_1302,
        i2pr_router_hash,
        &evidence_dir,
        "b-knows-a",
    )
    .await;
    let c_knows_a = probe_routerinfo_lookup(
        &mut handle,
        PeerId::from_hash(tunnel_participant_hash),
        publication_hash,
        &publication_ri,
        0x51A7_1303,
        i2pr_router_hash,
        &evidence_dir,
        "c-knows-a",
    )
    .await;
    let c_knows_b = probe_routerinfo_lookup(
        &mut handle,
        PeerId::from_hash(tunnel_participant_hash),
        service_hash,
        &service_ri,
        0x51A7_1304,
        i2pr_router_hash,
        &evidence_dir,
        "c-knows-b",
    )
    .await;

    // Plan 200 §11 — terminal classification. The bootstrap probe
    // itself only reaches the A/B/C main-NetDB bootstrap boundary; the
    // C..H phases are recorded as blocked until Plan 201 (or this
    // run's destination/Streaming driver) proves them.
    record_p200_classification(
        &evidence_dir,
        &[
            ("java-main-netdb-a-knows-b", a_knows_b),
            ("java-main-netdb-b-knows-a", b_knows_a),
            ("java-main-netdb-c-knows-a", c_knows_a),
            ("java-main-netdb-c-knows-b", c_knows_b),
            ("java-client-ls2-not-created", false),
            ("java-client-tunnel-publication-path", false),
            ("java-floodfill-candidate", false),
            ("java-store-emitted", false),
            ("java-network-visible-leaseset", false),
        ],
    );

    handle.shutdown();
    let _ = scope.shutdown().await;
    assert_eq!(handle.snapshot().active_sessions, 0);
}

#[allow(clippy::too_many_arguments)]
async fn send_transport_request(
    request: &TransportSendRequest,
    routing: &DestinationRouting,
    session: &mut EciesSessionManager,
    outbound: &DestinationOutboundRole,
    local_identity: &DestinationIdentity,
    local_ls2: &i2pr_proto::LeaseSet2,
    delivery: &i2pr_daemon::router_i2np::RouterDeliveryService,
    rng: &mut ChaCha8Rng,
) {
    let plan = StreamingDestinationAdapter::send(
        request,
        routing,
        session,
        outbound,
        local_identity.id(),
        local_identity.static_secret_bytes(),
        local_ls2,
        u32::try_from(wall_secs()).unwrap_or(u32::MAX),
        wall_ms(),
        rng,
    )
    .expect("adapter send");
    let cell_dispatch = deliver_outbound_cells(
        &plan.cells,
        wall_ms() + 60_000,
        Deadline::new(Duration::from_secs(60)).expect("deadline"),
        rng,
    )
    .expect("encode streaming cells");
    for cell_delivery in &cell_dispatch.deliveries {
        let send = RouterDeliveryRequest::new(
            cell_delivery.target(),
            cell_delivery.message_bytes().to_vec(),
            DELIVERY_TIMEOUT,
        )
        .expect("delivery request");
        assert_eq!(
            delivery.deliver(send, &CancellationToken::new()),
            RouterDeliveryOutcome::Accepted
        );
    }
}

#[allow(clippy::too_many_arguments)]
async fn pump_one_streaming_inbound(
    handle: &mut i2pr_daemon::router_i2np::Ssu2DaemonHandle,
    coord: &mut ExploratoryBuildCoordinator,
    dest: &mut DestinationTunnelCoordinator,
    dispatcher: &mut DestinationDispatcher,
    session: &mut EciesSessionManager,
    routing: &mut DestinationRouting,
    streaming: &mut StreamingManager,
    outbound: &DestinationOutboundRole,
    local_identity: &DestinationIdentity,
    local_ls2: &i2pr_proto::LeaseSet2,
    delivery: &i2pr_daemon::router_i2np::RouterDeliveryService,
    rng: &mut ChaCha8Rng,
    from_hash: &[u8; 32],
    errors: &mut u64,
    _evidence_dir: &Path,
) -> bool {
    let next = tokio::time::timeout(POLL_INTERVAL, handle.next_inbound()).await;
    let Ok(Some(inbound)) = next else {
        return false;
    };
    let message = match decode_inbound_i2np(&inbound.bytes) {
        Ok(message) => message,
        Err(_) => {
            *errors += 1;
            return false;
        }
    };
    let cell = match message.body() {
        I2npBody::TunnelData(cell) => cell.clone(),
        _ => return false,
    };
    let bytes = match dest.recover_garlic_bytes(coord.registry_mut(), &cell, wall_ms()) {
        Ok(bytes) => bytes,
        Err(_) => return false,
    };
    let now_secs = u32::try_from(wall_secs()).unwrap_or(u32::MAX);
    dispatcher.dispatch_garlic_envelope(
        session,
        local_identity.id(),
        local_identity.static_secret_bytes(),
        &local_identity.static_public_bytes(),
        now_secs,
        &I2npMessage::decode_standard(&bytes, MAX_I2NP_PAYLOAD_SIZE).expect("decode garlic"),
        routing.lease_set2_store_mut(),
    );
    let Some(queued) = dispatcher.pop_payload(local_identity.id()) else {
        return false;
    };
    if StreamingDestinationAdapter::receive(
        queued.bytes(),
        local_identity,
        streaming,
        from_hash,
        wall_ms(),
    )
    .is_err()
    {
        *errors += 1;
        return false;
    }
    let pending = streaming.drain_outbound();
    for request in &pending {
        send_transport_request(
            request,
            routing,
            session,
            outbound,
            local_identity,
            local_ls2,
            delivery,
            rng,
        )
        .await;
    }
    drain_streaming_timers(
        streaming,
        routing,
        session,
        outbound,
        local_identity,
        local_ls2,
        delivery,
        rng,
        _evidence_dir,
    )
    .await;
    true
}

#[allow(clippy::too_many_arguments)]
async fn drain_streaming_timers(
    streaming: &mut StreamingManager,
    routing: &mut DestinationRouting,
    session: &mut EciesSessionManager,
    outbound: &DestinationOutboundRole,
    local_identity: &DestinationIdentity,
    local_ls2: &i2pr_proto::LeaseSet2,
    delivery: &i2pr_daemon::router_i2np::RouterDeliveryService,
    rng: &mut ChaCha8Rng,
    _evidence_dir: &Path,
) {
    let now_ms = wall_ms();
    for request in streaming.poll_acks(now_ms) {
        send_transport_request(
            &request,
            routing,
            session,
            outbound,
            local_identity,
            local_ls2,
            delivery,
            rng,
        )
        .await;
    }
    for request in streaming.poll_retransmits(now_ms) {
        send_transport_request(
            &request,
            routing,
            session,
            outbound,
            local_identity,
            local_ls2,
            delivery,
            rng,
        )
        .await;
    }
}

#[allow(clippy::too_many_arguments)]
async fn accept_inbound_syn_response(
    streaming: &mut StreamingManager,
    local_identity: &DestinationIdentity,
    routing: &DestinationRouting,
    session: &mut EciesSessionManager,
    outbound: &DestinationOutboundRole,
    local_ls2: &i2pr_proto::LeaseSet2,
    delivery: &i2pr_daemon::router_i2np::RouterDeliveryService,
    rng: &mut ChaCha8Rng,
) -> Option<i2pr_client::streaming::connection::ConnectionId> {
    if streaming.listener_backlog(0) < 1 {
        return None;
    }
    let inbound_id = streaming.accept(0).expect("accept inbound SYN");
    let (peer_bytes, peer_key, inbound_local, inbound_remote) = {
        let inbound = streaming
            .get_connection(inbound_id)
            .expect("inbound connection");
        (
            inbound
                .peer_destination()
                .cloned()
                .expect("peer destination retained")
                .encode_to_vec(65535)
                .expect("encode peer destination"),
            inbound.peer_signing_key().clone(),
            inbound.local_port(),
            inbound.remote_port(),
        )
    };
    let peer_hash = *i2pr_crypto::sha256(&peer_bytes).as_bytes();
    let inbound_remote_desc = RemoteDestination {
        destination_hash: peer_hash,
        signing_public_key: peer_key,
        static_public_key: [0u8; 32],
    };
    let syn_response = streaming
        .accept_inbound_syn(
            local_identity,
            &inbound_remote_desc,
            inbound_id,
            inbound_local,
            inbound_remote,
            DEFAULT_ADVERTISED_MAX_PAYLOAD,
            wall_ms(),
            rng,
        )
        .expect("SYN response");
    send_transport_request(
        &syn_response,
        routing,
        session,
        outbound,
        local_identity,
        local_ls2,
        delivery,
        rng,
    )
    .await;
    Some(inbound_id)
}

#[allow(clippy::too_many_arguments)]
async fn pump_until_streaming<F>(
    handle: &mut i2pr_daemon::router_i2np::Ssu2DaemonHandle,
    coord: &mut ExploratoryBuildCoordinator,
    dest: &mut DestinationTunnelCoordinator,
    dispatcher: &mut DestinationDispatcher,
    session: &mut EciesSessionManager,
    routing: &mut DestinationRouting,
    streaming: &mut StreamingManager,
    outbound: &DestinationOutboundRole,
    local_identity: &DestinationIdentity,
    local_ls2: &i2pr_proto::LeaseSet2,
    delivery: &i2pr_daemon::router_i2np::RouterDeliveryService,
    rng: &mut ChaCha8Rng,
    reference_hash_bytes: &[u8; 32],
    _evidence_dir: &Path,
    deadline: tokio::time::Instant,
    _phase: &str,
    mut predicate: F,
) where
    F: FnMut(&mut StreamingManager) -> bool,
{
    while tokio::time::Instant::now() < deadline && !predicate(streaming) {
        let mut pump_errors = 0u64;
        pump_one_streaming_inbound(
            handle,
            coord,
            dest,
            dispatcher,
            session,
            routing,
            streaming,
            outbound,
            local_identity,
            local_ls2,
            delivery,
            rng,
            reference_hash_bytes,
            &mut pump_errors,
            _evidence_dir,
        )
        .await;
        drain_streaming_timers(
            streaming,
            routing,
            session,
            outbound,
            local_identity,
            local_ls2,
            delivery,
            rng,
            _evidence_dir,
        )
        .await;
    }
}

#[allow(clippy::too_many_arguments)]
async fn pump_until_streaming_with_state<F>(
    handle: &mut i2pr_daemon::router_i2np::Ssu2DaemonHandle,
    coord: &mut ExploratoryBuildCoordinator,
    dest: &mut DestinationTunnelCoordinator,
    dispatcher: &mut DestinationDispatcher,
    session: &mut EciesSessionManager,
    routing: &mut DestinationRouting,
    streaming: &mut StreamingManager,
    outbound: &DestinationOutboundRole,
    local_identity: &DestinationIdentity,
    local_ls2: &i2pr_proto::LeaseSet2,
    delivery: &i2pr_daemon::router_i2np::RouterDeliveryService,
    rng: &mut ChaCha8Rng,
    reference_hash_bytes: &[u8; 32],
    _evidence_dir: &Path,
    deadline: tokio::time::Instant,
    _phase: &str,
    target: Option<i2pr_client::streaming::connection::ConnectionId>,
    mut predicate: F,
) where
    F: FnMut(&mut StreamingManager) -> bool,
{
    while tokio::time::Instant::now() < deadline && !predicate(streaming) {
        let mut pump_errors = 0u64;
        pump_one_streaming_inbound(
            handle,
            coord,
            dest,
            dispatcher,
            session,
            routing,
            streaming,
            outbound,
            local_identity,
            local_ls2,
            delivery,
            rng,
            reference_hash_bytes,
            &mut pump_errors,
            _evidence_dir,
        )
        .await;
        drain_streaming_timers(
            streaming,
            routing,
            session,
            outbound,
            local_identity,
            local_ls2,
            delivery,
            rng,
            _evidence_dir,
        )
        .await;
        let _ = target;
    }
}

/// Plan 199 §A.3-A.4 — full destination message plane against the
/// exact-pinned Java reference. The same wire as the i2pd driver,
/// only the reference identifier + the SAM PRIV/PUB distinction differ.
#[tokio::test]
#[ignore = "Plan 194: requires exact-pinned external Java I2P environment"]
async fn destination_message_plane_against_java() {
    let java_ri_path = env_path("JAVA_ROUTER_INFO");
    let java_endpoint: SocketAddr = env_value("JAVA_SSU2_ENDPOINT").parse().expect("endpoint");
    let service_ri_path = env_path("JAVA_SERVICE_ROUTER_INFO");
    let service_endpoint: SocketAddr = env_value("JAVA_SERVICE_SSU2_ENDPOINT")
        .parse()
        .expect("service endpoint");
    let bind: SocketAddr = env_value("I2PR_SSU2_BIND").parse().expect("bind");
    let reference_control_endpoint: SocketAddr = env_value("JAVA_RAW_CONTROL_ENDPOINT")
        .parse()
        .expect("raw reference control endpoint");
    assert!(bind.ip().is_loopback(), "i2pr bind must be loopback");
    assert!(
        java_endpoint.ip().is_loopback(),
        "java endpoint must be loopback"
    );
    assert!(
        service_endpoint.ip().is_loopback(),
        "service endpoint must be loopback"
    );
    assert!(
        reference_control_endpoint.ip().is_loopback(),
        "reference control must be loopback"
    );
    let evidence_dir = env_path("EVIDENCE_DIR");
    std::fs::create_dir_all(&evidence_dir).expect("evidence dir");

    let bind_port = bind.port();
    assert!(bind_port != 0, "lane requires a fixed loopback bind");
    let config_text = format!(
        "schema_version = 1\n[router]\ndata_dir = \"./state\"\n[ssu2]\nenabled = true\nbind_ipv4 = \"127.0.0.1\"\nport = {bind_port}\n"
    );
    let config = Config::parse(&config_text).expect("strict controlled profile");
    assert!(config.ssu2.enabled);
    assert!(!config.ssu2.advertise);
    append_evidence(&evidence_dir, "daemon-strict-profile", "true");

    // ---- Plan 194 §5.1 authenticated transport ----------------------------
    let java_ri_bytes = std::fs::read(&java_ri_path).expect("read java router.info");
    let (java_hash, java_ssu2) =
        verify_reference_router_info(&java_ri_bytes).expect("verify java RouterInfo");
    let material = java_ssu2.address_material().expect("java key material");
    let responder_static =
        i2pr_runtime::Ssu2PublicKey::new(*material.static_public_key().as_bytes())
            .expect("static key");
    let responder_intro = i2pr_runtime::IntroKey::new(*material.intro_key().as_bytes());
    let target = daemon_dial_target(java_hash, java_endpoint, responder_static, responder_intro)
        .expect("dial target");
    let java_router_info = RouterInfo::decode(
        &java_ri_bytes,
        i2pr_runtime::constants::MAX_ESTABLISHMENT_ROUTER_INFO_BYTES,
    )
    .expect("decode java RouterInfo");
    let java_encryption_key: [u8; 32] = java_router_info
        .router_identity()
        .public_key()
        .as_bytes()
        .try_into()
        .expect("32-byte encryption key");
    let service_ri_bytes = std::fs::read(&service_ri_path).expect("read service router.info");
    let (service_hash, service_ssu2) =
        verify_reference_router_info(&service_ri_bytes).expect("verify service RouterInfo");
    let service_material = service_ssu2
        .address_material()
        .expect("service key material");
    let service_static =
        i2pr_runtime::Ssu2PublicKey::new(*service_material.static_public_key().as_bytes())
            .expect("service static key");
    let service_intro = i2pr_runtime::IntroKey::new(*service_material.intro_key().as_bytes());
    let service_target = daemon_dial_target(
        service_hash,
        service_endpoint,
        service_static,
        service_intro,
    )
    .expect("service dial target");
    let service_router_info = RouterInfo::decode(
        &service_ri_bytes,
        i2pr_runtime::constants::MAX_ESTABLISHMENT_ROUTER_INFO_BYTES,
    )
    .expect("decode service RouterInfo");
    append_evidence(&evidence_dir, "reference-routerinfo-verified", "true");
    append_evidence(&evidence_dir, "java-service-routerinfo-verified", "true");

    // Authoritative bootstrap through the ordinary validation path.
    let mut dest = DestinationTunnelCoordinator::new(
        LookupPolicy::default(),
        RouterInfoStoreConfig::default(),
    );
    dest.advance_time(wall_ms());
    let now = Date::from_millis(wall_ms());
    let bootstrapped = dest
        .bootstrap_reference_router_info(&java_ri_bytes, now)
        .expect("bootstrap reference");
    assert_eq!(bootstrapped, RouterHash::from_bytes(*java_hash.as_bytes()));
    append_evidence(
        &evidence_dir,
        "reference-bootstrap-store",
        &dest.store_stats().record_count.to_string(),
    );
    let service_bootstrapped = dest
        .bootstrap_reference_router_info(&service_ri_bytes, now)
        .expect("bootstrap service reference");
    assert_eq!(
        service_bootstrapped,
        RouterHash::from_bytes(*service_hash.as_bytes())
    );

    let caps = java_router_info
        .capabilities()
        .ok()
        .flatten()
        .map(|c| c.as_str().to_owned())
        .unwrap_or_default();
    let is_floodfill = caps.bytes().any(|b| b == b'f');
    if !is_floodfill {
        record_stop(
            &evidence_dir,
            &format!("java reference not advertising floodfill (caps={caps:?})"),
        );
        assert!(is_floodfill, "java reference must advertise floodfill");
    }
    append_evidence(&evidence_dir, "reference-floodfill-capable", "true");

    let bundle =
        i2pr_crypto::RouterIdentityBundle::generate(&mut rand_core::OsRng).expect("identity");
    let local_hash = bundle.identity().hash().expect("hash");
    let identity =
        generate_controlled_identity(&bundle, "127.0.0.1", bind_port).expect("controlled identity");
    append_evidence(
        &evidence_dir,
        "i2pr-routerinfo-len",
        &identity.router_info.len().to_string(),
    );

    let daemon_service = Ssu2DaemonService::new(&config.ssu2, identity).expect("daemon service");
    let token = CancellationToken::new();
    let scope = ChildScope::for_test(&token, ChildFailurePolicy::FailParent);
    let mut handle = daemon_service
        .start(&scope, &config.ssu2)
        .await
        .expect("daemon bind");
    assert_eq!(handle.local_v4().expect("bound v4"), bind);

    let _established = handle
        .dial(target, DIAL_TIMEOUT, &CancellationToken::new())
        .await
        .expect("authenticated session establishes with Java reference");
    let _service_established = handle
        .dial(service_target, DIAL_TIMEOUT, &CancellationToken::new())
        .await
        .expect("authenticated session establishes with Java service router");
    let deadline_active = tokio::time::Instant::now() + WAIT_TIMEOUT;
    while tokio::time::Instant::now() < deadline_active {
        if handle.snapshot().active_sessions >= 2 {
            break;
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
    assert!(handle.snapshot().active_sessions >= 2);
    append_evidence(
        &evidence_dir,
        "session-established",
        &handle.snapshot().sessions_established.to_string(),
    );

    // Plan 199 §A.2 — give the two stock Java RouterContexts each other's
    // signed RouterInfo through ordinary authenticated I2NP. This is the
    // only bootstrap bridge: no Java NetDB insertion, VMComm, reflection,
    // fabricated files, or public reseed is involved.
    let publication_peer = PeerId::from_hash(java_hash);
    let service_peer = PeerId::from_hash(service_hash);
    let publication_wire = database_store_router_info_wire(
        Hash::from_bytes(*service_hash.as_bytes()),
        &service_ri_bytes,
        0x51A7_1001,
    );
    let service_wire = database_store_router_info_wire(
        Hash::from_bytes(*java_hash.as_bytes()),
        &java_ri_bytes,
        0x51A7_1002,
    );
    for (peer, wire) in [
        (publication_peer, publication_wire),
        (service_peer, service_wire),
    ] {
        let request = RouterDeliveryRequest::new(peer, wire, DELIVERY_TIMEOUT)
            .expect("RouterInfo bootstrap request");
        assert_eq!(
            handle
                .delivery()
                .deliver(request, &CancellationToken::new()),
            RouterDeliveryOutcome::Accepted,
            "ordinary Java RouterInfo bootstrap must be admitted"
        );
    }
    append_evidence(
        &evidence_dir,
        "java-router-peer-bootstrap-submitted",
        "service-to-publication-and-publication-to-service",
    );

    let warmup_deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    while tokio::time::Instant::now() < warmup_deadline {
        if tokio::time::timeout(POLL_INTERVAL, handle.next_inbound())
            .await
            .is_err()
        {
            continue;
        }
    }

    // Counted reference service: an ordinary public I2PClient/I2PSession
    // owns and publishes this destination. The SAM bridge remains a
    // diagnostic compatibility surface only and is not used for these rows.
    let mut reference_control = ReferenceControl::connect(reference_control_endpoint).await;
    assert_eq!(reference_control.command("PING").await, "PONG");
    // Plan 200 §A.2 — `REPORT_STATUS` is the explicit bounded helper
    // status command. Asserts ONLY helper-local facts.
    let status_line = reference_control.command("REPORT_STATUS").await;
    assert!(
        status_line.starts_with("STATUS "),
        "REPORT_STATUS must return a STATUS line, got: {status_line:?}"
    );
    let reference_b64 = env_value("JAVA_RAW_REFERENCE_DESTINATION_B64");
    let reference_bytes = decode_destination_b64(&reference_b64);
    let reference_hash =
        i2pr_netdb::DestinationHash::from_hash(i2pr_crypto::sha256(&reference_bytes));
    append_evidence(
        &evidence_dir,
        "public-client-session-established",
        &format!("control_pong=true dest_len={}", reference_bytes.len()),
    );
    append_evidence(
        &evidence_dir,
        "public-client-destination-created",
        &format!(
            "dest_len={} session=connected control_ready=1 publication_observed=external",
            reference_bytes.len()
        ),
    );
    append_evidence(&evidence_dir, "public-client-leaseset-status", &status_line);

    // Real one-hop builds in both directions; replies route through
    // the coordinator to real Installed pool + registry roles.
    let mut coord = ExploratoryBuildCoordinator::new(ExploratoryPoolConfig::balanced());
    coord.advance_time(wall_ms());
    let bridge = ShortBuildI2npBridge::new();
    let mut rng = ChaCha8Rng::seed_from_u64(wall_secs());
    let delivery = handle.delivery().clone();

    let outbound_request = BuildRequest {
        direction: BuildDirection::Outbound,
        peer: PeerBuildMaterial {
            router_hash: java_hash,
            static_encryption_key: java_encryption_key,
            receive_tunnel: TunnelId::new(OBEP_RECEIVE).expect("receive"),
            next_tunnel: TunnelId::new(OBEP_NEXT).expect("next"),
            role: HopRole::OutboundEndpoint,
        },
        creator_tunnel_id: TunnelId::new(OUTBOUND_CREATOR).expect("creator"),
        message_id: 0x51A7_5001,
        outbound_reply_router: Some(local_hash),
        originator_hash: None,
    };
    coord
        .submit(
            outbound_request,
            &delivery,
            &bridge,
            BridgeHeader::ShortTransport {
                message_id: 0x51A7_5001,
                expiration_seconds: wall_secs().saturating_add(60) as u32,
            },
            &mut rng,
        )
        .expect("outbound submit ok");
    append_evidence(&evidence_dir, "outbound-build-emitted", "true");

    let inbound_request = BuildRequest {
        direction: BuildDirection::Inbound,
        peer: PeerBuildMaterial {
            router_hash: service_hash,
            static_encryption_key: service_router_info
                .router_identity()
                .public_key()
                .as_bytes()
                .try_into()
                .expect("service encryption key"),
            receive_tunnel: TunnelId::new(IBGW_RECEIVE).expect("receive"),
            next_tunnel: TunnelId::new(IBGW_NEXT).expect("next"),
            role: HopRole::InboundGateway,
        },
        creator_tunnel_id: TunnelId::new(INBOUND_CREATOR).expect("creator"),
        message_id: 0x51A7_5101,
        outbound_reply_router: None,
        originator_hash: Some(local_hash),
    };
    coord
        .submit(
            inbound_request,
            &delivery,
            &bridge,
            BridgeHeader::ShortTransport {
                message_id: 0x51A7_5101,
                expiration_seconds: wall_secs().saturating_add(60) as u32,
            },
            &mut rng,
        )
        .expect("inbound submit ok");
    append_evidence(&evidence_dir, "inbound-build-emitted", "true");

    // Pump inbound I2NP until both builds Install with real material.
    let mut outbound_slot: Option<i2pr_tunnel::pool::TunnelSlot> = None;
    let mut installed_inbound = false;
    let mut pump_installed_ob = 0u64;
    let mut pump_installed_ib = 0u64;
    let mut pump_kind_reply = 0u64;
    let mut pump_dispatch_error = 0u64;
    let mut pump_invalid = 0u64;
    let snapshot_before = handle.snapshot();
    let install_deadline = tokio::time::Instant::now() + ACCEPT_TIMEOUT;
    while tokio::time::Instant::now() < install_deadline
        && (outbound_slot.is_none() || !installed_inbound)
    {
        let next = tokio::time::timeout(POLL_INTERVAL, handle.next_inbound()).await;
        let Ok(Some(inbound)) = next else {
            continue;
        };
        let routed = match coord.route_inbound_i2np(&inbound, wall_ms()) {
            Ok(routed) => routed,
            Err(_) => {
                pump_dispatch_error += 1;
                continue;
            }
        };
        if let i2pr_daemon::router_i2np::RouterI2npOutcome::TunnelBuildReserved { kind, .. } =
            &routed.dispatcher
            && matches!(
                kind,
                i2pr_daemon::router_i2np::RouterI2npKind::OutboundTunnelBuildReply
            )
        {
            pump_kind_reply += 1;
        }
        for outcome in routed.coordinator {
            if let BuildCoordinatorOutcome::Installed {
                slot, direction, ..
            } = outcome
            {
                match direction {
                    BuildDirection::Outbound => {
                        outbound_slot = Some(slot);
                        pump_installed_ob += 1;
                    }
                    BuildDirection::Inbound => {
                        installed_inbound = true;
                        pump_installed_ib += 1;
                    }
                }
            } else {
                pump_invalid += 1;
            }
        }
    }
    append_evidence(
        &evidence_dir,
        "install-pump-summary",
        &format!(
            "installed_ob={pump_installed_ob} installed_ib={pump_installed_ib} non_install={pump_invalid} dispatch_error={pump_dispatch_error} kind_reply={pump_kind_reply}"
        ),
    );
    let snapshot_after = handle.snapshot();
    append_evidence(
        &evidence_dir,
        "session-pump-delta",
        &format!(
            "datagrams_received={} i2np_received={} protocol_drops={} auth_failures={} queue_drops={}",
            snapshot_after
                .datagrams_received
                .saturating_sub(snapshot_before.datagrams_received),
            snapshot_after
                .i2np_received
                .saturating_sub(snapshot_before.i2np_received),
            snapshot_after
                .protocol_drops
                .saturating_sub(snapshot_before.protocol_drops),
            snapshot_after
                .auth_failures
                .saturating_sub(snapshot_before.auth_failures),
            snapshot_after
                .inbound_queue_drops
                .saturating_sub(snapshot_before.inbound_queue_drops),
        ),
    );
    let outbound_slot = match outbound_slot {
        Some(slot) => slot,
        None => {
            // Plan 194 §11 stop: Java accepted both builds (transit
            // log evidence) but emitted no consumable
            // ShortTunnelBuildReply within the bounded window. Mark
            // every install-dependent row with stop provenance; the
            // shell records blocked, never passed.
            assert!(dest.note_direct_transport_attempt().is_err());
            append_evidence(&evidence_dir, "direct-rejected", "true");
            let mut scheduler = TunnelLivenessScheduler::new(LivenessConfig::plan_185_defaults());
            scheduler.advance_time(wall_ms());
            scheduler
                .register_pair(
                    i2pr_tunnel::pool::TunnelSlot::from_raw(1),
                    i2pr_tunnel::pool::TunnelSlot::from_raw(2),
                )
                .expect("pair");
            scheduler.advance_time(wall_ms() + FIRST_LIVENESS_DELAY_MS + 1);
            let action = scheduler.drive();
            assert!(matches!(action, LivenessAction::SendTest { .. }));
            append_evidence(&evidence_dir, "liveness-first-test", "passed");
            record_stop(
                &evidence_dir,
                &format!(
                    "Plan 194 §11 stop: java outbound build never installed (installed_ob={pump_installed_ob} kind_reply={pump_kind_reply})"
                ),
            );
            // Plan 220 — even on the install-stalled stop path,
            // emit exactly one `p220-classification` row. The
            // authoritative epoch was never reached, so it is
            // honestly a HASH-stage observability gap, never a
            // root-cause attribution (`java_hash` is Router B).
            record_p220_early_stop_gap(
                &evidence_dir,
                &p220_bytes_to_hex(java_hash.as_bytes()),
                "authoritative-epoch-never-reached-install-stalled",
            );
            // Plan 222 — every run emits exactly one P222 terminal. A
            // pre-epoch stop cannot prove selector equivalence.
            record_p222_early_stop_gap(
                &evidence_dir,
                "authoritative-epoch-never-reached-install-stalled",
            );
            // Plan 224 — every run emits exactly one P224 terminal. A
            // pre-epoch stop has no P224 snapshot, so the terminal is
            // honestly the lookup-path observability gap.
            record_p224_early_stop_gap(
                &evidence_dir,
                "authoritative-epoch-never-reached-install-stalled",
            );
            handle.shutdown();
            let _ = scope.shutdown().await;
            return;
        }
    };
    assert!(
        installed_inbound,
        "Plan 194 §11 stop: java inbound build never installed"
    );
    assert_eq!(coord.registry().outbound_len(), 1);
    assert_eq!(coord.registry().inbound_len(), 1);
    append_evidence(&evidence_dir, "outbound-installed", "true");
    append_evidence(&evidence_dir, "inbound-installed", "true");

    let registrations_out = coord.registrations(TunnelDirection::Outbound);
    let registrations_in = coord.registrations(TunnelDirection::Inbound);
    assert_eq!(registrations_out.len(), 1);
    assert_eq!(registrations_in.len(), 1);
    assert!(
        registrations_out[0]
            .hops()
            .iter()
            .any(|hop| hop.hash() == Hash::from_bytes(*java_hash.as_bytes())),
        "outbound hop must be the publication router"
    );
    let receive_ids = coord.registry().inbound_receive_ids();
    assert_eq!(receive_ids.len(), 1);
    append_evidence(
        &evidence_dir,
        "destination-material-real",
        &format!(
            "outbound_slots=1 inbound_slots=1 zero_hop=0 receive={}",
            receive_ids[0].get()
        ),
    );

    let gateway_role = coord
        .registry_mut()
        .remove_outbound(outbound_slot)
        .expect("real outbound role");
    let destination_outbound =
        DestinationOutboundRole::from_role(gateway_role, wall_ms() + 600_000);

    // ---- Plan 194 §5.3 remote Standard LeaseSet2 lookup ------------------
    let routing_key = i2pr_netdb::router_hash_from_destination(reference_hash);
    let local_receive_for_lookup = receive_ids[0];
    let reply_path = reply_path_for_inbound_route(coord.registry(), local_receive_for_lookup)
        .expect("typed inbound gateway route");
    let inbound_route = coord
        .registry()
        .inbound_gateway_route(local_receive_for_lookup)
        .expect("inbound route exists");
    assert_eq!(inbound_route.gateway_router, service_hash);
    assert_eq!(inbound_route.gateway_receive_tunnel.get(), IBGW_RECEIVE);
    assert_eq!(inbound_route.local_receive_tunnel.get(), IBGW_NEXT);
    assert_eq!(reply_path.tunnel_id(), IBGW_RECEIVE);
    append_evidence(
        &evidence_dir,
        "inbound-reply-path",
        &format!(
            "gateway_matches_service_router=true gateway_tunnel={} local_receive={} ids_distinct=true",
            inbound_route.gateway_receive_tunnel.get(),
            inbound_route.local_receive_tunnel.get(),
        ),
    );
    let (lookup_id, action) = dest
        .begin_lease_lookup(reference_hash, &routing_key, reply_path)
        .expect("lease lookup send");
    // Plan 201 §G — emit the Branch G observation snapshot the
    // daemon-owned destination coordinator just advanced during
    // `begin_lease_lookup`. The recorded counter snapshot lets the
    // harness attribute any later lookup failure to a specific
    // Branch G boundary without re-running the diagnostic.
    let post_lookup_counters = dest.counters();
    append_evidence(
        &evidence_dir,
        "p201-lookup-boundary-floodfill-selection-present",
        &format!(
            "floodfill_candidates_present={} floodfill_candidates_absent={} \
             reply_paths_derived={} reply_paths_unresolved={} \
             lookup_key_matches={} lookup_key_mismatches={} \
             ls2_records_decoded={} ls2_records_decode_rejected={} \
             ls2_records_signature_rejected={} \
             inbound_cells_garlic_completed={} inbound_cells_garlic_incomplete={}",
            post_lookup_counters.floodfill_candidates_present,
            post_lookup_counters.floodfill_candidates_absent,
            post_lookup_counters.reply_paths_derived,
            post_lookup_counters.reply_paths_unresolved,
            post_lookup_counters.lookup_key_matches,
            post_lookup_counters.lookup_key_mismatches,
            post_lookup_counters.ls2_records_decoded,
            post_lookup_counters.ls2_records_decode_rejected,
            post_lookup_counters.ls2_records_signature_rejected,
            post_lookup_counters.inbound_cells_garlic_completed,
            post_lookup_counters.inbound_cells_garlic_incomplete,
        ),
    );
    let mut tunnel_rng = ChaCha8Rng::seed_from_u64(wall_secs().wrapping_add(7));
    let (dispatch, proof) = dest
        .compose_lookup_via_tunnel(
            &action,
            destination_outbound.role(),
            0x51A7_6001,
            wall_ms() + 60_000,
            Deadline::new(Duration::from_secs(60)).expect("deadline"),
            &mut tunnel_rng,
            0,
        )
        .expect("compose lease lookup");
    assert!(proof.via_tunnel);
    append_evidence(
        &evidence_dir,
        "outbound-lookup-via-tunnel",
        &format!("cells={}", proof.cell_count),
    );
    for cell_delivery in &dispatch.deliveries {
        let request = RouterDeliveryRequest::new(
            cell_delivery.target(),
            cell_delivery.message_bytes().to_vec(),
            DELIVERY_TIMEOUT,
        )
        .expect("delivery request");
        assert_eq!(
            delivery.deliver(request, &CancellationToken::new()),
            RouterDeliveryOutcome::Accepted
        );
    }

    let lookup_deadline = tokio::time::Instant::now() + DATAGRAM_WAIT;
    let mut lease_summary: Option<i2pr_daemon::destination_tunnels::RemoteLeaseSummary> = None;
    let mut lookup_pump_error = 0u64;
    while tokio::time::Instant::now() < lookup_deadline && lease_summary.is_none() {
        let next = tokio::time::timeout(POLL_INTERVAL, handle.next_inbound()).await;
        let Ok(Some(inbound)) = next else {
            continue;
        };
        let message = match decode_inbound_i2np(&inbound.bytes) {
            Ok(message) => message,
            Err(_) => {
                lookup_pump_error += 1;
                continue;
            }
        };
        let cell = match message.body() {
            I2npBody::TunnelData(cell) => cell.clone(),
            _ => continue,
        };
        let outcome = match dispatch_inbound_tunnel_data(coord.registry_mut(), &cell, wall_ms()) {
            Ok(outcome) => outcome,
            Err(_) => {
                lookup_pump_error += 1;
                continue;
            }
        };
        let bytes = match outcome {
            i2pr_daemon::inbound_dispatch::InboundDispatchOutcome::DatabaseStoreComplete {
                bytes,
            } => bytes,
            _ => continue,
        };
        let envelope =
            I2npMessage::decode_standard(&bytes, MAX_I2NP_PAYLOAD_SIZE).expect("decode store");
        let now_secs = u32::try_from(wall_secs()).unwrap_or(u32::MAX);
        let pre_ingest_counters = dest.counters();
        match dest
            .ingest_tunnel_lease_store(lookup_id, &envelope, now_secs)
            .expect("ingest lease store")
        {
            LeaseStoreIngestOutcome::Completed { summary, .. } => {
                lease_summary = Some(summary);
                // Plan 201 §G — the LS2 lookup completed cleanly. Emit
                // the post-ingest Branch G counter snapshot so a
                // subsequent diagnostic row can prove the lookup
                // traversed every documented boundary.
                let post_counters = dest.counters();
                append_evidence(
                    &evidence_dir,
                    "p201-lookup-boundary-ls2-key-match-match",
                    &format!(
                        "pre_ls2_decoded={} post_ls2_decoded={} \
                         pre_lookup_key_matches={} post_lookup_key_matches={} \
                         pre_signature_rejected={} post_signature_rejected={} \
                         outcome=completed",
                        pre_ingest_counters.ls2_records_decoded,
                        post_counters.ls2_records_decoded,
                        pre_ingest_counters.lookup_key_matches,
                        post_counters.lookup_key_matches,
                        pre_ingest_counters.ls2_records_signature_rejected,
                        post_counters.ls2_records_signature_rejected,
                    ),
                );
                append_evidence(
                    &evidence_dir,
                    "p201-lookup-boundary-database-store-ls2-decode-decoded",
                    &format!(
                        "ls2_records_decoded={} ls2_records_decode_rejected={} \
                         ls2_records_signature_rejected={}",
                        post_counters.ls2_records_decoded,
                        post_counters.ls2_records_decode_rejected,
                        post_counters.ls2_records_signature_rejected,
                    ),
                );
                append_evidence(
                    &evidence_dir,
                    "p201-lookup-boundary-reply-gateway-derived",
                    &format!(
                        "reply_paths_derived={} reply_paths_unresolved={}",
                        post_counters.reply_paths_derived, post_counters.reply_paths_unresolved,
                    ),
                );
            }
            LeaseStoreIngestOutcome::Continue | LeaseStoreIngestOutcome::Ignored => {
                let post_counters = dest.counters();
                let outcome_label = if pre_ingest_counters.lookup_key_mismatches
                    != post_counters.lookup_key_mismatches
                {
                    "ls2-key-mismatch"
                } else if pre_ingest_counters.ls2_records_decode_rejected
                    != post_counters.ls2_records_decode_rejected
                {
                    "decode-rejected"
                } else if pre_ingest_counters.ls2_records_signature_rejected
                    != post_counters.ls2_records_signature_rejected
                {
                    "signature-rejected"
                } else {
                    "ignored"
                };
                append_evidence(
                    &evidence_dir,
                    "p201-lookup-boundary-database-store-ls2-decode-decoded",
                    &format!(
                        "ls2_records_decoded={} ls2_records_decode_rejected={} \
                         ls2_records_signature_rejected={} outcome={}",
                        post_counters.ls2_records_decoded,
                        post_counters.ls2_records_decode_rejected,
                        post_counters.ls2_records_signature_rejected,
                        outcome_label,
                    ),
                );
            }
        }
    }
    #[allow(unused_variables)]
    let summary = if let Some(s) = lease_summary {
        s
    } else {
        // Plan 194 §11 stop: Java accepted the build, the outbound
        // tunnel sent the DatabaseLookup, but the reference LS2
        // never resolved on the inbound tunnel. The public Java client
        // session is ready and its client-specific LS2 is locally present,
        // but the exact-pinned Java router did not make that LS2 visible in
        // the distinct publication router's main NetDB. This is the first
        // Plan 199 publication boundary; the harness records every
        // install-dependent + delivery-dependent row as `blocked` with
        // this stop provenance.
        assert!(dest.note_direct_transport_attempt().is_err());
        append_evidence(&evidence_dir, "direct-rejected", "true");
        let mut scheduler = TunnelLivenessScheduler::new(LivenessConfig::plan_185_defaults());
        scheduler.advance_time(wall_ms());
        scheduler
            .register_pair(
                i2pr_tunnel::pool::TunnelSlot::from_raw(1),
                i2pr_tunnel::pool::TunnelSlot::from_raw(2),
            )
            .expect("pair");
        scheduler.advance_time(wall_ms() + FIRST_LIVENESS_DELAY_MS + 1);
        let action = scheduler.drive();
        assert!(matches!(action, LivenessAction::SendTest { .. }));
        append_evidence(&evidence_dir, "liveness-first-test", "passed");
        record_stop(
            &evidence_dir,
            &format!(
                "Plan 199 stop: client-ls2-local-but-not-network-visible (sent DatabaseLookup, no response within {}s; lookup_pump_error={lookup_pump_error})",
                DATAGRAM_WAIT.as_secs()
            ),
        );
        // Plan 220 — emit the `p220-classification`
        // observability gap even on the lease-store-stalled stop
        // path. The authoritative epoch was never reached, so no
        // Java-state observation exists to attribute (`java_hash`
        // is Router B).
        record_p220_early_stop_gap(
            &evidence_dir,
            &p220_bytes_to_hex(java_hash.as_bytes()),
            "authoritative-epoch-never-reached-lease-stalled",
        );
        // Plan 222 — every run emits exactly one P222 terminal. A
        // pre-epoch stop yields the pre-epoch observability gap.
        record_p222_early_stop_gap(
            &evidence_dir,
            "authoritative-epoch-never-reached-lease-stalled",
        );
        // Plan 224 — every run emits exactly one P224 terminal. A
        // pre-epoch stop has no P224 snapshot, so the terminal is
        // honestly the lookup-path observability gap.
        record_p224_early_stop_gap(
            &evidence_dir,
            "authoritative-epoch-never-reached-lease-stalled",
        );
        handle.shutdown();
        let _ = scope.shutdown().await;
        return;
    };
    // Plan 217 §6.A — once an installed outbound role has been
    // transferred out of `coord.registry_mut()` (the
    // `DestinationOutboundRole::from_role` move above), the registry
    // must report `outbound_len() == 0` for that slot. The previous
    // duplicate block asserted the registry still owned it and ran a
    // second `remove_outbound` against the same slot; that regression
    // was the source of the Plan 216 panic.
    assert_eq!(
        coord.registry().outbound_len(),
        0,
        "Plan 217 §6.A: registry must NOT retain an outbound slot after a successful DestinationOutboundRole transfer (destination driver transfer-once invariant)"
    );
    assert_eq!(coord.registry().inbound_len(), 1);
    let _ = Hash::from_bytes(*java_hash.as_bytes());
    append_evidence(
        &evidence_dir,
        "destination-outbound-transferred",
        "outbound_role_transferred_once registry_outbound_len=0 inbound_len=1",
    );
    assert_eq!(summary.destination, reference_hash);
    assert!(summary.lease_count >= 1, "reference must publish a lease");
    append_evidence(
        &evidence_dir,
        "lookup-pump-error",
        &lookup_pump_error.to_string(),
    );
    append_evidence(
        &evidence_dir,
        "lease-lookup-completed",
        &format!(
            "leases={} published={} expires={}",
            summary.lease_count, summary.published_seconds, summary.expires_seconds
        ),
    );

    // ---- Plan 194 §5.4(b) local Standard LeaseSet2 publication ------------
    let mut identity_rng = OsRng;
    let local_identity =
        DestinationIdentity::generate(&mut identity_rng).expect("destination identity");
    let tunnel_expires = wall_secs() + 600;
    let lease_source = InboundLeaseSource::from_parts(
        registrations_in[0].slot(),
        Hash::from_bytes(*java_hash.as_bytes()),
        IBGW_RECEIVE,
        tunnel_expires,
        tunnel_expires.saturating_sub(60),
    );
    let published = u32::try_from(wall_secs()).unwrap_or(u32::MAX);
    let local_ls2 =
        build_signed_lease_set2(&local_identity, &[lease_source], published).expect("local ls2");
    let local_dest_hash = local_identity.id().as_netdb_key();

    let store_message = i2pr_proto::DatabaseStoreMessage {
        key: local_ls2.key_hash().expect("key hash"),
        reply_token: 0,
        reply_tunnel_id: None,
        reply_gateway: None,
        data: i2pr_proto::DatabaseStoreData::LeaseSet2(Box::new(local_ls2.clone())),
    };
    let publication_id = dest
        .begin_ls2_publication(store_message, RouterHash::from_bytes(*java_hash.as_bytes()))
        .expect("begin publication");
    let (pub_dispatch, pub_proof) = dest
        .compose_ls2_publication_via_tunnel(
            publication_id,
            Hash::from_bytes(*java_hash.as_bytes()),
            destination_outbound.role(),
            0x51A7_6101,
            wall_ms() + 60_000,
            Deadline::new(Duration::from_secs(60)).expect("deadline"),
            &mut tunnel_rng,
            0,
        )
        .expect("compose publication");
    assert!(pub_proof.via_tunnel);
    for cell_delivery in &pub_dispatch.deliveries {
        let request = RouterDeliveryRequest::new(
            cell_delivery.target(),
            cell_delivery.message_bytes().to_vec(),
            DELIVERY_TIMEOUT,
        )
        .expect("delivery request");
        assert_eq!(
            delivery.deliver(request, &CancellationToken::new()),
            RouterDeliveryOutcome::Accepted
        );
    }
    append_evidence(
        &evidence_dir,
        "ls2-publication-tunnel",
        &format!("cells={}", pub_proof.cell_count),
    );

    // ---- Plan 194 §5.4(c) outbound ECIES/Garlic + tunnel delivery ---------
    let mut routing = DestinationRouting::new(DestinationRoutingConfig::balanced());
    let cached = dest
        .lease_store()
        .get(&reference_hash)
        .expect("cached reference ls2")
        .clone();
    let remote = routing
        .install_remote_lease_set2(cached)
        .expect("install remote ls2");
    assert_eq!(remote, reference_hash);
    let mut session = EciesSessionManager::new(EciesSessionConfig::balanced());
    let app_out = b"plan194-destination-probe-a";
    let request = OutboundRequest::new(
        i2pr_proto::PROTOCOL_TYPE_RAW,
        0,
        0,
        app_out,
        wall_ms(),
        Some(local_ls2.clone()),
    )
    .expect("outbound request");
    let mut compose_rng = ChaCha8Rng::seed_from_u64(wall_secs().wrapping_add(11));
    let plan = compose_outbound_delivery(
        &routing,
        &mut session,
        &destination_outbound,
        local_identity.id(),
        local_identity.static_secret_bytes(),
        reference_hash,
        &request,
        u32::try_from(wall_secs()).unwrap_or(u32::MAX),
        wall_ms(),
        &mut compose_rng,
    )
    .expect("compose destination delivery");
    let cell_dispatch = deliver_outbound_cells(
        &plan.cells,
        wall_ms() + 60_000,
        Deadline::new(Duration::from_secs(60)).expect("deadline"),
        &mut compose_rng,
    )
    .expect("encode destination cells");
    for cell_delivery in &cell_dispatch.deliveries {
        let send = RouterDeliveryRequest::new(
            cell_delivery.target(),
            cell_delivery.message_bytes().to_vec(),
            DELIVERY_TIMEOUT,
        )
        .expect("delivery request");
        assert_eq!(
            delivery.deliver(send, &CancellationToken::new()),
            RouterDeliveryOutcome::Accepted
        );
    }
    append_evidence(
        &evidence_dir,
        "destination-outbound-delivered",
        &format!("cells={} payload_len={}", plan.cells.len(), app_out.len()),
    );

    let received = reference_control.wait_raw(DATAGRAM_WAIT).await;
    // Plan 220 WP G — directionally correct dispatch evidence. The
    // forward i2pr→Java receipt below MUST NOT satisfy any reverse
    // observation; it only fills the forward fact.
    let mut p220_forward_received = false;
    if let Some((received_len, received_digest)) = received {
        if received_len == app_out.len() && received_digest == sha256_hex(app_out) {
            p220_forward_received = true;
            append_evidence(
                &evidence_dir,
                "reference-received",
                &format!(
                    "payload_len={} match=true digest={}",
                    received_len, received_digest
                ),
            );
        } else {
            append_evidence(
                &evidence_dir,
                "reference-received-timeout",
                &format!(
                    "digest mismatch expected_len={} observed_len={}",
                    app_out.len(),
                    received_len
                ),
            );
        }
    } else {
        append_evidence(
            &evidence_dir,
            "reference-received-timeout",
            &format!("timeout after {}s", DATAGRAM_WAIT.as_secs()),
        );
    }

    // ---- Plan 220 §7 authoritative observation ---------------------------
    // Fresh A-view-of-B / PeerManager / selector queries at the
    // post-driver-bootstrap / pre-reverse-send epoch, taken by the
    // driver itself after its own A/B RouterInfo bootstrap (which
    // already submitted B's signed RouterInfo to A through
    // ordinary authenticated I2NP — D220-3). The Router B hash is
    // the protocol-derived identity hash, never a raw RouterInfo
    // byte slice, and the Java self snapshot must echo that same
    // value (D220-2 cross-check). Missing responses become
    // Unknown, never protocol facts.
    //
    // Naming: in this driver `java_hash` is Router B (the
    // publication router, JAVA_ROUTER_INFO) and `service_hash` is
    // Router A (the service router, JAVA_SERVICE_ROUTER_INFO).
    let rust_b_hex = p220_bytes_to_hex(java_hash.as_bytes());
    let reverse_lookup_target_hex = p220_bytes_to_hex(local_dest_hash.as_bytes());
    let diag_a_port: u16 = env_value("JAVA_DIAGNOSTIC_A_PORT")
        .parse()
        .expect("diagnostic A port");
    let diag_b_port: u16 = env_value("JAVA_DIAGNOSTIC_B_PORT")
        .parse()
        .expect("diagnostic B port");
    let mut p220_facts = p220_collect_authoritative(
        diag_a_port,
        diag_b_port,
        &rust_b_hex,
        &reverse_lookup_target_hex,
    )
    .await;
    p220_facts.forward_i2pr_to_java_received = P220Observed::Known(p220_forward_received);

    // ---- Plan 222 WP B — exact client-lookup preflight --------------------
    // Helper source Destination hash is the client DBID (`fromLocalDest`
    // / client DBID used by OCMOSJ). The target is the i2pr destination
    // hash the reverse lookup must resolve. Both travel as exact 32-byte
    // lowercase hex; the Java diagnostic derives the routing key with its
    // own `routingKeyGenerator().getRoutingKey` and reproduces the
    // `netdb.searchLimit` + EXTRA_PEERS width through the
    // production-equivalent 3-argument selector overload.
    let helper_client_dbid_hex = p220_bytes_to_hex(reference_hash.as_bytes());
    let p222_preflight = p222_collect_preflight(
        diag_a_port,
        &helper_client_dbid_hex,
        &reverse_lookup_target_hex,
        &rust_b_hex,
    )
    .await;
    let p222_selector_equivalent = p222_selector_equivalence(p222_preflight.as_ref());
    let (p222_selector_nonempty, p222_target_ls_pre_send) = match p222_preflight.as_ref() {
        Some(pre) if pre.observable && pre.client_db_is_client => (
            P220Observed::Known(!pre.selector_empty),
            P220Observed::Known(pre.target_ls_present_before_send),
        ),
        Some(_) => (
            P220Observed::Unknown("client-db-not-proven"),
            P220Observed::Unknown("client-db-not-proven"),
        ),
        None => (
            P220Observed::Unknown("preflight-unreachable"),
            P220Observed::Unknown("preflight-unreachable"),
        ),
    };

    // ---- Plan 224 WP B/C — pre-send lookup-path snapshots ------------------
    // Router-B main-NetDB state first (the mandatory gate before any
    // search-path attribution), then the Router-A helper client-subDB
    // state, both at the exact tracked-send target hash. Read-only
    // local diagnostic commands: no publication retry, no re-publish,
    // no standalone lookup that could prime the helper client DB. The
    // settle between publication and these snapshots is the
    // unmodified forward-path + P220/P222 sequence above (no added
    // sleep, no tuning). The full frozen tracked-send sequence below
    // runs unconditionally: a B-gate terminal stops the attribution
    // descent, never the run (P222/P223 rows and the frozen 45-second
    // window still require it).
    let p224_b_pre = p224_collect_main_ls(diag_b_port, &reverse_lookup_target_hex).await;
    record_p224_main_snapshot(
        &evidence_dir,
        "p224-b-main-before-send",
        p224_b_pre.as_ref(),
    );
    let p224_a_pre = p224_collect_client_ls(
        diag_a_port,
        &helper_client_dbid_hex,
        &reverse_lookup_target_hex,
    )
    .await;
    record_p224_client_snapshot(
        &evidence_dir,
        "p224-a-client-before-send",
        p224_a_pre.as_ref(),
    );
    // Exact JVM Base64 renderings for the whitelist-only log
    // sanitizer (stateless diagnostic; touches no router state).
    let p224_target_b64 = p224_collect_hash_b64(diag_a_port, &reverse_lookup_target_hex).await;
    let p224_router_b_b64 = p224_collect_hash_b64(diag_a_port, &rust_b_hex).await;
    let p224_helper_b64 = p224_collect_hash_b64(diag_a_port, &helper_client_dbid_hex).await;
    append_evidence(
        &evidence_dir,
        "p224-target-context",
        &format!(
            "target_hash_hex={} target_hash_b64={} router_b_hash_hex={} router_b_hash_b64={} helper_dbid_hex={} helper_dbid_b64={}",
            reverse_lookup_target_hex,
            p224_target_b64.as_deref().unwrap_or("unknown"),
            rust_b_hex,
            p224_router_b_b64.as_deref().unwrap_or("unknown"),
            helper_client_dbid_hex,
            p224_helper_b64.as_deref().unwrap_or("unknown"),
        ),
    );

    // ---- Plan 194 §5.4(c) inbound reply via real inbound tunnel ----------
    // Plan 222 WP C/E — tracked helper send using the public
    // listener-enabled long `sendMessage`. Legacy `send_raw()` remains
    // for historical tests; the P222 reverse path uses `SEND_TRACKED`.
    let local_b64 = i2pr_api::sam::base64::encode(
        &local_identity
            .destination()
            .encode_to_vec(65535)
            .expect("encode dest"),
    );
    // Plan 223 WP B — exact Rust Destination facts for the same bytes
    // the reverse send will use. No inference from source alone; the
    // helper + router Java parses below must match these exactly.
    let p223_rust_dest = {
        let dest = local_identity.destination();
        let hash_hex = p220_bytes_to_hex(dest.hash().expect("destination hash").as_bytes());
        Some(P223Dest {
            hash_hex,
            enc_type_code: dest.public_key().key_type().code() as i32,
            enc_type_name: format!("{:?}", dest.public_key().key_type()),
            public_key_len: dest.public_key().as_bytes().len(),
            sig_type_code: dest.signing_key().key_type().code() as i32,
        })
    };
    // Pre-send Java observations at the same epoch as the P222 preflight
    // (post-bootstrap, pre-send). Both are read-only; neither mutates
    // NetDB, KeyManager, tunnel, or LeaseSet state.
    let p223_helper_dest = reference_control
        .inspect_dest(&local_b64)
        .await
        .and_then(|line| p223_parse_dest_info(&line));
    let p223_router_dest = p223_collect_router_dest_inspect(diag_a_port, &local_b64).await;
    // P223 WP C pre-send branch snapshot (same epoch). Recorded now;
    // the post-fix fallback re-queries only if status 17 persists is
    // handled by reusing this pre-send snapshot plus a post-send refresh
    // below when needed.
    let p223_branch_presend = p223_collect_branch(
        diag_a_port,
        &helper_client_dbid_hex,
        &reverse_lookup_target_hex,
        &helper_client_dbid_hex,
    )
    .await;
    let app_back = b"plan194-destination-reply-b";
    let reverse_sha256 = sha256_hex(app_back);
    let tracked_start = tokio::time::Instant::now();
    let tracked = reference_control
        .send_raw_tracked(&local_b64, app_back, 0, 0)
        .await;
    // `reverse_java_send_admitted` is set only after the tracked call
    // returns successfully with a valid nonce.
    let reverse_admitted = tracked.is_some();
    if tracked.as_ref().is_some_and(|t| t.digest != reverse_sha256) {
        append_evidence(
            &evidence_dir,
            "p222-tracked-digest-mismatch",
            "tracked digest does not match intended reverse payload",
        );
    }
    p220_facts.reverse_java_send_admitted = P220Observed::Known(reverse_admitted);
    let inbound_send_status = if reverse_admitted {
        "public-send-accepted"
    } else {
        "public-send-rejected"
    };
    let tracked_nonce_observed: P220Observed<u64> = match tracked.as_ref() {
        Some(t) => P220Observed::Known(t.nonce),
        None => P220Observed::Unknown("tracked-send-not-admitted"),
    };

    let mut dispatcher = DestinationDispatcher::new();
    dispatcher
        .register_destination(local_identity.id())
        .expect("register destination");
    dispatcher
        .bind_destination_hash(local_identity.id(), local_dest_hash)
        .expect("bind destination hash");
    let mut inbound_payload: Option<Vec<u8>> = None;
    // Plan 220 WP G — reverse-direction TunnelData observation is
    // tracked independently of the forward receipt above.
    let mut p220_tunneldata_observed = false;
    let mut reply_pump_error = 0u64;
    // Plan 222 WP E — preserve the 45-second delivery window exactly.
    // The reverse-send flow runs the existing i2pr inbound pump for
    // exactly the existing `DATAGRAM_WAIT` duration, polls
    // `SEND_STATUS nonce` during the window without delaying the pump
    // (the deadline is wall-clock fixed), freezes the legacy
    // payload-delivery result at 45 seconds, then continues
    // status-only polling until 70 seconds from the tracked-send
    // start. The i2pr payload acceptance window is never resumed.
    let reply_deadline = tokio::time::Instant::now() + DATAGRAM_WAIT;
    let mut p222_status_events: Vec<TrackedStatusEvent> = Vec::new();
    let mut last_status_poll = tokio::time::Instant::now()
        .checked_sub(Duration::from_secs(10))
        .unwrap_or_else(tokio::time::Instant::now);
    while tokio::time::Instant::now() < reply_deadline && inbound_payload.is_none() {
        // Periodic status poll (every ~2 s) without delaying the pump:
        // the poll uses the bounded helper map and short control RTT;
        // the payload deadline stays fixed regardless of poll latency.
        if tracked.is_some()
            && tokio::time::Instant::now().duration_since(last_status_poll)
                >= Duration::from_secs(2)
        {
            last_status_poll = tokio::time::Instant::now();
            if let Some(nonce) = tracked.as_ref().map(|t| t.nonce) {
                #[allow(clippy::collapsible_if)]
                if let Some(events) = reference_control.send_status(nonce).await {
                    p222_status_events = events;
                }
            }
        }
        let next = tokio::time::timeout(POLL_INTERVAL, handle.next_inbound()).await;
        let Ok(Some(inbound)) = next else {
            continue;
        };
        let message = match decode_inbound_i2np(&inbound.bytes) {
            Ok(message) => message,
            Err(_) => {
                reply_pump_error += 1;
                continue;
            }
        };
        let cell = match message.body() {
            I2npBody::TunnelData(cell) => {
                p220_tunneldata_observed = true;
                cell.clone()
            }
            _ => continue,
        };
        let bytes = match dest.recover_garlic_bytes(coord.registry_mut(), &cell, wall_ms()) {
            Ok(bytes) => bytes,
            Err(_) => continue,
        };
        let envelope =
            I2npMessage::decode_standard(&bytes, MAX_I2NP_PAYLOAD_SIZE).expect("decode garlic");
        let now_secs = u32::try_from(wall_secs()).unwrap_or(u32::MAX);
        dispatcher.dispatch_garlic_envelope(
            &mut session,
            local_identity.id(),
            local_identity.static_secret_bytes(),
            &local_identity.static_public_bytes(),
            now_secs,
            &envelope,
            routing.lease_set2_store_mut(),
        );
        if let Some(queued) = dispatcher.pop_payload(local_identity.id()) {
            // Plan 192: parse both the 9-byte short-transport Data
            // envelope and the I2CP-style Data body Java writes
            // inside its Garlic clove.
            let decoded = decode_inbound_i2np(queued.bytes()).expect("decode queued");
            if let I2npBody::Data(body) = decoded.body() {
                let i2cp_body = i2pr_proto::decode_i2cp_data_body(body.payload.as_bytes())
                    .expect("decode I2CP Data body");
                inbound_payload = Some(i2cp_body.payload);
            }
        }
    }
    if let Some(reply) = inbound_payload.clone() {
        assert_eq!(reply, app_back, "inbound payload mismatch");
        append_evidence(
            &evidence_dir,
            "destination-inbound-received",
            &format!(
                "payload_len={} match=true pump_error={reply_pump_error}",
                reply.len()
            ),
        );
    } else {
        append_evidence(
            &evidence_dir,
            "destination-inbound-send-failed",
            &format!("send_status={inbound_send_status} (Plan 192 i2cp-wire-format-corrective)"),
        );
    }

    // Freeze the legacy 45-second payload-delivery result here. The
    // later status-only observation MUST NOT retroactively change
    // whether this row passed or failed.
    let frozen_tunneldata_45s = p220_tunneldata_observed;
    let frozen_payload_45s = inbound_payload
        .as_ref()
        .is_some_and(|reply| reply == app_back);

    // Plan 222 WP E — status-only polling extension. If no decisive
    // status has arrived, continue status-only polling until 70 seconds
    // from the tracked-send start. Do not resume or extend the i2pr
    // payload acceptance window after 45 seconds.
    if let Some(nonce) = tracked.as_ref().map(|t| t.nonce) {
        // Refresh once more at the freeze boundary.
        if let Some(events) = reference_control.send_status(nonce).await {
            p222_status_events = events;
        }
        let status_deadline = tracked_start + P222_STATUS_OBSERVATION_DEADLINE;
        while tokio::time::Instant::now() < status_deadline {
            let decisive = p222_status_events
                .iter()
                .any(|e| matches!(e.status, 3 | 4 | 16 | 17 | 19 | 20 | 21 | 22));
            if decisive {
                break;
            }
            // Absence of a terminal callback at/after the default
            // timeout is Unknown: the pinned client listener and router
            // timeout both default to 60 seconds, so the terminal
            // timeout notification may race listener expiration. Keep
            // polling until the 70-second diagnostic deadline.
            tokio::time::sleep(Duration::from_secs(1)).await;
            if let Some(events) = reference_control.send_status(nonce).await {
                // Retain the strongest later evidence while preserving
                // the full ordered sequence (later success supersedes
                // probable failure for delivery-path proof).
                if events.len() >= p222_status_events.len() {
                    p222_status_events = events;
                }
            }
        }
        append_evidence(
            &evidence_dir,
            "p222-status-only-observation",
            &format!(
                "nonce={nonce} events={:?} frozen_tunneldata_45s={frozen_tunneldata_45s} frozen_payload_45s={frozen_payload_45s}",
                p222_status_events
                    .iter()
                    .map(|e| e.status)
                    .collect::<Vec<_>>(),
            ),
        );
    }

    assert!(dest.note_direct_transport_attempt().is_err());
    append_evidence(&evidence_dir, "direct-rejected", "true");

    let mut scheduler = TunnelLivenessScheduler::new(LivenessConfig::plan_185_defaults());
    scheduler.advance_time(wall_ms());
    scheduler
        .register_pair(
            i2pr_tunnel::pool::TunnelSlot::from_raw(1),
            i2pr_tunnel::pool::TunnelSlot::from_raw(2),
        )
        .expect("pair");
    scheduler.advance_time(wall_ms() + FIRST_LIVENESS_DELAY_MS + 1);
    let action = scheduler.drive();
    assert!(matches!(action, LivenessAction::SendTest { .. }));
    append_evidence(&evidence_dir, "liveness-first-test", "passed");

    let snapshot = handle.snapshot();
    append_evidence(
        &evidence_dir,
        "manager-cleanup",
        &format!(
            "sessions_established={} active_sessions={} datagrams_received={} i2np_received={} protocol_drops={} auth_failures={} queue_drops={}",
            snapshot.sessions_established,
            snapshot.active_sessions,
            snapshot.datagrams_received,
            snapshot.i2np_received,
            snapshot.protocol_drops,
            snapshot.auth_failures,
            snapshot.inbound_queue_drops,
        ),
    );

    handle.shutdown();
    let _ = scope.shutdown().await;
    let final_snapshot = handle.snapshot();
    assert_eq!(final_snapshot.pending_outbound, 0);
    assert_eq!(final_snapshot.pending_inbound, 0);
    assert_eq!(final_snapshot.active_sessions, 0);
    append_evidence(&evidence_dir, "shutdown-baseline", "true");
    // Plan 220 §11 — fill the dispatch-epoch facts and emit exactly
    // one `p220-classification` row. The reverse payload outcome is
    // keyed ONLY on the reverse pump above: forward
    // `reference-received` MUST NOT satisfy it (D220-7).
    p220_facts.reverse_i2pr_tunneldata_observed = P220Observed::Known(p220_tunneldata_observed);
    p220_facts.reverse_i2pr_payload_recovered = P220Observed::Known(
        inbound_payload
            .as_ref()
            .is_some_and(|reply| reply == app_back),
    );
    let _ = record_p220_classification(&evidence_dir, &p220_facts);
    // Plan 222 WP F/H — status-to-fact mapping and exactly one P222
    // terminal per run. ACCEPTED alone does not pass client-NetDB; no
    // listener status is synthesized; missing callbacks stay Unknown;
    // the 70-second diagnostic deadline cannot alter the frozen
    // 45-second payload outcome.
    let (
        status_accepted,
        status_no_leaseset,
        status_bad_leaseset,
        status_expired_leaseset,
        status_unsupported_encryption,
        status_no_tunnels,
        status_best_effort_failure,
        status_guaranteed_success,
        status_other_terminal,
        ordered_statuses,
    ) = p222_status_facts(&p222_status_events);
    let p222_facts = P222Facts {
        selector_equivalent: p222_selector_equivalent,
        selector_nonempty: p222_selector_nonempty,
        target_leaseset_present_pre_send: p222_target_ls_pre_send,
        tracked_nonce: tracked_nonce_observed,
        status_accepted,
        status_no_leaseset,
        status_bad_leaseset,
        status_expired_leaseset,
        status_unsupported_encryption,
        status_no_tunnels,
        status_best_effort_failure,
        status_guaranteed_success,
        status_other_terminal,
        reverse_i2pr_tunneldata_observed_45s: P220Observed::Known(frozen_tunneldata_45s),
        reverse_i2pr_payload_recovered_45s: P220Observed::Known(frozen_payload_45s),
        ordered_statuses,
    };
    let _ = record_p222_classification(
        &evidence_dir,
        &p222_facts,
        p222_preflight.as_ref(),
        tracked.as_ref(),
        &reverse_sha256,
    );
    // Plan 223 WP B/C/G — exact Destination + branch discriminator and
    // exactly one P223 final terminal. Pre-send Java observations were
    // taken above; refresh the branch post-send only when status 17
    // persists so the C1–C3 intersection reflects the failure epoch.
    let status_17 = matches!(
        p222_facts.status_unsupported_encryption,
        P220Observed::Known(true)
    );
    let p223_preflight = p223_classify_preflight(
        p223_rust_dest.as_ref(),
        p223_helper_dest.as_ref(),
        status_17,
    );
    record_p223_destination(
        &evidence_dir,
        p223_rust_dest.as_ref(),
        p223_helper_dest.as_ref(),
        p223_router_dest.as_ref(),
        p223_preflight,
    );
    // Local LS2 facts for the same identity (post-fix must stay type-4).
    {
        let (enc_code, key_len, key_match) = match local_ls2.usable_x25519_key() {
            Ok(k) => (
                k.key_type().code(),
                k.as_bytes().len(),
                k.as_bytes() == &local_identity.static_public_bytes()[..],
            ),
            Err(_) => (0xFFFF, 0, false),
        };
        append_evidence(
            &evidence_dir,
            "p223-local-ls2",
            &format!(
                "ls_type=3 key_count={} enc_type_code={} key_len={} key_match={}",
                local_ls2.encryption_keys().len(),
                enc_code,
                key_len,
                key_match,
            ),
        );
    }
    let p223_branch_post = if status_17 {
        p223_collect_branch(
            diag_a_port,
            &helper_client_dbid_hex,
            &reverse_lookup_target_hex,
            &helper_client_dbid_hex,
        )
        .await
        .or(p223_branch_presend.clone())
    } else {
        p223_branch_presend.clone()
    };
    // Prefer the post-send refresh when status 17 persists; otherwise the
    // pre-send snapshot is the authoritative C record (or Unknown when
    // delivery passes and C is not required).
    let p223_branch_ref = if status_17 {
        p223_branch_post.as_ref().or(p223_branch_presend.as_ref())
    } else {
        p223_branch_presend.as_ref()
    };
    record_p223_branch(&evidence_dir, p223_branch_ref);
    // Decision table (§13 G1–G7).
    let p223_terminal = if frozen_payload_45s {
        P223Terminal::ReverseDeliveryPassed
    } else if status_17 {
        match p223_branch_ref {
            None => P223Terminal::Status17PersistsUnknown,
            Some(branch) if !branch.observable => P223Terminal::Status17PersistsUnknown,
            Some(branch) if !branch.source_keys_present => {
                P223Terminal::Status17PersistsSourceKeysMissing
            }
            Some(branch) if branch.source_keys_present && !branch.source_supports_x25519 => {
                P223Terminal::Status17PersistsSourceKeysNoX25519
            }
            Some(branch) if branch.target_ls_present && !branch.target_has_x25519 => {
                P223Terminal::Status17PersistsTargetLsNoX25519
            }
            Some(branch)
                if branch.source_keys_present
                    && branch.target_ls_present
                    && !branch.selected_key_present =>
            {
                P223Terminal::Status17PersistsNoKeyIntersection
            }
            Some(_) => P223Terminal::Status17PersistsUnknown,
        }
    } else if !p222_status_events.is_empty() {
        // Status 17 disappeared but another terminal appears (§13 G6).
        // Contradiction check: Java reports type-0 + selected X25519 yet
        // status 17 would have been G7; here status is non-17, so record
        // the next boundary honestly without a second corrective.
        P223Terminal::NextBoundary
    } else {
        // No payload and no decisive status: check for contradiction
        // (hash/type mismatch across parsers) else Unknown.
        let contradiction = match (
            p223_rust_dest.as_ref(),
            p223_helper_dest.as_ref(),
            p223_router_dest.as_ref(),
        ) {
            (Some(rust), Some(helper), _) if rust.hash_hex != helper.hash_hex => true,
            (Some(rust), _, Some(router)) if rust.hash_hex != router.hash_hex => true,
            _ => false,
        };
        if contradiction {
            P223Terminal::EvidenceContradiction
        } else {
            P223Terminal::Status17PersistsUnknown
        }
    };
    // For NEXT-BOUNDARY, include the ordered statuses in the detail.
    let p223_detail = if p223_terminal == P223Terminal::NextBoundary {
        format!(
            "ordered_statuses={:?} preflight={} hash_match={} frozen_payload_45s={}",
            p222_status_events
                .iter()
                .map(|e| e.status)
                .collect::<Vec<_>>(),
            p223_preflight.token(),
            match (&p223_rust_dest, &p223_helper_dest) {
                (Some(rust), Some(helper)) => rust.hash_hex == helper.hash_hex,
                _ => false,
            },
            frozen_payload_45s,
        )
    } else {
        format!(
            "preflight={} frozen_payload_45s={} frozen_tunneldata_45s={}",
            p223_preflight.token(),
            frozen_payload_45s,
            frozen_tunneldata_45s,
        )
    };
    let _ = record_p223_classification(&evidence_dir, p223_terminal, &p223_detail);
    // Plan 224 WP C/D — post-send lookup trace, post snapshots, terminal.
    // The frozen 45-second results above are inputs now, never
    // recomputed: trace collection MUST NOT retroactively change the
    // payload row (Plan 224 §15 item 22). Scratch Java logs are read
    // with fixed-substring matching only; only typed booleans reach
    // evidence (Plan 224 §8.1: raw lines may carry reply key/tag
    // material and stay scratch-only).
    let p224_a_log_dir: Option<PathBuf> = std::env::var("JAVA_A_LOG_DIR").ok().map(PathBuf::from);
    let p224_b_log_dir: Option<PathBuf> = std::env::var("JAVA_B_LOG_DIR").ok().map(PathBuf::from);
    if p224_a_log_dir.is_none() || p224_b_log_dir.is_none() {
        append_evidence(
            &evidence_dir,
            "p224-log-dir-unavailable",
            "JAVA_A_LOG_DIR or JAVA_B_LOG_DIR missing; trace is Unknown, never synthesized",
        );
    }
    let p224_scan_a = p224_a_log_dir.as_ref().and_then(|dir| {
        p224_target_b64.as_ref().and_then(|target_b64| {
            p224_router_b_b64.as_ref().and_then(|b_b64| {
                p224_helper_b64.as_deref().and_then(|helper_b64| {
                    p224_scan_log_dir(dir, target_b64, b_b64, Some(helper_b64))
                })
            })
        })
    });
    let p224_scan_b = p224_b_log_dir.as_ref().and_then(|dir| {
        p224_target_b64.as_ref().and_then(|target_b64| {
            p224_router_b_b64.as_ref().and_then(|b_b64| {
                p224_helper_b64.as_deref().and_then(|helper_b64| {
                    p224_scan_log_dir(dir, target_b64, b_b64, Some(helper_b64))
                })
            })
        })
    });
    let p224_logger_a_ok = p224_a_log_dir
        .as_ref()
        .is_some_and(|dir| p224_logger_config_installed(dir));
    let p224_logger_b_ok = p224_b_log_dir
        .as_ref()
        .is_some_and(|dir| p224_logger_config_installed(dir));
    if !p224_logger_a_ok || !p224_logger_b_ok {
        append_evidence(
            &evidence_dir,
            "p224-logger-config-unverified",
            "targeted logger.config not proven installed in both datadirs; trace absence is Unknown",
        );
    }
    let p224_trace = p224_build_trace(
        &reverse_lookup_target_hex,
        p224_target_b64.as_deref(),
        p224_router_b_b64.as_deref(),
        p224_helper_b64.as_deref(),
        p224_scan_a.as_ref(),
        p224_scan_b.as_ref(),
        p224_logger_a_ok,
        p224_logger_b_ok,
    );
    record_p224_trace(&evidence_dir, &p224_trace);
    let p224_a_post = p224_collect_client_ls(
        diag_a_port,
        &helper_client_dbid_hex,
        &reverse_lookup_target_hex,
    )
    .await;
    record_p224_client_snapshot(&evidence_dir, "p224-a-client-after", p224_a_post.as_ref());
    let p224_b_post = p224_collect_main_ls(diag_b_port, &reverse_lookup_target_hex).await;
    record_p224_main_snapshot(&evidence_dir, "p224-b-main-after", p224_b_post.as_ref());
    let p224_status_21 = matches!(p222_facts.status_no_leaseset, P220Observed::Known(true));
    let p224_terminal = p224_classify(
        &reverse_lookup_target_hex,
        p224_b_pre.as_ref(),
        p224_b_post.as_ref(),
        p224_a_pre.as_ref(),
        p224_a_post.as_ref(),
        &p224_trace,
        p224_status_21,
        &p222_facts.ordered_statuses,
        frozen_payload_45s,
    );
    let p224_b_answerable_pre = p224_b_pre.as_ref().map(p224_b_answerable);
    let _ = record_p224_classification(
        &evidence_dir,
        p224_terminal,
        &reverse_lookup_target_hex,
        p224_b_answerable_pre,
        &p222_facts.ordered_statuses,
        frozen_payload_45s,
        frozen_tunneldata_45s,
        p224_trace.reply_encryption_error_seen,
        p224_trace.observable,
    );
    let _ = PeerId::from_hash(java_hash);
}

/// Plan 220 — early-stop gap: the driver stopped before the
/// authoritative epoch (install-stalled or lease-stalled), so no
/// Java-state observation exists. Exactly one
/// `p220-classification` row is still emitted so every run closes
/// the attribution loop, but it is honestly an observability gap
/// at the HASH stage with the stop reason — never a root cause.
fn record_p220_early_stop_gap(evidence_dir: &Path, rust_b_hash_hex: &str, reason: &'static str) {
    let gap_bool: P220Observed<bool> = P220Observed::Unknown(reason);
    let gap_string: P220Observed<String> = P220Observed::Unknown(reason);
    let gap_count: P220Observed<u64> = P220Observed::Unknown(reason);
    let facts = P220Facts {
        rust_b_hash_hex: rust_b_hash_hex.to_owned(),
        java_b_self_hex: gap_string.clone(),
        java_b_self_b64: gap_string.clone(),
        hash_match: gap_bool,
        b_live_has_f: gap_bool,
        b_live_sha256: gap_string.clone(),
        b_live_published: gap_count,
        a_stored_present: gap_bool,
        a_stored_identity_match: gap_bool,
        a_stored_sha256: gap_string.clone(),
        a_stored_published: gap_count,
        a_stored_has_f: gap_bool,
        a_main_router_count: gap_count,
        a_peermanager_b_indexed: gap_bool,
        selector_observable: gap_bool,
        selector_kbucket_size: gap_count,
        selector_count: gap_count,
        selector_contains_b: gap_bool,
        client_lookup_started: gap_bool,
        client_lookup_peer_selected: gap_bool,
        client_lookup_succeeded: gap_bool,
        target_leaseset_present: gap_bool,
        target_lease_selected: gap_bool,
        outbound_client_tunnel_selected: gap_bool,
        garlic_constructed: gap_bool,
        tunnel_dispatch_submitted: gap_bool,
        forward_i2pr_to_java_received: gap_bool,
        reverse_java_send_admitted: gap_bool,
        reverse_i2pr_tunneldata_observed: gap_bool,
        reverse_i2pr_payload_recovered: gap_bool,
    };
    let _ = record_p220_classification(evidence_dir, &facts);
}

// ---- Plan 222 — corrected client-NetDB/OCMOSJ narrowing --------------------
// Plan 220's `SELECTOR Known(pass)` row used the raw target hash and
// hard-coded N=3 through the main facade; it is retained as historical
// evidence only. Plan 222 reproduces the exact client lookup (helper
// client DBID + Java-derived routing key + effective
// `netdb.searchLimit` + EXTRA_PEERS width through the
// production-equivalent selector overload) and correlates one reverse
// helper send through the public nonce-bearing
// `SendMessageStatusListener` path. The existing 45-second i2pr
// reverse-delivery acceptance window stays frozen; any later listener
// polling is status-only diagnostic observation bounded to 70 seconds
// from the tracked-send start and MUST NOT retroactively change the
// 45-second payload row.

/// Plan 222 status-only observation deadline: 70 seconds from the
/// tracked-send start. Rationale: Java default OCMOSJ overall timeout
/// is 60 seconds; the 10-second margin is diagnostic scheduling
/// tolerance; no Java timeout property is changed. The diagnostic
/// status deadline is >= 60 s and <= 75 s; the 45-second i2pr payload
/// window (`DATAGRAM_WAIT`) is frozen before this extension and no
/// reuse of the later deadline for the payload pass/fail row is
/// allowed.
const P222_STATUS_OBSERVATION_DEADLINE: Duration = Duration::from_secs(70);
/// Maximum tracked status events per nonce (helper + driver bound).
const P222_MAX_TRACKED_EVENTS: usize = 16;

/// Plan 222 tracked-send admission record from `TRACKED_SENT`.
#[derive(Clone, Debug, PartialEq, Eq)]
struct TrackedSend {
    nonce: u64,
    payload_len: usize,
    digest: String,
}

/// Plan 222 ordered per-message status event.
#[derive(Clone, Debug, PartialEq, Eq)]
struct TrackedStatusEvent {
    status: i32,
    elapsed_ms: u64,
}

/// Parses `TRACKED_SENT nonce=<u64> payload_len=<n> digest=<sha256>`.
/// Strict: missing nonce, malformed integer, malformed digest, or
/// duplicate malformed fields yield `None` (Unknown diagnostic
/// evidence, never a protocol failure).
fn p222_parse_tracked_sent(line: &str) -> Option<TrackedSend> {
    let body = line.strip_prefix("TRACKED_SENT ")?;
    let mut nonce: Option<u64> = None;
    let mut payload_len: Option<usize> = None;
    let mut digest: Option<String> = None;
    for field in body.split_whitespace() {
        let (key, value) = field.split_once('=')?;
        match key {
            "nonce" => {
                if nonce.is_some() {
                    return None;
                }
                nonce = Some(value.parse::<u64>().ok()?);
            }
            "payload_len" => {
                if payload_len.is_some() {
                    return None;
                }
                payload_len = Some(value.parse::<usize>().ok()?);
            }
            "digest" => {
                if digest.is_some() {
                    return None;
                }
                if value.len() != 64
                    || !value.bytes().all(|b| b.is_ascii_hexdigit())
                    || value.bytes().any(|b| b.is_ascii_uppercase())
                {
                    return None;
                }
                digest = Some(value.to_owned());
            }
            _ => return None,
        }
    }
    Some(TrackedSend {
        nonce: nonce?,
        payload_len: payload_len?,
        digest: digest?,
    })
}

/// Parses `TRACKED_STATUS nonce=<n> count=<n> events=<status:elapsed,...>`
/// or `count=0 events=none`. `TRACKED_STATUS_UNKNOWN` yields `None`.
/// Rejects more than 16 events and any malformed integer.
fn p222_parse_tracked_status(line: &str, expected_nonce: u64) -> Option<Vec<TrackedStatusEvent>> {
    if line.starts_with("TRACKED_STATUS_UNKNOWN") {
        return None;
    }
    let body = line.strip_prefix("TRACKED_STATUS ")?;
    let mut nonce: Option<u64> = None;
    let mut count: Option<usize> = None;
    let mut events_raw: Option<&str> = None;
    for field in body.split_whitespace() {
        let (key, value) = field.split_once('=')?;
        match key {
            "nonce" => {
                if nonce.is_some() {
                    return None;
                }
                nonce = Some(value.parse::<u64>().ok()?);
            }
            "count" => {
                if count.is_some() {
                    return None;
                }
                count = Some(value.parse::<usize>().ok()?);
            }
            "events" => {
                if events_raw.is_some() {
                    return None;
                }
                events_raw = Some(value);
            }
            _ => return None,
        }
    }
    if nonce? != expected_nonce {
        return None;
    }
    let count = count?;
    if count > P222_MAX_TRACKED_EVENTS {
        return None;
    }
    let events_raw = events_raw?;
    if count == 0 {
        if events_raw != "none" {
            return None;
        }
        return Some(Vec::new());
    }
    let parts: Vec<&str> = events_raw.split(',').collect();
    if parts.len() != count {
        return None;
    }
    if parts.len() > P222_MAX_TRACKED_EVENTS {
        return None;
    }
    let mut events = Vec::with_capacity(parts.len());
    for part in parts {
        let (status_raw, elapsed_raw) = part.split_once(':')?;
        let status: i32 = status_raw.parse().ok()?;
        let elapsed_ms: u64 = elapsed_raw.parse().ok()?;
        events.push(TrackedStatusEvent { status, elapsed_ms });
    }
    Some(events)
}

/// Plan 222 exact client-lookup preflight observation from one
/// `P222-CLIENT-LOOKUP-PREFLIGHT` response.
#[derive(Clone, Debug)]
struct P222Preflight {
    observable: bool,
    client_db_resolved: bool,
    client_db_is_client: bool,
    target_hash_hex: String,
    routing_key_hex: String,
    routing_key_differs: bool,
    target_ls_present_before_send: bool,
    facade_floodfill_enabled: bool,
    router_uptime_ms: u64,
    netdb_search_limit_effective: u64,
    selector_extra_peers: u64,
    selector_width: u64,
    selector_kbucket_size: u64,
    selector_count: u64,
    selector_contains_b: bool,
    selector_empty: bool,
}

fn p222_parse_preflight(line: &str) -> Option<P222Preflight> {
    let kv = p220_parse_kv(&line.replace("P222-EV ", "P220-EV "));
    let observable = kv.get("observable").is_some_and(|v| v == "true");
    let get_bool = |key: &str| kv.get(key).is_some_and(|v| v == "true");
    let get_u64 = |key: &str| kv.get(key).and_then(|v| v.parse::<u64>().ok());
    Some(P222Preflight {
        observable,
        client_db_resolved: get_bool("client_db_resolved"),
        client_db_is_client: get_bool("client_db_is_client"),
        target_hash_hex: kv.get("target_hash_hex").cloned().unwrap_or_default(),
        routing_key_hex: kv.get("routing_key_hex").cloned().unwrap_or_default(),
        routing_key_differs: get_bool("routing_key_differs"),
        target_ls_present_before_send: get_bool("target_ls_present_before_send"),
        facade_floodfill_enabled: get_bool("facade_floodfill_enabled"),
        router_uptime_ms: get_u64("router_uptime_ms").unwrap_or(u64::MAX),
        netdb_search_limit_effective: get_u64("netdb_search_limit_effective").unwrap_or(u64::MAX),
        selector_extra_peers: get_u64("selector_extra_peers").unwrap_or(u64::MAX),
        selector_width: get_u64("selector_width").unwrap_or(u64::MAX),
        selector_kbucket_size: get_u64("selector_input_kbucket_size").unwrap_or(u64::MAX),
        selector_count: get_u64("selector_count").unwrap_or(u64::MAX),
        selector_contains_b: get_bool("selector_contains_b"),
        selector_empty: get_bool("selector_empty"),
    })
}

/// Queries the exact client-lookup preflight at the authoritative
/// epoch. `client_dbid_hex` is the helper source Destination hash
/// (the `fromLocalDest` / client DBID used by OCMOSJ); `target_hex`
/// is the i2pr destination hash the reverse lookup must resolve;
/// `router_b_hex` is Router B. Returns `None` when the diagnostic is
/// unreachable (Unknown, never a protocol fact).
async fn p222_collect_preflight(
    diag_port: u16,
    client_dbid_hex: &str,
    target_hex: &str,
    router_b_hex: &str,
) -> Option<P222Preflight> {
    let line = p220_query_diagnostic(
        diag_port,
        &format!("P222-CLIENT-LOOKUP-PREFLIGHT {client_dbid_hex} {target_hex} {router_b_hex}"),
    )
    .await?;
    if !line.starts_with("P222-EV ") {
        return None;
    }
    p222_parse_preflight(&line)
}

/// Plan 222 terminal taxonomy. Exactly one terminal per run; no
/// additional terminal strings may be invented during execution.
#[derive(Clone, Debug, PartialEq, Eq)]
enum P222Terminal {
    ObservabilityGapPreEpoch,
    ObservabilityGapSelectorEquivalence,
    CorrectedAttributionClientNetdbNoLookupPeer,
    CorrectedAttributionClientNetdbNoUsableLeaseset,
    CorrectedAttributionOcmOSJBadLeaseset,
    CorrectedAttributionOcmOSJExpiredLeaseset,
    CorrectedAttributionOcmOSJUnsupportedEncryption,
    CorrectedAttributionOcmOSJNoUsableTunnelOrGarlicPath,
    CorrectedAttributionJavaDispatchPathReachedI2prTunneldataNotObserved,
    ObservabilityGapOcmOSJPostAccept,
    EvidenceContradictionAckSuccessWithoutI2prPayload,
    ReverseDeliveryPassed,
}

impl P222Terminal {
    fn token(&self) -> &'static str {
        match self {
            Self::ObservabilityGapPreEpoch => "P222-OBSERVABILITY-GAP-PRE-EPOCH",
            Self::ObservabilityGapSelectorEquivalence => {
                "P222-OBSERVABILITY-GAP-SELECTOR-EQUIVALENCE"
            }
            Self::CorrectedAttributionClientNetdbNoLookupPeer => {
                "P222-CORRECTED-ATTRIBUTION CLIENT-NETDB-NO-LOOKUP-PEER"
            }
            Self::CorrectedAttributionClientNetdbNoUsableLeaseset => {
                "P222-CORRECTED-ATTRIBUTION CLIENT-NETDB-NO-USABLE-LEASESET"
            }
            Self::CorrectedAttributionOcmOSJBadLeaseset => {
                "P222-CORRECTED-ATTRIBUTION OCMOSJ-BAD-LEASESET"
            }
            Self::CorrectedAttributionOcmOSJExpiredLeaseset => {
                "P222-CORRECTED-ATTRIBUTION OCMOSJ-EXPIRED-LEASESET"
            }
            Self::CorrectedAttributionOcmOSJUnsupportedEncryption => {
                "P222-CORRECTED-ATTRIBUTION OCMOSJ-UNSUPPORTED-ENCRYPTION"
            }
            Self::CorrectedAttributionOcmOSJNoUsableTunnelOrGarlicPath => {
                "P222-CORRECTED-ATTRIBUTION OCMOSJ-NO-USABLE-TUNNEL-OR-GARLIC-PATH"
            }
            Self::CorrectedAttributionJavaDispatchPathReachedI2prTunneldataNotObserved => {
                "P222-CORRECTED-ATTRIBUTION JAVA-DISPATCH-PATH-REACHED-I2PR-TUNNELDATA-NOT-OBSERVED"
            }
            Self::ObservabilityGapOcmOSJPostAccept => "P222-OBSERVABILITY-GAP-OCMOSJ-POST-ACCEPT",
            Self::EvidenceContradictionAckSuccessWithoutI2prPayload => {
                "P222-EVIDENCE-CONTRADICTION-ACK-SUCCESS-WITHOUT-I2PR-PAYLOAD"
            }
            Self::ReverseDeliveryPassed => "P222-REVERSE-DELIVERY-PASSED",
        }
    }
}

/// Plan 222 facts separate from frozen P220 facts.
#[derive(Clone, Debug)]
struct P222Facts {
    selector_equivalent: P220Observed<bool>,
    selector_nonempty: P220Observed<bool>,
    target_leaseset_present_pre_send: P220Observed<bool>,
    tracked_nonce: P220Observed<u64>,
    status_accepted: P220Observed<bool>,
    status_no_leaseset: P220Observed<bool>,
    status_bad_leaseset: P220Observed<bool>,
    status_expired_leaseset: P220Observed<bool>,
    status_unsupported_encryption: P220Observed<bool>,
    status_no_tunnels: P220Observed<bool>,
    status_best_effort_failure: P220Observed<bool>,
    status_guaranteed_success: P220Observed<bool>,
    status_other_terminal: P220Observed<bool>,
    reverse_i2pr_tunneldata_observed_45s: P220Observed<bool>,
    reverse_i2pr_payload_recovered_45s: P220Observed<bool>,
    ordered_statuses: Vec<i32>,
}

impl P222Facts {
    fn derive(&self) -> P222Terminal {
        // Reverse-delivery fast path first: a digest-matched payload
        // within the frozen 45-second window proves the chain passed.
        if matches!(
            self.reverse_i2pr_payload_recovered_45s,
            P220Observed::Known(true)
        ) {
            // A later GUARANTEED_SUCCESS with no payload is a
            // contradiction, but payload presence wins as delivery proof.
            // If both payload and guaranteed success are present the run
            // passed; contradiction fires only when success arrived yet
            // the frozen 45-second payload row still failed.
            if matches!(self.status_guaranteed_success, P220Observed::Known(true)) {
                return P222Terminal::ReverseDeliveryPassed;
            }
            return P222Terminal::ReverseDeliveryPassed;
        }
        // Selector equivalence must hold before any OCMOSJ attribution.
        match self.selector_equivalent {
            P220Observed::Known(true) => {}
            _ => return P222Terminal::ObservabilityGapSelectorEquivalence,
        }
        // Empty exact selector + actual client DB: no lookup peer, since
        // pinned client-DB `getAllRouters()` fallback is empty.
        match self.selector_nonempty {
            P220Observed::Known(false) => {
                return P222Terminal::CorrectedAttributionClientNetdbNoLookupPeer;
            }
            P220Observed::Known(true) => {}
            P220Observed::Unknown(_) => {
                return P222Terminal::ObservabilityGapSelectorEquivalence;
            }
        }
        // Tracked nonce required for any per-message attribution.
        if !matches!(self.tracked_nonce, P220Observed::Known(_)) {
            return P222Terminal::ObservabilityGapOcmOSJPostAccept;
        }
        // Exact failure statuses map only to documented boundaries.
        // ACCEPTED alone proves admission only.
        if matches!(self.status_no_leaseset, P220Observed::Known(true)) {
            return P222Terminal::CorrectedAttributionClientNetdbNoUsableLeaseset;
        }
        if matches!(self.status_bad_leaseset, P220Observed::Known(true)) {
            return P222Terminal::CorrectedAttributionOcmOSJBadLeaseset;
        }
        if matches!(self.status_expired_leaseset, P220Observed::Known(true)) {
            return P222Terminal::CorrectedAttributionOcmOSJExpiredLeaseset;
        }
        if matches!(
            self.status_unsupported_encryption,
            P220Observed::Known(true)
        ) {
            return P222Terminal::CorrectedAttributionOcmOSJUnsupportedEncryption;
        }
        if matches!(self.status_no_tunnels, P220Observed::Known(true)) {
            return P222Terminal::CorrectedAttributionOcmOSJNoUsableTunnelOrGarlicPath;
        }
        if matches!(self.status_guaranteed_success, P220Observed::Known(true)) {
            // OCMOSJ delivery-ACK path passed yet the frozen 45-second
            // i2pr payload row failed: evidence contradiction.
            return P222Terminal::EvidenceContradictionAckSuccessWithoutI2prPayload;
        }
        if matches!(self.status_best_effort_failure, P220Observed::Known(true)) {
            // Post-garlic dispatch path reached (SendTimeoutJob). If i2pr
            // saw no TunnelData in the frozen window, the boundary is
            // dispatch-reached / tunneldata-not-observed. Do not claim
            // the packet left the kernel.
            match self.reverse_i2pr_tunneldata_observed_45s {
                P220Observed::Known(false) => {
                    return P222Terminal::CorrectedAttributionJavaDispatchPathReachedI2prTunneldataNotObserved;
                }
                _ => return P222Terminal::ObservabilityGapOcmOSJPostAccept,
            }
        }
        // No decisive terminal status by the 70-second deadline, or an
        // undocumented status code: observability gap. Absence of a
        // callback is Unknown, never dispatch proof.
        P222Terminal::ObservabilityGapOcmOSJPostAccept
    }
}

/// Derives P222 selector-equivalence from a parsed preflight. The old
/// P220-only observation (raw hash + N=3, no client DBID) can never
/// satisfy this: feeding only it yields the selector-equivalence gap.
fn p222_selector_equivalence(preflight: Option<&P222Preflight>) -> P220Observed<bool> {
    let Some(pre) = preflight else {
        return P220Observed::Unknown("preflight-unreachable");
    };
    if !pre.observable {
        return P220Observed::Unknown("preflight-not-observable");
    }
    if !pre.client_db_resolved || !pre.client_db_is_client {
        return P220Observed::Unknown("client-db-not-proven");
    }
    if pre.routing_key_hex.is_empty() || pre.target_hash_hex.is_empty() {
        return P220Observed::Unknown("routing-key-absent");
    }
    // Raw target hash rejected as selector key when the routing key is
    // available: equivalence requires both recorded and normally
    // differing. A caller that supplies only the raw hash fails here.
    if !pre.routing_key_differs
        && pre
            .routing_key_hex
            .eq_ignore_ascii_case(&pre.target_hash_hex)
    {
        // Equal keys are suspicious (routing keys normally differ);
        // without a distinct routing key the selector is not proven
        // production-equivalent.
        return P220Observed::Known(false);
    }
    // N=3 hard-code rejected: the width must equal the effective
    // `netdb.searchLimit` + EXTRA_PEERS(1) as emitted by Java.
    if pre.selector_extra_peers != 1 {
        return P220Observed::Known(false);
    }
    if pre.selector_width != pre.netdb_search_limit_effective.saturating_add(1) {
        return P220Observed::Known(false);
    }
    if pre.selector_width == 0 || pre.selector_width > 8 {
        return P220Observed::Known(false);
    }
    P220Observed::Known(true)
}

/// Maps an ordered nonce-correlated status sequence to P222 status
/// facts. Later success supersedes probable failure for
/// strongest-evidence evaluation, but the full ordered sequence is
/// preserved in `ordered_statuses`. Missing callbacks stay Unknown.
#[allow(clippy::type_complexity)]
fn p222_status_facts(
    events: &[TrackedStatusEvent],
) -> (
    P220Observed<bool>,
    P220Observed<bool>,
    P220Observed<bool>,
    P220Observed<bool>,
    P220Observed<bool>,
    P220Observed<bool>,
    P220Observed<bool>,
    P220Observed<bool>,
    P220Observed<bool>,
    Vec<i32>,
) {
    let ordered: Vec<i32> = events.iter().map(|e| e.status).collect();
    let has = |code: i32| ordered.contains(&code);
    // STATUS_SEND_ACCEPTED = 1 proves admission only.
    let accepted = if has(1) {
        P220Observed::Known(true)
    } else {
        P220Observed::Unknown("accepted-not-observed")
    };
    let known = |code: i32| {
        if has(code) {
            P220Observed::Known(true)
        } else {
            P220Observed::Known(false)
        }
    };
    // 21 NO_LEASESET, 19 BAD, 20 EXPIRED, 17 UNSUPPORTED, 16 NO_TUNNELS,
    // 3 BEST_EFFORT_FAILURE, 4 GUARANTEED_SUCCESS. Code 22 (META) and
    // other terminals (15,18,23,…) map to other_terminal with the exact
    // code preserved in `ordered_statuses`.
    let no_leaseset = known(21);
    let bad = known(19);
    let expired = known(20);
    let unsupported = known(17);
    let no_tunnels = known(16);
    let best_effort = known(3);
    let guaranteed = known(4);
    let documented = [1, 3, 4, 16, 17, 19, 20, 21];
    let other = if ordered.iter().any(|s| !documented.contains(s)) {
        P220Observed::Known(true)
    } else if ordered.is_empty() {
        P220Observed::Unknown("no-terminal-callback")
    } else {
        P220Observed::Known(false)
    };
    (
        accepted,
        no_leaseset,
        bad,
        expired,
        unsupported,
        no_tunnels,
        best_effort,
        guaranteed,
        other,
        ordered,
    )
}

/// Emits exactly one `p222-classification` row plus supporting P222
/// evidence rows. Returns the terminal token.
fn record_p222_classification(
    evidence_dir: &Path,
    facts: &P222Facts,
    preflight: Option<&P222Preflight>,
    tracked: Option<&TrackedSend>,
    reverse_sha256: &str,
) -> String {
    let terminal = facts.derive();
    if let Some(pre) = preflight {
        append_evidence(
            evidence_dir,
            "p222-client-lookup-preflight",
            &format!(
                "observable={} client_db_resolved={} client_db_is_client={} target_hash_hex={} routing_key_hex={} routing_key_differs={} target_ls_present_before_send={} facade_floodfill_enabled={} router_uptime_ms={} netdb_search_limit_effective={} selector_extra_peers={} selector_width={} selector_kbucket_size={} selector_count={} selector_contains_b={} selector_empty={}",
                pre.observable,
                pre.client_db_resolved,
                pre.client_db_is_client,
                pre.target_hash_hex,
                pre.routing_key_hex,
                pre.routing_key_differs,
                pre.target_ls_present_before_send,
                pre.facade_floodfill_enabled,
                pre.router_uptime_ms,
                pre.netdb_search_limit_effective,
                pre.selector_extra_peers,
                pre.selector_width,
                pre.selector_kbucket_size,
                pre.selector_count,
                pre.selector_contains_b,
                pre.selector_empty,
            ),
        );
    } else {
        append_evidence(
            evidence_dir,
            "p222-client-lookup-preflight",
            "observable=false reason=preflight-unreachable",
        );
    }
    if let Some(tracked) = tracked {
        append_evidence(
            evidence_dir,
            "p222-tracked-send",
            &format!(
                "nonce={} payload_len={} digest={} reverse_sha256={} ordered_statuses={:?}",
                tracked.nonce,
                tracked.payload_len,
                tracked.digest,
                reverse_sha256,
                facts.ordered_statuses,
            ),
        );
    } else {
        append_evidence(
            evidence_dir,
            "p222-tracked-send",
            "nonce=unknown reason=tracked-send-not-admitted",
        );
    }
    append_evidence(
        evidence_dir,
        "p222-classification",
        &format!(
            "{} selector_equivalent={:?} selector_nonempty={:?} target_leaseset_present_pre_send={:?} tracked_nonce={:?} status_accepted={:?} status_no_leaseset={:?} status_bad_leaseset={:?} status_expired_leaseset={:?} status_unsupported_encryption={:?} status_no_tunnels={:?} status_best_effort_failure={:?} status_guaranteed_success={:?} status_other_terminal={:?} reverse_i2pr_tunneldata_observed_45s={:?} reverse_i2pr_payload_recovered_45s={:?} reverse_sha256={}",
            terminal.token(),
            facts.selector_equivalent,
            facts.selector_nonempty,
            facts.target_leaseset_present_pre_send,
            facts.tracked_nonce,
            facts.status_accepted,
            facts.status_no_leaseset,
            facts.status_bad_leaseset,
            facts.status_expired_leaseset,
            facts.status_unsupported_encryption,
            facts.status_no_tunnels,
            facts.status_best_effort_failure,
            facts.status_guaranteed_success,
            facts.status_other_terminal,
            facts.reverse_i2pr_tunneldata_observed_45s,
            facts.reverse_i2pr_payload_recovered_45s,
            reverse_sha256,
        ),
    );
    terminal.token().to_owned()
}

/// Plan 222 — early-stop gap before the authoritative epoch. A
/// pre-epoch stop (install-stalled or lease-stalled) cannot prove
/// selector equivalence; the terminal is the pre-epoch observability
/// gap, never a root-cause attribution.
fn record_p222_early_stop_gap(evidence_dir: &Path, reason: &'static str) {
    append_evidence(
        evidence_dir,
        "p222-client-lookup-preflight",
        &format!("observable=false reason={reason}"),
    );
    append_evidence(
        evidence_dir,
        "p222-tracked-send",
        &format!("nonce=unknown reason={reason}"),
    );
    append_evidence(
        evidence_dir,
        "p222-classification",
        &format!(
            "{} reason={reason}",
            P222Terminal::ObservabilityGapPreEpoch.token()
        ),
    );
}

/// Plan 223 WP B — exact Destination observation (Rust + Java).
#[derive(Clone, Debug)]
struct P223Dest {
    hash_hex: String,
    enc_type_code: i32,
    enc_type_name: String,
    public_key_len: usize,
    sig_type_code: i32,
}

fn p223_parse_dest_info(line: &str) -> Option<P223Dest> {
    let body = line.strip_prefix("DEST_INFO ")?;
    let mut hash_hex: Option<String> = None;
    let mut enc_type_code: Option<i32> = None;
    let mut enc_type_name: Option<String> = None;
    let mut public_key_len: Option<usize> = None;
    let mut sig_type_code: Option<i32> = None;
    for field in body.split_whitespace() {
        let (key, value) = field.split_once('=')?;
        match key {
            "hash_hex" => {
                if value.len() != 64 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
                    return None;
                }
                hash_hex = Some(value.to_lowercase());
            }
            "enc_type_code" => enc_type_code = Some(value.parse().ok()?),
            "enc_type_name" => enc_type_name = Some(value.to_owned()),
            "public_key_len" => public_key_len = Some(value.parse().ok()?),
            "sig_type_code" => sig_type_code = Some(value.parse().ok()?),
            _ => return None,
        }
    }
    Some(P223Dest {
        hash_hex: hash_hex?,
        enc_type_code: enc_type_code?,
        enc_type_name: enc_type_name?,
        public_key_len: public_key_len?,
        sig_type_code: sig_type_code?,
    })
}

fn p223_parse_router_dest_inspect(line: &str) -> Option<P223Dest> {
    // `P223-EV kind=dest-inspect ...` — reuse the DEST_INFO parser after
    // normalising the prefix.
    let normalised = line
        .replace("P223-EV kind=dest-inspect ", "DEST_INFO ")
        .replace("P223-EV ", "DEST_INFO ");
    // The router row carries `observable=true/false`; strip it for parsing.
    let filtered: String = normalised
        .split_whitespace()
        .filter(|f| {
            !f.starts_with("observable=") && !f.starts_with("kind=") && !f.starts_with("reason=")
        })
        .collect::<Vec<_>>()
        .join(" ");
    // Ensure DEST_INFO prefix remains.
    let with_prefix = if filtered.starts_with("DEST_INFO ") {
        filtered
    } else {
        format!("DEST_INFO {filtered}")
    };
    p223_parse_dest_info(&with_prefix)
}

/// Plan 223 WP C — bounded status-17 branch discriminator.
#[derive(Clone, Debug)]
struct P223Branch {
    observable: bool,
    source_keys_present: bool,
    source_supported_types: String,
    source_supports_elgamal: bool,
    source_supports_x25519: bool,
    target_ls_present: bool,
    target_ls_type: String,
    target_destination_hash_match: bool,
    target_destination_enc_type: i32,
    target_key_count: usize,
    target_key_types: String,
    target_has_x25519: bool,
    selected_key_present: bool,
    selected_key_type: i32,
}

fn p223_parse_branch(line: &str) -> Option<P223Branch> {
    let kv = p220_parse_kv(&line.replace("P223-EV ", "P220-EV "));
    let observable = kv.get("observable").is_some_and(|v| v == "true");
    let get_bool = |key: &str| kv.get(key).is_some_and(|v| v == "true");
    let get_i32 = |key: &str| {
        kv.get(key)
            .and_then(|v| v.parse::<i32>().ok())
            .unwrap_or(-1)
    };
    let get_usize = |key: &str| {
        kv.get(key)
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(usize::MAX)
    };
    Some(P223Branch {
        observable,
        source_keys_present: get_bool("source_keys_present"),
        source_supported_types: kv
            .get("source_supported_types")
            .cloned()
            .unwrap_or_default(),
        source_supports_elgamal: get_bool("source_supports_elgamal"),
        source_supports_x25519: get_bool("source_supports_x25519"),
        target_ls_present: get_bool("target_ls_present"),
        target_ls_type: kv.get("target_ls_type").cloned().unwrap_or_default(),
        target_destination_hash_match: get_bool("target_destination_hash_match"),
        target_destination_enc_type: get_i32("target_destination_enc_type"),
        target_key_count: get_usize("target_key_count"),
        target_key_types: kv.get("target_key_types").cloned().unwrap_or_default(),
        target_has_x25519: get_bool("target_has_x25519"),
        selected_key_present: get_bool("selected_key_present"),
        selected_key_type: get_i32("selected_key_type"),
    })
}

async fn p223_collect_router_dest_inspect(diag_port: u16, dest_b64: &str) -> Option<P223Dest> {
    let line = p220_query_diagnostic(diag_port, &format!("P223-DEST-INSPECT {dest_b64}")).await?;
    if !line.starts_with("P223-EV ") {
        return None;
    }
    p223_parse_router_dest_inspect(&line)
}

async fn p223_collect_branch(
    diag_port: u16,
    client_dbid_hex: &str,
    target_hex: &str,
    source_hex: &str,
) -> Option<P223Branch> {
    let line = p220_query_diagnostic(
        diag_port,
        &format!("P223-BRANCH {client_dbid_hex} {target_hex} {source_hex}"),
    )
    .await?;
    if !line.starts_with("P223-EV ") {
        return None;
    }
    p223_parse_branch(&line)
}

/// Plan 223 pre-fix classifier (gate, not final terminal).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum P223Preflight {
    Confirmed,
    NotConfirmed,
    ObservabilityGap,
}

impl P223Preflight {
    fn token(self) -> &'static str {
        match self {
            Self::Confirmed => "P223-PREFLIGHT-DESTINATION-ENC-GUARD-CONFIRMED",
            Self::NotConfirmed => "P223-PREFLIGHT-DESTINATION-ENC-GUARD-NOT-CONFIRMED",
            Self::ObservabilityGap => "P223-PREFLIGHT-OBSERVABILITY-GAP",
        }
    }
}

fn p223_classify_preflight(
    rust: Option<&P223Dest>,
    java: Option<&P223Dest>,
    status_17_observed: bool,
) -> P223Preflight {
    let (Some(rust), Some(java)) = (rust, java) else {
        return P223Preflight::ObservabilityGap;
    };
    if rust.hash_hex != java.hash_hex
        || rust.enc_type_code != java.enc_type_code
        || rust.public_key_len != java.public_key_len
    {
        return P223Preflight::ObservabilityGap;
    }
    if java.enc_type_code != 0 && status_17_observed {
        P223Preflight::Confirmed
    } else if java.enc_type_code == 0 {
        P223Preflight::NotConfirmed
    } else {
        P223Preflight::ObservabilityGap
    }
}

/// Plan 223 final terminals (§5 post-corrective).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum P223Terminal {
    ReverseDeliveryPassed,
    Status17PersistsSourceKeysMissing,
    Status17PersistsSourceKeysNoX25519,
    Status17PersistsTargetLsNoX25519,
    Status17PersistsNoKeyIntersection,
    Status17PersistsUnknown,
    NextBoundary,
    EvidenceContradiction,
}

impl P223Terminal {
    fn token(self) -> &'static str {
        match self {
            Self::ReverseDeliveryPassed => "P223-REVERSE-DELIVERY-PASSED",
            Self::Status17PersistsSourceKeysMissing => "P223-STATUS17-PERSISTS-SOURCE-KEYS-MISSING",
            Self::Status17PersistsSourceKeysNoX25519 => {
                "P223-STATUS17-PERSISTS-SOURCE-KEYS-NO-X25519"
            }
            Self::Status17PersistsTargetLsNoX25519 => "P223-STATUS17-PERSISTS-TARGET-LS2-NO-X25519",
            Self::Status17PersistsNoKeyIntersection => "P223-STATUS17-PERSISTS-NO-KEY-INTERSECTION",
            Self::Status17PersistsUnknown => "P223-STATUS17-PERSISTS-UNKNOWN",
            Self::NextBoundary => "P223-NEXT-BOUNDARY",
            Self::EvidenceContradiction => "P223-EVIDENCE-CONTRADICTION",
        }
    }
}

fn record_p223_destination(
    evidence_dir: &Path,
    rust: Option<&P223Dest>,
    helper: Option<&P223Dest>,
    router: Option<&P223Dest>,
    preflight: P223Preflight,
) {
    if let Some(rust) = rust {
        append_evidence(
            evidence_dir,
            "p223-rust-destination",
            &format!(
                "hash_hex={} enc_type_code={} public_key_len={} sig_type_code={}",
                rust.hash_hex, rust.enc_type_code, rust.public_key_len, rust.sig_type_code
            ),
        );
    } else {
        append_evidence(
            evidence_dir,
            "p223-rust-destination",
            "observable=false reason=rust-destination-unavailable",
        );
    }
    if let Some(helper) = helper {
        append_evidence(
            evidence_dir,
            "p223-java-helper-destination",
            &format!(
                "hash_hex={} enc_type_code={} enc_type_name={} public_key_len={} sig_type_code={}",
                helper.hash_hex,
                helper.enc_type_code,
                helper.enc_type_name,
                helper.public_key_len,
                helper.sig_type_code
            ),
        );
    } else {
        append_evidence(
            evidence_dir,
            "p223-java-helper-destination",
            "observable=false reason=helper-inspect-unreachable",
        );
    }
    if let Some(router) = router {
        append_evidence(
            evidence_dir,
            "p223-java-router-destination",
            &format!(
                "hash_hex={} enc_type_code={} enc_type_name={} public_key_len={} sig_type_code={}",
                router.hash_hex,
                router.enc_type_code,
                router.enc_type_name,
                router.public_key_len,
                router.sig_type_code
            ),
        );
    } else {
        append_evidence(
            evidence_dir,
            "p223-java-router-destination",
            "observable=false reason=router-inspect-unreachable",
        );
    }
    let hash_match = match (rust, helper, router) {
        (Some(rust), Some(helper), Some(router)) => {
            rust.hash_hex == helper.hash_hex && rust.hash_hex == router.hash_hex
        }
        (Some(rust), Some(helper), None) => rust.hash_hex == helper.hash_hex,
        _ => false,
    };
    append_evidence(
        evidence_dir,
        "p223-destination-match",
        &format!("hash_match={} preflight={}", hash_match, preflight.token()),
    );
}

fn record_p223_branch(evidence_dir: &Path, branch: Option<&P223Branch>) {
    if let Some(branch) = branch {
        append_evidence(
            evidence_dir,
            "p223-branch",
            &format!(
                "observable={} source_keys_present={} source_supported_types={} source_supports_elgamal={} source_supports_x25519={} target_ls_present={} target_ls_type={} target_destination_hash_match={} target_destination_enc_type={} target_key_count={} target_key_types={} target_has_x25519={} selected_key_present={} selected_key_type={}",
                branch.observable,
                branch.source_keys_present,
                branch.source_supported_types,
                branch.source_supports_elgamal,
                branch.source_supports_x25519,
                branch.target_ls_present,
                branch.target_ls_type,
                branch.target_destination_hash_match,
                branch.target_destination_enc_type,
                branch.target_key_count,
                branch.target_key_types,
                branch.target_has_x25519,
                branch.selected_key_present,
                branch.selected_key_type,
            ),
        );
    } else {
        append_evidence(
            evidence_dir,
            "p223-branch",
            "observable=false reason=branch-unreachable",
        );
    }
}

fn record_p223_classification(evidence_dir: &Path, terminal: P223Terminal, detail: &str) -> String {
    append_evidence(
        evidence_dir,
        "p223-classification",
        &format!("{} {detail}", terminal.token()),
    );
    terminal.token().to_owned()
}

// ---- Plan 224 — NO_LEASESET lookup-path attribution ------------------------
// Pinned Java I2P 2.13.0 (`9134f808337b401e8e53c73734c81fab04280c9d`)
// source authority:
//
// - `OutboundClientMessageOneShotJob` maps an ordinary client-NetDB
//   lookup failure to `STATUS_SEND_FAILURE_NO_LEASESET (21)` after a
//   15-second `LS_LOOKUP_TIMEOUT`; status 21 proves only that no
//   usable target LeaseSet reached the send path, not which stage
//   failed (Plan 224 §3.1);
// - a client LS lookup runs `IterativeSearchJob` with the helper's
//   outbound/inbound client tunnels and fails fast with
//   `failed, no IB client tunnel to receive reply` or
//   `skipped, no ratchet/elg support` when the reply path cannot
//   form (Plan 224 §3.2);
// - `HandleDatabaseLookupMessageJob` answers an LS DLM only from a
//   main-NetDB LeaseSet with `getReceivedAsPublished() == true`,
//   logging `We have the published LS <target>, answering query`
//   (Plan 224 §3.3);
// - an LS DSM arriving down a client tunnel is tagged
//   `receivedBy=<client>` and routed into that exact client sub-DB,
//   logging `Storing garlic LS down tunnel for: <target> sent to:
//   <client>` (Plan 224 §3.4);
// - `InNetMessagePool` stores a matching DSM inline before the
//   lookup-success reply job runs, so a store-vs-success scheduling
//   race is NOT an authorized hypothesis (Plan 224 §3.5).
//
// Attribution only: no production i2pr wire change, no Java
// mutation, no topology/tunnel/selector/SAM change, no second
// lookup that could prime the helper client DB. The frozen
// 45-second i2pr payload window (`DATAGRAM_WAIT`) is captured
// BEFORE trace collection and is never derived from or altered by
// it. Snapshots are local read-only diagnostic commands
// (`P224-MAIN-LS`, `P224-CLIENT-LS`); the trace sanitizer below
// reads scratch Java logs with fixed-substring matching only and
// emits bounded typed booleans/counts plus hashes — never raw log
// lines, never session keys/tags, never payloads.

/// Plan 224 Router-B main-NetDB LS snapshot from `P224-MAIN-LS`.
#[derive(Clone, Debug)]
struct P224MainLs {
    target_hash_hex: String,
    raw_present: bool,
    validated_present: bool,
    entry_type: i64,
    received_as_published: Option<bool>,
    received_as_reply: Option<bool>,
    received_by_hex: String,
    ls2_unpublished: String,
    lease_count: i64,
    key_count: i64,
    key_types: String,
    latest_lease_ms: u64,
    current: Option<bool>,
}

/// Plan 224 Router-A helper client-subDB LS snapshot from
/// `P224-CLIENT-LS`.
#[derive(Clone, Debug)]
struct P224ClientLs {
    client_dbid_hex: String,
    target_hash_hex: String,
    client_db_resolved: bool,
    client_db_is_client: bool,
    raw_present: bool,
    validated_present: bool,
    entry_type: i64,
    received_as_published: Option<bool>,
    received_as_reply: Option<bool>,
    received_by_hex: String,
    ls2_unpublished: String,
    lease_count: i64,
    key_count: i64,
    key_types: String,
    latest_lease_ms: u64,
    current: Option<bool>,
}

fn p224_kv_bool(map: &std::collections::HashMap<String, String>, key: &str) -> Option<bool> {
    match map.get(key).map(String::as_str) {
        Some("true") => Some(true),
        Some("false") => Some(false),
        _ => None,
    }
}

fn p224_kv_i64(map: &std::collections::HashMap<String, String>, key: &str) -> i64 {
    map.get(key)
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(-1)
}

fn p224_kv_u64(map: &std::collections::HashMap<String, String>, key: &str) -> u64 {
    map.get(key)
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0)
}

fn p224_is_hex64(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Parses `P224-EV kind=main-ls ...`. Returns `None` for error lines
/// or unparseable input (Unknown, never a protocol fact). An absent
/// target (`observable=true raw_present=false validated_present=false`)
/// still parses: absence is a fact, not a parse failure.
fn p224_parse_main_ls(line: &str) -> Option<P224MainLs> {
    if !line.starts_with("P224-EV ") || !line.contains("kind=main-ls") {
        return None;
    }
    let kv = p220_parse_kv(&line.replace("P224-EV ", "P220-EV "));
    if !kv.get("observable").is_some_and(|v| v == "true") {
        return None;
    }
    let target = kv.get("target_hash_hex").cloned().unwrap_or_default();
    if !p224_is_hex64(&target) {
        return None;
    }
    Some(P224MainLs {
        target_hash_hex: target.to_lowercase(),
        raw_present: kv.get("raw_present").is_some_and(|v| v == "true"),
        validated_present: kv.get("validated_present").is_some_and(|v| v == "true"),
        entry_type: p224_kv_i64(&kv, "entry_type"),
        received_as_published: p224_kv_bool(&kv, "received_as_published"),
        received_as_reply: p224_kv_bool(&kv, "received_as_reply"),
        received_by_hex: kv
            .get("received_by_hex")
            .cloned()
            .unwrap_or_else(|| "unknown".to_owned()),
        ls2_unpublished: kv
            .get("ls2_unpublished")
            .cloned()
            .unwrap_or_else(|| "unknown".to_owned()),
        lease_count: p224_kv_i64(&kv, "lease_count"),
        key_count: p224_kv_i64(&kv, "key_count"),
        key_types: kv
            .get("key_types")
            .cloned()
            .unwrap_or_else(|| "unknown".to_owned()),
        latest_lease_ms: p224_kv_u64(&kv, "latest_lease_ms"),
        current: p224_kv_bool(&kv, "current"),
    })
}

/// Parses `P224-EV kind=client-ls ...`. A main-DB fallback
/// (`observable=false`) yields `None`: fallback snapshots are
/// Unknown/invalid for Plan-224 client authority, never usable LS
/// facts.
fn p224_parse_client_ls(line: &str) -> Option<P224ClientLs> {
    if !line.starts_with("P224-EV ") || !line.contains("kind=client-ls") {
        return None;
    }
    let kv = p220_parse_kv(&line.replace("P224-EV ", "P220-EV "));
    if !kv.get("observable").is_some_and(|v| v == "true") {
        return None;
    }
    if !kv.get("client_db_resolved").is_some_and(|v| v == "true")
        || !kv.get("client_db_is_client").is_some_and(|v| v == "true")
    {
        return None;
    }
    let target = kv.get("target_hash_hex").cloned().unwrap_or_default();
    if !p224_is_hex64(&target) {
        return None;
    }
    Some(P224ClientLs {
        client_dbid_hex: kv
            .get("client_dbid_hex")
            .cloned()
            .unwrap_or_default()
            .to_lowercase(),
        target_hash_hex: target.to_lowercase(),
        client_db_resolved: true,
        client_db_is_client: true,
        raw_present: kv.get("raw_present").is_some_and(|v| v == "true"),
        validated_present: kv.get("validated_present").is_some_and(|v| v == "true"),
        entry_type: p224_kv_i64(&kv, "entry_type"),
        received_as_published: p224_kv_bool(&kv, "received_as_published"),
        received_as_reply: p224_kv_bool(&kv, "received_as_reply"),
        received_by_hex: kv
            .get("received_by_hex")
            .cloned()
            .unwrap_or_else(|| "unknown".to_owned()),
        ls2_unpublished: kv
            .get("ls2_unpublished")
            .cloned()
            .unwrap_or_else(|| "unknown".to_owned()),
        lease_count: p224_kv_i64(&kv, "lease_count"),
        key_count: p224_kv_i64(&kv, "key_count"),
        key_types: kv
            .get("key_types")
            .cloned()
            .unwrap_or_else(|| "unknown".to_owned()),
        latest_lease_ms: p224_kv_u64(&kv, "latest_lease_ms"),
        current: p224_kv_bool(&kv, "current"),
    })
}

async fn p224_collect_main_ls(diag_port: u16, target_hex: &str) -> Option<P224MainLs> {
    let line = p220_query_diagnostic(diag_port, &format!("P224-MAIN-LS {target_hex}")).await?;
    p224_parse_main_ls(&line)
}

async fn p224_collect_client_ls(
    diag_port: u16,
    client_dbid_hex: &str,
    target_hex: &str,
) -> Option<P224ClientLs> {
    let line = p220_query_diagnostic(
        diag_port,
        &format!("P224-CLIENT-LS {client_dbid_hex} {target_hex}"),
    )
    .await?;
    p224_parse_client_ls(&line)
}

/// Renders one hash exactly as the pinned JVM logs it
/// (`Hash.toBase64()`, I2P alphabet) via the stateless
/// `P224-HASH-B64` diagnostic. Returns `None` when unreachable or
/// when the echoed hex does not match the request (Unknown, never a
/// guessed rendering: reimplementing the alphabet in Rust would risk
/// silent correlation mismatch).
/// Parses `P224-EV kind=hash-b64 ...`. Returns the JVM rendering
/// only when the echoed hex matches the request exactly (a mismatch
/// is Unknown, never a guessed rendering).
fn p224_parse_hash_b64(line: &str, expected_hex: &str) -> Option<String> {
    if !line.starts_with("P224-EV ") || !line.contains("kind=hash-b64") {
        return None;
    }
    let kv = p220_parse_kv(&line.replace("P224-EV ", "P220-EV "));
    if !kv.get("observable").is_some_and(|v| v == "true") {
        return None;
    }
    let echoed = kv.get("hash_hex").cloned().unwrap_or_default();
    if !echoed.eq_ignore_ascii_case(expected_hex) {
        return None;
    }
    let b64 = kv.get("hash_b64").cloned().unwrap_or_default();
    if b64.is_empty()
        || b64.len() > 64
        || !b64
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'~' || b == b'=')
    {
        return None;
    }
    Some(b64)
}

async fn p224_collect_hash_b64(diag_port: u16, hash_hex: &str) -> Option<String> {
    let line = p220_query_diagnostic(diag_port, &format!("P224-HASH-B64 {hash_hex}")).await?;
    p224_parse_hash_b64(&line, hash_hex)
}

/// Plan 224 sanitized lookup-trace facts (Plan 224 §9). Only
/// exact-target-correlated booleans may be authoritative, except
/// `reply_encryption_error_seen`, which is explicitly supporting-only
/// (the pinned log token carries no target key).
#[derive(Clone, Debug, Default)]
struct P224Trace {
    observable: bool,
    target_hash_hex: String,
    query_started: bool,
    query_to_b: bool,
    query_via_client_reply_tunnel: bool,
    no_ib_client_tunnel: bool,
    no_ratchet_or_elg_support: bool,
    search_success: bool,
    search_failed: bool,
    b_lookup_received: bool,
    b_published_ls_answered: bool,
    /// Router-B DLM logging proven live by at least one handled
    /// lookup of any key this run. Required only by the D4
    /// not-received terminal, the sole terminal proving a negative
    /// on Router B.
    b_dlm_proven_live: bool,
    a_client_tunnel_ls_received: bool,
    reply_encryption_error_seen: bool,
}

/// Bounds for the whitelist-only scratch-log scan. ISJ tries are
/// bounded by the search width plus retries (dozens per run); the
/// caps below are orders of magnitude above any legitimate lookup
/// epoch and exist only so a hostile or screenful log cannot drive
/// unbounded driver memory.
const P224_MAX_LOG_FILES_PER_ROUTER: usize = 16;
const P224_MAX_LOG_BYTES_PER_ROUTER: u64 = 96 * 1024 * 1024;

#[derive(Clone, Debug, Default)]
struct P224LogScan {
    files_read: usize,
    bytes_scanned: u64,
    truncated: bool,
    isj_try: u64,
    isj_try_to_b: u64,
    isj_try_client_tunnel: u64,
    isj_no_ib: u64,
    isj_no_crypto: u64,
    isj_success: u64,
    isj_failed: u64,
    a_client_tunnel_ls: u64,
    b_lookup: u64,
    b_answered: u64,
    b_reply_enc_err: u64,
    a_isj_any: u64,
    b_dlm_any: u64,
}

/// Whitelist-only scan of one router's `log-router-*.txt` scratch
/// files. Every predicate is a fixed-substring match (`str::contains`,
/// never regex); only bounded counts leave this function. Raw lines
/// are never retained, never logged, never written to evidence: a
/// `HandleDatabaseLookupMessageJob` INFO line can carry ephemeral
/// reply key/tag material (Plan 224 §8.1), so even matched lines stay
/// inside this function.
fn p224_scan_log_dir(
    dir: &Path,
    target_b64: &str,
    router_b_b64: &str,
    helper_b64: Option<&str>,
) -> Option<P224LogScan> {
    let mut entries: Vec<PathBuf> = Vec::new();
    let read_dir = std::fs::read_dir(dir).ok()?;
    for entry in read_dir {
        let entry = entry.ok()?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with("log-router-") || !name.ends_with(".txt") {
            continue;
        }
        entries.push(entry.path());
    }
    if entries.is_empty() {
        return None;
    }
    entries.sort();
    entries.truncate(P224_MAX_LOG_FILES_PER_ROUTER);
    let for_ls_target = format!("for LS {target_b64}");
    let isj_for_target_success = format!("ISJ for {target_b64} successful");
    let isj_for_target_failed = format!("ISJ for {target_b64} failed");
    let storing_ls_target = format!("Storing garlic LS down tunnel for: {target_b64}");
    let storing_ls_helper = helper_b64.map(|helper| format!("sent to: {helper}"));
    let handling_lookup_target = format!("Handling database lookup message for {target_b64}");
    let answering_ls_target = format!("We have the published LS {target_b64}, answering query");
    let mut scan = P224LogScan::default();
    for path in entries {
        let bytes = std::fs::read(&path).ok()?;
        if scan.bytes_scanned.saturating_add(bytes.len() as u64) > P224_MAX_LOG_BYTES_PER_ROUTER {
            scan.truncated = true;
            break;
        }
        scan.files_read += 1;
        scan.bytes_scanned += bytes.len() as u64;
        // Lossy conversion is safe here: correlation tokens are pure
        // ASCII (Base64 + English log scaffolding), so a non-UTF8
        // byte can never hide or forge a match.
        let text = String::from_utf8_lossy(&bytes);
        for line in text.lines() {
            if line.contains("ISJ ") {
                scan.a_isj_any += 1;
            }
            if line.contains("Handling database lookup message for ") {
                scan.b_dlm_any += 1;
            }
            if !line.contains(target_b64) {
                if line.contains("DLM reply encryption error") {
                    scan.b_reply_enc_err += 1;
                }
                continue;
            }
            if line.contains("ISJ try ") && line.contains(&for_ls_target) {
                scan.isj_try += 1;
                if line.contains(router_b_b64) {
                    scan.isj_try_to_b += 1;
                }
                if line.contains("reply via client tunnel? true") {
                    scan.isj_try_client_tunnel += 1;
                }
            }
            if line.contains(&for_ls_target)
                && line.contains("failed, no IB client tunnel to receive reply")
            {
                scan.isj_no_ib += 1;
            }
            if line.contains(&for_ls_target) && line.contains("skipped, no ratchet/elg support") {
                scan.isj_no_crypto += 1;
            }
            if line.contains(&isj_for_target_success) {
                scan.isj_success += 1;
            }
            if line.contains(&isj_for_target_failed) {
                scan.isj_failed += 1;
            }
            if line.contains(&storing_ls_target)
                && storing_ls_helper
                    .as_ref()
                    .is_some_and(|helper| line.contains(helper))
            {
                scan.a_client_tunnel_ls += 1;
            }
            if line.contains(&handling_lookup_target) {
                scan.b_lookup += 1;
            }
            if line.contains(&answering_ls_target) {
                scan.b_answered += 1;
            }
            if line.contains("DLM reply encryption error") {
                scan.b_reply_enc_err += 1;
            }
        }
    }
    Some(scan)
}

/// Builds the sanitized trace from the two routers' scratch-log
/// scans. `observable` is true only when every correlation input is
/// proven: all three exact Base64 renderings available, both log dirs readable
/// with at least one log file each, byte caps respected, Router-A
/// ISJ logging proven live by at least one `ISJ ` line (without it,
/// absence of a target line cannot distinguish "never queried" from
/// "logging off"), and the targeted `logger.config` verified
/// installed in both datadirs (the file the harness writes before
/// router startup; the driver checks the exact three
/// `logger.record.` scopes through `<logdir>/../logger.config`).
fn p224_build_trace(
    target_hash_hex: &str,
    target_b64: Option<&str>,
    router_b_b64: Option<&str>,
    helper_b64: Option<&str>,
    scan_a: Option<&P224LogScan>,
    scan_b: Option<&P224LogScan>,
    logger_config_a_ok: bool,
    logger_config_b_ok: bool,
) -> P224Trace {
    let mut trace = P224Trace {
        observable: false,
        target_hash_hex: target_hash_hex.to_owned(),
        ..P224Trace::default()
    };
    let (Some(target_b64), Some(_), Some(helper_b64)) = (target_b64, router_b_b64, helper_b64)
    else {
        return trace;
    };
    let (Some(scan_a), Some(scan_b)) = (scan_a, scan_b) else {
        return trace;
    };
    if target_b64.is_empty() || helper_b64.is_empty() || scan_a.truncated || scan_b.truncated {
        return trace;
    }
    if scan_a.files_read == 0 || scan_b.files_read == 0 {
        return trace;
    }
    if !logger_config_a_ok || !logger_config_b_ok {
        return trace;
    }
    // Positive control: Router-A ISJ logging must have emitted at
    // least once this run. The helper client lookup is only one of
    // many ISJ epochs (tunnel-build searches precede it), so zero
    // `ISJ ` lines means the INFO override never applied and every
    // absence below would be meaningless.
    if scan_a.a_isj_any == 0 {
        return trace;
    }
    let _ = (target_hash_hex, helper_b64);
    trace.observable = true;
    trace.query_started = scan_a.isj_try > 0;
    trace.query_to_b = scan_a.isj_try_to_b > 0;
    trace.query_via_client_reply_tunnel = scan_a.isj_try_client_tunnel > 0;
    trace.no_ib_client_tunnel = scan_a.isj_no_ib > 0;
    trace.no_ratchet_or_elg_support = scan_a.isj_no_crypto > 0;
    trace.search_success = scan_a.isj_success > 0;
    trace.search_failed = scan_a.isj_failed > 0;
    trace.b_lookup_received = scan_b.b_lookup > 0;
    trace.b_published_ls_answered = scan_b.b_answered > 0;
    trace.b_dlm_proven_live = scan_b.b_dlm_any > 0;
    trace.a_client_tunnel_ls_received = scan_a.a_client_tunnel_ls > 0;
    trace.reply_encryption_error_seen = scan_a.b_reply_enc_err > 0 || scan_b.b_reply_enc_err > 0;
    trace
}

/// Verifies the targeted Plan-224 `logger.config` the harness writes
/// into each scratch router datadir before startup. The log dir is
/// `<datadir>/logs`, so the config is its sibling. Requires the
/// exact default level plus the three exact class scopes; any
/// deviation yields false (Unknown, never assumed active).
fn p224_logger_config_installed(log_dir: &Path) -> bool {
    let config_path = log_dir.join("..").join("logger.config");
    let Ok(bytes) = std::fs::read(&config_path) else {
        return false;
    };
    if bytes.len() > 65536 {
        return false;
    }
    let text = String::from_utf8_lossy(&bytes);
    text.contains("logger.defaultLevel=ERROR")
        && text.contains("logger.record.net.i2p.router.networkdb.kademlia.IterativeSearchJob=INFO")
        && text
            .contains("logger.record.net.i2p.router.networkdb.HandleDatabaseLookupMessageJob=DEBUG")
        && text.contains("logger.record.net.i2p.router.tunnel.InboundMessageDistributor=INFO")
}

/// Plan 224 terminal taxonomy (Plan 224 §13). Exactly one terminal
/// per authoritative attempt, in earliest-proven-failing-stage
/// priority order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum P224Terminal {
    AttributionBMainLsAbsent,
    AttributionBMainLsInvalidOrStale,
    AttributionBMainLsNotQueryAnswerable,
    AttributionANoInboundClientReplyTunnel,
    AttributionALookupReplyCryptoUnavailable,
    AttributionASearchExhaustedWithoutQueryingB,
    AttributionAToBLookupNotReceived,
    EvidenceContradictionBAnswerableButNotAnswered,
    AttributionBReplyNotObservedOnAClientTunnel,
    EvidenceContradictionClientTunnelDsmNotInClientDb,
    EvidenceContradictionNoLeasesetWithUsableClientLs,
    NextBoundary,
    ReverseDeliveryPassed,
    ObservabilityGapLookupPath,
}

impl P224Terminal {
    fn token(self) -> &'static str {
        match self {
            Self::AttributionBMainLsAbsent => "P224-ATTRIBUTION-B-MAIN-LS-ABSENT",
            Self::AttributionBMainLsInvalidOrStale => "P224-ATTRIBUTION-B-MAIN-LS-INVALID-OR-STALE",
            Self::AttributionBMainLsNotQueryAnswerable => {
                "P224-ATTRIBUTION-B-MAIN-LS-NOT-QUERY-ANSWERABLE"
            }
            Self::AttributionANoInboundClientReplyTunnel => {
                "P224-ATTRIBUTION-A-NO-INBOUND-CLIENT-REPLY-TUNNEL"
            }
            Self::AttributionALookupReplyCryptoUnavailable => {
                "P224-ATTRIBUTION-A-LOOKUP-REPLY-CRYPTO-UNAVAILABLE"
            }
            Self::AttributionASearchExhaustedWithoutQueryingB => {
                "P224-ATTRIBUTION-A-SEARCH-EXHAUSTED-WITHOUT-QUERYING-B"
            }
            Self::AttributionAToBLookupNotReceived => "P224-ATTRIBUTION-A-TO-B-LOOKUP-NOT-RECEIVED",
            Self::EvidenceContradictionBAnswerableButNotAnswered => {
                "P224-EVIDENCE-CONTRADICTION-B-ANSWERABLE-BUT-NOT-ANSWERED"
            }
            Self::AttributionBReplyNotObservedOnAClientTunnel => {
                "P224-ATTRIBUTION-B-REPLY-NOT-OBSERVED-ON-A-CLIENT-TUNNEL"
            }
            Self::EvidenceContradictionClientTunnelDsmNotInClientDb => {
                "P224-EVIDENCE-CONTRADICTION-CLIENT-TUNNEL-DSM-NOT-IN-CLIENT-DB"
            }
            Self::EvidenceContradictionNoLeasesetWithUsableClientLs => {
                "P224-EVIDENCE-CONTRADICTION-NO-LEASESET-WITH-USABLE-CLIENT-LS"
            }
            Self::NextBoundary => "P224-NEXT-BOUNDARY",
            Self::ReverseDeliveryPassed => "P224-REVERSE-DELIVERY-PASSED",
            Self::ObservabilityGapLookupPath => "P224-OBSERVABILITY-GAP-LOOKUP-PATH",
        }
    }
}

/// Pinned answerability rule (`HandleDatabaseLookupMessageJob`): a
/// floodfill answers an LS DLM only from a validated LeaseSet that
/// is current and marked received-as-published. Raw presence alone,
/// a stale entry, or a non-published entry never answers.
fn p224_b_answerable(snap: &P224MainLs) -> bool {
    snap.validated_present && snap.current == Some(true) && snap.received_as_published == Some(true)
}

/// Plan 224 terminal classifier (Plan 224 §13 D1–D11). Inputs are the
/// pre/post Router-B main snapshots, the pre/post Router-A helper
/// client snapshots, the sanitized exact-target trace, the
/// nonce-correlated ordered status sequence, and the frozen 45-second
/// payload result. The frozen result is an input, never derived
/// here: trace collection MUST NOT retroactively change it (the
/// caller freezes `frozen_payload_45s` before any P224 observation).
#[allow(clippy::too_many_arguments)]
fn p224_classify(
    expected_target_hex: &str,
    b_pre: Option<&P224MainLs>,
    b_post: Option<&P224MainLs>,
    a_pre: Option<&P224ClientLs>,
    a_post: Option<&P224ClientLs>,
    trace: &P224Trace,
    status_no_leaseset_21: bool,
    ordered_statuses: &[i32],
    frozen_payload_45s: bool,
) -> P224Terminal {
    // D10. Digest-matched reverse payload inside the frozen window
    // proves the chain passed, regardless of lookup-trace shape.
    if frozen_payload_45s {
        return P224Terminal::ReverseDeliveryPassed;
    }
    // D1 gate. The Router-B pre-send snapshot is mandatory before
    // any search-path attribution; it must carry the exact tracked
    // target hash (snapshots are never reused across attempts).
    let Some(b_pre) = b_pre else {
        return P224Terminal::ObservabilityGapLookupPath;
    };
    if !b_pre
        .target_hash_hex
        .eq_ignore_ascii_case(expected_target_hex)
    {
        return P224Terminal::ObservabilityGapLookupPath;
    };
    // B1. The controlled publication submission left no raw LS in
    // Router B's main NetDB at the authoritative pre-send epoch.
    if !b_pre.raw_present {
        return P224Terminal::AttributionBMainLsAbsent;
    }
    // B2. Raw present but not validated/current: invalid or stale.
    if !b_pre.validated_present || b_pre.current != Some(true) {
        return P224Terminal::AttributionBMainLsInvalidOrStale;
    }
    // B3. Valid and current but not received-as-published: pinned
    // Java will not answer an LS DLM from this entry.
    if b_pre.received_as_published != Some(true) {
        return P224Terminal::AttributionBMainLsNotQueryAnswerable;
    }
    // B4. Answerable: continue down the lookup path. Every deeper
    // terminal needs the pre-send helper client snapshot (exact
    // target, proven client facade) plus an observable trace.
    let Some(a_pre) = a_pre else {
        return P224Terminal::ObservabilityGapLookupPath;
    };
    if !a_pre
        .target_hash_hex
        .eq_ignore_ascii_case(expected_target_hex)
        || !a_pre.client_db_resolved
        || !a_pre.client_db_is_client
    {
        return P224Terminal::ObservabilityGapLookupPath;
    }
    if !trace.observable
        || !trace
            .target_hash_hex
            .eq_ignore_ascii_case(expected_target_hex)
    {
        return P224Terminal::ObservabilityGapLookupPath;
    }
    // D2. The search cannot form a client lookup: no usable inbound
    // client reply tunnel, or no ratchet/ElGamal reply capability.
    // These trace facts imply lookup failure without any status
    // condition, and they are the earliest lookup-path stage.
    if trace.no_ib_client_tunnel {
        return P224Terminal::AttributionANoInboundClientReplyTunnel;
    }
    if trace.no_ratchet_or_elg_support {
        return P224Terminal::AttributionALookupReplyCryptoUnavailable;
    }
    // D3. B answerable, the lookup started, but the exact trace
    // proves B was never queried while the search failed with
    // status 21. Selector membership alone never satisfies
    // `query_to_b`: only an exact-target ISJ try line carrying
    // Router B's hash does.
    if trace.query_started && !trace.query_to_b && trace.search_failed && status_no_leaseset_21 {
        return P224Terminal::AttributionASearchExhaustedWithoutQueryingB;
    }
    // D4. A dispatched the query to B but B never observed it. This
    // is the only terminal that proves a negative on Router B, so it
    // additionally requires B-side DLM logging proven live
    // (`b_dlm_proven_live`: B handled and logged at least one lookup
    // this run); otherwise the absence could be logging-off rather
    // than delivery failure, and the honest terminal is the gap.
    if trace.query_to_b && !trace.b_lookup_received && trace.search_failed {
        if trace.b_dlm_proven_live {
            return P224Terminal::AttributionAToBLookupNotReceived;
        }
        return P224Terminal::ObservabilityGapLookupPath;
    }
    // D5. B received the query but did not answer despite answerable
    // pre-send state. Consult the post-send snapshot: a state change
    // re-derives the B terminal from post-send state (with the
    // transition recorded by the caller); a still-answerable entry
    // is an evidence contradiction.
    if trace.b_lookup_received && !trace.b_published_ls_answered {
        match b_post {
            Some(post)
                if post
                    .target_hash_hex
                    .eq_ignore_ascii_case(expected_target_hex) =>
            {
                if p224_b_answerable(post) {
                    return P224Terminal::EvidenceContradictionBAnswerableButNotAnswered;
                }
                if !post.raw_present {
                    return P224Terminal::AttributionBMainLsAbsent;
                }
                if !post.validated_present || post.current != Some(true) {
                    return P224Terminal::AttributionBMainLsInvalidOrStale;
                }
                return P224Terminal::AttributionBMainLsNotQueryAnswerable;
            }
            _ => return P224Terminal::ObservabilityGapLookupPath,
        }
    }
    // D6. B answered but A never observed the target LS on the
    // helper inbound client tunnel, with status 21 observed. The
    // non-target-specific `reply_encryption_error_seen` bit is
    // supporting evidence only, recorded by the caller.
    if trace.b_published_ls_answered && !trace.a_client_tunnel_ls_received && status_no_leaseset_21
    {
        return P224Terminal::AttributionBReplyNotObservedOnAClientTunnel;
    }
    // D7/D8 need the post-send helper client-subDB state. A missing
    // post snapshot is incomplete evidence, not a protocol fact.
    let Some(a_post) = a_post else {
        return P224Terminal::ObservabilityGapLookupPath;
    };
    if !a_post
        .target_hash_hex
        .eq_ignore_ascii_case(expected_target_hex)
        || !a_post.client_db_resolved
        || !a_post.client_db_is_client
    {
        return P224Terminal::ObservabilityGapLookupPath;
    }
    // D7. Pinned source tags the client-tunnel DSM with
    // `receivedBy=helper`, routes it to that client DB, and stores
    // it inline before the lookup reply job runs: a received DSM
    // with a still-absent client LS is an evidence contradiction,
    // never an assumed scheduling race.
    if trace.a_client_tunnel_ls_received && !a_post.validated_present {
        return P224Terminal::EvidenceContradictionClientTunnelDsmNotInClientDb;
    }
    // D8. The client DB holds the target yet OCMOSJ still emits 21.
    if a_post.validated_present && status_no_leaseset_21 {
        return P224Terminal::EvidenceContradictionNoLeasesetWithUsableClientLs;
    }
    // D9. Status 21 disappeared and another terminal status appears:
    // a new boundary owned by a successor plan. ACCEPTED (1) alone
    // is admission, not a terminal; an empty sequence is Unknown.
    if !status_no_leaseset_21
        && ordered_statuses.iter().any(|s| *s != 1)
        && !ordered_statuses.is_empty()
    {
        return P224Terminal::NextBoundary;
    }
    // D11. None of the above can be proven.
    P224Terminal::ObservabilityGapLookupPath
}

fn record_p224_snapshot_row(evidence_dir: &Path, label: &str, detail: &str) {
    append_evidence(evidence_dir, label, detail);
}

fn record_p224_main_snapshot(evidence_dir: &Path, label: &str, snap: Option<&P224MainLs>) {
    match snap {
        Some(s) => record_p224_snapshot_row(
            evidence_dir,
            label,
            &format!(
                "target_hash_hex={} raw_present={} validated_present={} entry_type={} received_as_published={} received_as_reply={} received_by_hex={} ls2_unpublished={} lease_count={} key_count={} key_types={} latest_lease_ms={} current={}",
                s.target_hash_hex,
                s.raw_present,
                s.validated_present,
                s.entry_type,
                s.received_as_published
                    .map(|b| b.to_string())
                    .as_deref()
                    .unwrap_or("unknown"),
                s.received_as_reply
                    .map(|b| b.to_string())
                    .as_deref()
                    .unwrap_or("unknown"),
                s.received_by_hex,
                s.ls2_unpublished,
                s.lease_count,
                s.key_count,
                s.key_types,
                s.latest_lease_ms,
                s.current
                    .map(|b| b.to_string())
                    .as_deref()
                    .unwrap_or("unknown"),
            ),
        ),
        None => record_p224_snapshot_row(
            evidence_dir,
            label,
            "observable=false reason=snapshot-unreachable",
        ),
    }
}

fn record_p224_client_snapshot(evidence_dir: &Path, label: &str, snap: Option<&P224ClientLs>) {
    match snap {
        Some(s) => record_p224_snapshot_row(
            evidence_dir,
            label,
            &format!(
                "client_dbid_hex={} target_hash_hex={} observable=true client_db_resolved={} client_db_is_client={} raw_present={} validated_present={} entry_type={} received_as_published={} received_as_reply={} received_by_hex={} ls2_unpublished={} lease_count={} key_count={} key_types={} latest_lease_ms={} current={}",
                s.client_dbid_hex,
                s.target_hash_hex,
                s.client_db_resolved,
                s.client_db_is_client,
                s.raw_present,
                s.validated_present,
                s.entry_type,
                s.received_as_published
                    .map(|b| b.to_string())
                    .as_deref()
                    .unwrap_or("unknown"),
                s.received_as_reply
                    .map(|b| b.to_string())
                    .as_deref()
                    .unwrap_or("unknown"),
                s.received_by_hex,
                s.ls2_unpublished,
                s.lease_count,
                s.key_count,
                s.key_types,
                s.latest_lease_ms,
                s.current
                    .map(|b| b.to_string())
                    .as_deref()
                    .unwrap_or("unknown"),
            ),
        ),
        None => record_p224_snapshot_row(
            evidence_dir,
            label,
            "observable=false reason=snapshot-unreachable-or-main-fallback",
        ),
    }
}

fn record_p224_trace(evidence_dir: &Path, trace: &P224Trace) {
    append_evidence(
        evidence_dir,
        "p224-lookup-trace",
        &format!(
            "observable={} target_hash_hex={} query_started={} query_to_b={} query_via_client_reply_tunnel={} no_ib_client_tunnel={} no_ratchet_or_elg_support={} search_success={} search_failed={} b_lookup_received={} b_published_ls_answered={} b_dlm_proven_live={} a_client_tunnel_ls_received={} reply_encryption_error_seen={}",
            trace.observable,
            if trace.target_hash_hex.is_empty() {
                "unknown"
            } else {
                &trace.target_hash_hex
            },
            trace.query_started,
            trace.query_to_b,
            trace.query_via_client_reply_tunnel,
            trace.no_ib_client_tunnel,
            trace.no_ratchet_or_elg_support,
            trace.search_success,
            trace.search_failed,
            trace.b_lookup_received,
            trace.b_published_ls_answered,
            trace.b_dlm_proven_live,
            trace.a_client_tunnel_ls_received,
            trace.reply_encryption_error_seen,
        ),
    );
}

/// Emits exactly one `p224-classification` row per authoritative
/// attempt. Returns the terminal token.
#[allow(clippy::too_many_arguments)]
fn record_p224_classification(
    evidence_dir: &Path,
    terminal: P224Terminal,
    expected_target_hex: &str,
    b_answerable_pre: Option<bool>,
    ordered_statuses: &[i32],
    frozen_payload_45s: bool,
    frozen_tunneldata_45s: bool,
    reply_encryption_error_seen: bool,
    trace_observable: bool,
) -> String {
    append_evidence(
        evidence_dir,
        "p224-classification",
        &format!(
            "{} target_hash_hex={} b_main_ls_answerable_pre_send={} ordered_statuses={:?} frozen_tunneldata_45s={} frozen_payload_45s={} reply_encryption_error_seen={} trace_observable={}",
            terminal.token(),
            expected_target_hex,
            b_answerable_pre
                .map(|b| b.to_string())
                .as_deref()
                .unwrap_or("unknown"),
            ordered_statuses,
            frozen_tunneldata_45s,
            frozen_payload_45s,
            reply_encryption_error_seen,
            trace_observable,
        ),
    );
    terminal.token().to_owned()
}

/// Plan 224 — pre-epoch stop before the authoritative epoch. No
/// P224 snapshot exists, so the terminal is honestly the lookup-path
/// observability gap, never a root-cause attribution.
fn record_p224_early_stop_gap(evidence_dir: &Path, reason: &'static str) {
    append_evidence(
        evidence_dir,
        "p224-b-main-before-send",
        &format!("observable=false reason={reason}"),
    );
    append_evidence(
        evidence_dir,
        "p224-a-client-before-send",
        &format!("observable=false reason={reason}"),
    );
    append_evidence(
        evidence_dir,
        "p224-lookup-trace",
        &format!("observable=false target_hash_hex=unknown reason={reason}"),
    );
    append_evidence(
        evidence_dir,
        "p224-classification",
        &format!(
            "{} reason={reason}",
            P224Terminal::ObservabilityGapLookupPath.token()
        ),
    );
}

/// Plan 199 §A.5 — full Streaming matrix (Direction A + B) against
/// the exact-pinned Java I2P 2.13.0 reference. Same wire as the i2pd
/// first-family driver.
#[tokio::test]
#[ignore = "Plan 194: requires exact-pinned external Java I2P environment"]
async fn streaming_through_java() {
    let java_ri_path = env_path("JAVA_ROUTER_INFO");
    let java_endpoint: SocketAddr = env_value("JAVA_SSU2_ENDPOINT").parse().expect("endpoint");
    let service_ri_path = env_path("JAVA_SERVICE_ROUTER_INFO");
    let service_endpoint: SocketAddr = env_value("JAVA_SERVICE_SSU2_ENDPOINT")
        .parse()
        .expect("service endpoint");
    let bind: SocketAddr = env_value("I2PR_SSU2_BIND").parse().expect("bind");
    let reference_control_endpoint: SocketAddr = env_value("JAVA_STREAM_CONTROL_ENDPOINT")
        .parse()
        .expect("stream reference control endpoint");
    assert!(bind.ip().is_loopback(), "i2pr bind must be loopback");
    assert!(
        java_endpoint.ip().is_loopback(),
        "java endpoint must be loopback"
    );
    assert!(
        service_endpoint.ip().is_loopback(),
        "service endpoint must be loopback"
    );
    assert!(
        reference_control_endpoint.ip().is_loopback(),
        "reference control must be loopback"
    );
    let evidence_dir = env_path("EVIDENCE_DIR");
    std::fs::create_dir_all(&evidence_dir).expect("evidence dir");

    // Plan 217 §6.D — disjoint streaming namespace. These identifiers
    // MUST NOT collide with the destination driver's namespace
    // (file-level OUTBOUND_/OBEP_/IBGW_ and message_ids 0x51A7_5xxx /
    // 0x51A7_6xxx). Stock Java rejects duplicate build/tunnel-id
    // registrations when both drivers share the same Java RouterContext.
    const STREAM_OUTBOUND_CREATOR: u32 = 0x1700;
    const STREAM_INBOUND_CREATOR: u32 = 0x1800;
    const STREAM_OBEP_RECEIVE: u32 = 0x9701;
    const STREAM_OBEP_NEXT: u32 = 0x9702;
    const STREAM_IBGW_RECEIVE: u32 = 0x9801;
    const STREAM_IBGW_NEXT: u32 = 0x9802;
    const STREAM_MSG_OUTBOUND_BUILD: u32 = 0x51A7_7001;
    const STREAM_MSG_INBOUND_BUILD: u32 = 0x51A7_7101;
    const STREAM_MSG_LOOKUP: u32 = 0x51A7_7201;
    const STREAM_MSG_PUBLICATION: u32 = 0x51A7_7301;
    const STREAM_MSG_REPUBLICATION: u32 = 0x51A7_7401;
    const STREAM_MSG_ROUTERINFO_PUBLICATION: u32 = 0x51A7_7501;
    const STREAM_MSG_ROUTERINFO_SERVICE: u32 = 0x51A7_7502;

    let bind_port = bind.port();
    assert!(bind_port != 0, "lane requires a fixed loopback bind");
    let config_text = format!(
        "schema_version = 1\n[router]\ndata_dir = \"./state\"\n[ssu2]\nenabled = true\nbind_ipv4 = \"127.0.0.1\"\nport = {bind_port}\n"
    );
    let config = Config::parse(&config_text).expect("strict controlled profile");
    assert!(config.ssu2.enabled);
    assert!(!config.ssu2.advertise);
    append_evidence(&evidence_dir, "daemon-strict-profile", "true");

    let java_ri_bytes = std::fs::read(&java_ri_path).expect("read java router.info");
    let (java_hash, java_ssu2) =
        verify_reference_router_info(&java_ri_bytes).expect("verify java RouterInfo");
    let material = java_ssu2.address_material().expect("java key material");
    let responder_static =
        i2pr_runtime::Ssu2PublicKey::new(*material.static_public_key().as_bytes())
            .expect("static key");
    let responder_intro = i2pr_runtime::IntroKey::new(*material.intro_key().as_bytes());
    let target = daemon_dial_target(java_hash, java_endpoint, responder_static, responder_intro)
        .expect("dial target");
    let java_router_info = RouterInfo::decode(
        &java_ri_bytes,
        i2pr_runtime::constants::MAX_ESTABLISHMENT_ROUTER_INFO_BYTES,
    )
    .expect("decode java RouterInfo");
    let java_encryption_key: [u8; 32] = java_router_info
        .router_identity()
        .public_key()
        .as_bytes()
        .try_into()
        .expect("32-byte encryption key");
    let service_ri_bytes = std::fs::read(&service_ri_path).expect("read service router.info");
    let (service_hash, service_ssu2) =
        verify_reference_router_info(&service_ri_bytes).expect("verify service RouterInfo");
    let service_material = service_ssu2
        .address_material()
        .expect("service key material");
    let service_static =
        i2pr_runtime::Ssu2PublicKey::new(*service_material.static_public_key().as_bytes())
            .expect("service static key");
    let service_intro = i2pr_runtime::IntroKey::new(*service_material.intro_key().as_bytes());
    let service_target = daemon_dial_target(
        service_hash,
        service_endpoint,
        service_static,
        service_intro,
    )
    .expect("service dial target");
    let service_router_info = RouterInfo::decode(
        &service_ri_bytes,
        i2pr_runtime::constants::MAX_ESTABLISHMENT_ROUTER_INFO_BYTES,
    )
    .expect("decode service RouterInfo");
    append_evidence(&evidence_dir, "reference-routerinfo-verified", "true");
    append_evidence(&evidence_dir, "java-service-routerinfo-verified", "true");

    let mut dest = DestinationTunnelCoordinator::new(
        LookupPolicy::default(),
        RouterInfoStoreConfig::default(),
    );
    dest.advance_time(wall_ms());
    let now = Date::from_millis(wall_ms());
    let bootstrapped = dest
        .bootstrap_reference_router_info(&java_ri_bytes, now)
        .expect("bootstrap reference");
    assert_eq!(bootstrapped, RouterHash::from_bytes(*java_hash.as_bytes()));
    append_evidence(
        &evidence_dir,
        "reference-bootstrap-store",
        &dest.store_stats().record_count.to_string(),
    );
    let service_bootstrapped = dest
        .bootstrap_reference_router_info(&service_ri_bytes, now)
        .expect("bootstrap service reference");
    assert_eq!(
        service_bootstrapped,
        RouterHash::from_bytes(*service_hash.as_bytes())
    );
    let caps = java_router_info
        .capabilities()
        .ok()
        .flatten()
        .map(|c| c.as_str().to_owned())
        .unwrap_or_default();
    if !caps.bytes().any(|b| b == b'f') {
        record_stop(
            &evidence_dir,
            &format!("java reference not advertising floodfill (caps={caps:?})"),
        );
        assert!(
            caps.bytes().any(|b| b == b'f'),
            "java reference must advertise floodfill"
        );
    }
    append_evidence(&evidence_dir, "reference-floodfill-capable", "true");

    let bundle =
        i2pr_crypto::RouterIdentityBundle::generate(&mut rand_core::OsRng).expect("identity");
    let local_hash = bundle.identity().hash().expect("hash");
    let identity =
        generate_controlled_identity(&bundle, "127.0.0.1", bind_port).expect("controlled identity");

    let daemon_service = Ssu2DaemonService::new(&config.ssu2, identity).expect("daemon service");
    let token = CancellationToken::new();
    let scope = ChildScope::for_test(&token, ChildFailurePolicy::FailParent);
    let mut handle = daemon_service
        .start(&scope, &config.ssu2)
        .await
        .expect("daemon bind");
    assert_eq!(handle.local_v4().expect("bound v4"), bind);

    let _established = Box::pin(handle.dial(target, DIAL_TIMEOUT, &CancellationToken::new()))
        .await
        .expect("authenticated session establishes");
    let _service_established =
        Box::pin(handle.dial(service_target, DIAL_TIMEOUT, &CancellationToken::new()))
            .await
            .expect("authenticated session establishes with Java service router");
    let deadline_active = tokio::time::Instant::now() + WAIT_TIMEOUT;
    while tokio::time::Instant::now() < deadline_active {
        if handle.snapshot().active_sessions >= 2 {
            break;
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
    assert!(handle.snapshot().active_sessions >= 2);
    append_evidence(
        &evidence_dir,
        "session-established",
        &handle.snapshot().sessions_established.to_string(),
    );

    let publication_peer = PeerId::from_hash(java_hash);
    let service_peer = PeerId::from_hash(service_hash);
    let publication_wire = database_store_router_info_wire(
        Hash::from_bytes(*service_hash.as_bytes()),
        &service_ri_bytes,
        STREAM_MSG_ROUTERINFO_PUBLICATION,
    );
    let service_wire = database_store_router_info_wire(
        Hash::from_bytes(*java_hash.as_bytes()),
        &java_ri_bytes,
        STREAM_MSG_ROUTERINFO_SERVICE,
    );
    for (peer, wire) in [
        (publication_peer, publication_wire),
        (service_peer, service_wire),
    ] {
        let request = RouterDeliveryRequest::new(peer, wire, DELIVERY_TIMEOUT)
            .expect("RouterInfo bootstrap request");
        assert_eq!(
            handle
                .delivery()
                .deliver(request, &CancellationToken::new()),
            RouterDeliveryOutcome::Accepted,
            "ordinary Java RouterInfo bootstrap must be admitted"
        );
    }
    append_evidence(
        &evidence_dir,
        "java-router-peer-bootstrap-submitted",
        "service-to-publication-and-publication-to-service",
    );

    let warmup_deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    while tokio::time::Instant::now() < warmup_deadline {
        if tokio::time::timeout(POLL_INTERVAL, handle.next_inbound())
            .await
            .is_err()
        {
            continue;
        }
    }

    // Counted reference service: public I2PSocketManager/I2PServerSocket.
    // The SAM STREAM result remains diagnostic-only and is not used here.
    let mut stream_control = ReferenceControl::connect(reference_control_endpoint).await;
    assert_eq!(stream_control.command("PING").await, "PONG");
    // Plan 200 §A.2 — explicit bounded helper status command.
    let status_line = stream_control.command("REPORT_STATUS").await;
    assert!(
        status_line.starts_with("STATUS "),
        "REPORT_STATUS must return a STATUS line, got: {status_line:?}"
    );
    let reference_b64 = env_value("JAVA_STREAM_REFERENCE_DESTINATION_B64");
    let reference_bytes = decode_destination_b64(&reference_b64);
    let reference_hash =
        i2pr_netdb::DestinationHash::from_hash(i2pr_crypto::sha256(&reference_bytes));
    append_evidence(
        &evidence_dir,
        "public-streaming-session-established",
        &format!("control_pong=true dest_len={}", reference_bytes.len()),
    );
    append_evidence(
        &evidence_dir,
        "public-streaming-destination-created",
        &format!(
            "dest_len={} session=connected control_ready=1 publication_observed=external",
            reference_bytes.len()
        ),
    );
    append_evidence(
        &evidence_dir,
        "public-streaming-leaseset-status",
        &status_line,
    );

    let mut coord = ExploratoryBuildCoordinator::new(ExploratoryPoolConfig::balanced());
    coord.advance_time(wall_ms());
    let bridge = ShortBuildI2npBridge::new();
    let mut rng = ChaCha8Rng::seed_from_u64(wall_secs());
    let delivery = handle.delivery().clone();

    let outbound_request = BuildRequest {
        direction: BuildDirection::Outbound,
        peer: PeerBuildMaterial {
            router_hash: java_hash,
            static_encryption_key: java_encryption_key,
            receive_tunnel: TunnelId::new(STREAM_OBEP_RECEIVE).expect("receive"),
            next_tunnel: TunnelId::new(STREAM_OBEP_NEXT).expect("next"),
            role: HopRole::OutboundEndpoint,
        },
        creator_tunnel_id: TunnelId::new(STREAM_OUTBOUND_CREATOR).expect("creator"),
        message_id: STREAM_MSG_OUTBOUND_BUILD,
        outbound_reply_router: Some(local_hash),
        originator_hash: None,
    };
    coord
        .submit(
            outbound_request,
            &delivery,
            &bridge,
            BridgeHeader::ShortTransport {
                message_id: STREAM_MSG_OUTBOUND_BUILD,
                expiration_seconds: wall_secs().saturating_add(60) as u32,
            },
            &mut rng,
        )
        .expect("outbound submit ok");
    append_evidence(&evidence_dir, "outbound-build-emitted", "true");

    let inbound_request = BuildRequest {
        direction: BuildDirection::Inbound,
        peer: PeerBuildMaterial {
            router_hash: service_hash,
            static_encryption_key: service_router_info
                .router_identity()
                .public_key()
                .as_bytes()
                .try_into()
                .expect("service encryption key"),
            receive_tunnel: TunnelId::new(STREAM_IBGW_RECEIVE).expect("receive"),
            next_tunnel: TunnelId::new(STREAM_IBGW_NEXT).expect("next"),
            role: HopRole::InboundGateway,
        },
        creator_tunnel_id: TunnelId::new(STREAM_INBOUND_CREATOR).expect("creator"),
        message_id: STREAM_MSG_INBOUND_BUILD,
        outbound_reply_router: None,
        originator_hash: Some(local_hash),
    };
    coord
        .submit(
            inbound_request,
            &delivery,
            &bridge,
            BridgeHeader::ShortTransport {
                message_id: STREAM_MSG_INBOUND_BUILD,
                expiration_seconds: wall_secs().saturating_add(60) as u32,
            },
            &mut rng,
        )
        .expect("inbound submit ok");
    append_evidence(&evidence_dir, "inbound-build-emitted", "true");

    let mut outbound_slot: Option<i2pr_tunnel::pool::TunnelSlot> = None;
    let mut installed_inbound = false;
    let mut pump_installed_ob = 0u64;
    let mut pump_installed_ib = 0u64;
    let mut pump_kind_reply = 0u64;
    let install_deadline = tokio::time::Instant::now() + ACCEPT_TIMEOUT;
    while tokio::time::Instant::now() < install_deadline
        && (outbound_slot.is_none() || !installed_inbound)
    {
        let next = tokio::time::timeout(POLL_INTERVAL, handle.next_inbound()).await;
        let Ok(Some(inbound)) = next else {
            continue;
        };
        let routed = match coord.route_inbound_i2np(&inbound, wall_ms()) {
            Ok(routed) => routed,
            Err(_) => continue,
        };
        if let i2pr_daemon::router_i2np::RouterI2npOutcome::TunnelBuildReserved { kind, .. } =
            &routed.dispatcher
            && matches!(
                kind,
                i2pr_daemon::router_i2np::RouterI2npKind::OutboundTunnelBuildReply
            )
        {
            pump_kind_reply += 1;
        }
        for outcome in routed.coordinator {
            if let BuildCoordinatorOutcome::Installed {
                slot, direction, ..
            } = outcome
            {
                match direction {
                    BuildDirection::Outbound => {
                        outbound_slot = Some(slot);
                        pump_installed_ob += 1;
                    }
                    BuildDirection::Inbound => {
                        installed_inbound = true;
                        pump_installed_ib += 1;
                    }
                }
            }
        }
    }
    let outbound_slot = match outbound_slot {
        Some(slot) => slot,
        None => {
            record_stop(
                &evidence_dir,
                &format!(
                    "Plan 194 §11 stop: java outbound build never installed (installed_ob={pump_installed_ob} kind_reply={pump_kind_reply})"
                ),
            );
            handle.shutdown();
            let _ = scope.shutdown().await;
            return;
        }
    };
    if !installed_inbound {
        record_stop(
            &evidence_dir,
            &format!(
                "Plan 194 §11 stop: java inbound build never installed (installed_ib={pump_installed_ib})"
            ),
        );
        handle.shutdown();
        let _ = scope.shutdown().await;
        return;
    }
    append_evidence(&evidence_dir, "outbound-installed", "true");
    append_evidence(&evidence_dir, "inbound-installed", "true");

    let gateway_role = coord
        .registry_mut()
        .remove_outbound(outbound_slot)
        .expect("real outbound role");
    let destination_outbound =
        DestinationOutboundRole::from_role(gateway_role, wall_ms() + 1_800_000);

    // Lease lookup over the real tunnel path.
    let routing_key = i2pr_netdb::router_hash_from_destination(reference_hash);
    let receive_ids = coord.registry().inbound_receive_ids();
    assert_eq!(receive_ids.len(), 1);
    let local_receive_for_lookup = receive_ids[0];
    let reply_path = reply_path_for_inbound_route(coord.registry(), local_receive_for_lookup)
        .expect("typed inbound gateway route");
    let (lookup_id, action) = dest
        .begin_lease_lookup(reference_hash, &routing_key, reply_path)
        .expect("lease lookup send");
    let mut tunnel_rng = ChaCha8Rng::seed_from_u64(wall_secs().wrapping_add(7));
    let (dispatch, proof) = dest
        .compose_lookup_via_tunnel(
            &action,
            destination_outbound.role(),
            STREAM_MSG_LOOKUP,
            wall_ms() + 60_000,
            Deadline::new(Duration::from_secs(60)).expect("deadline"),
            &mut tunnel_rng,
            0,
        )
        .expect("compose lease lookup");
    assert!(proof.via_tunnel);
    for cell_delivery in &dispatch.deliveries {
        let request = RouterDeliveryRequest::new(
            cell_delivery.target(),
            cell_delivery.message_bytes().to_vec(),
            DELIVERY_TIMEOUT,
        )
        .expect("delivery request");
        assert_eq!(
            delivery.deliver(request, &CancellationToken::new()),
            RouterDeliveryOutcome::Accepted
        );
    }

    let lookup_deadline = tokio::time::Instant::now() + STREAM_WAIT;
    let mut lease_summary: Option<i2pr_daemon::destination_tunnels::RemoteLeaseSummary> = None;
    while tokio::time::Instant::now() < lookup_deadline && lease_summary.is_none() {
        let next = tokio::time::timeout(POLL_INTERVAL, handle.next_inbound()).await;
        let Ok(Some(inbound)) = next else {
            continue;
        };
        let message = match decode_inbound_i2np(&inbound.bytes) {
            Ok(message) => message,
            Err(_) => continue,
        };
        let cell = match message.body() {
            I2npBody::TunnelData(cell) => cell.clone(),
            _ => continue,
        };
        let outcome = match dispatch_inbound_tunnel_data(coord.registry_mut(), &cell, wall_ms()) {
            Ok(outcome) => outcome,
            Err(_) => continue,
        };
        let bytes = match outcome {
            i2pr_daemon::inbound_dispatch::InboundDispatchOutcome::DatabaseStoreComplete {
                bytes,
            } => bytes,
            _ => continue,
        };
        let envelope =
            I2npMessage::decode_standard(&bytes, MAX_I2NP_PAYLOAD_SIZE).expect("decode store");
        let now_secs = u32::try_from(wall_secs()).unwrap_or(u32::MAX);
        match dest
            .ingest_tunnel_lease_store(lookup_id, &envelope, now_secs)
            .expect("ingest lease store")
        {
            LeaseStoreIngestOutcome::Completed { summary, .. } => {
                lease_summary = Some(summary);
            }
            LeaseStoreIngestOutcome::Continue | LeaseStoreIngestOutcome::Ignored => {}
        }
    }
    let summary = match lease_summary {
        Some(summary) => summary,
        None => {
            record_stop(
                &evidence_dir,
                "phase=lease-lookup reference LeaseSet2 never resolved",
            );
            handle.shutdown();
            let _ = scope.shutdown().await;
            return;
        }
    };
    assert_eq!(summary.destination, reference_hash);
    append_evidence(
        &evidence_dir,
        "lease-lookup-completed",
        &format!("leases={}", summary.lease_count),
    );

    let mut identity_rng = OsRng;
    let local_identity =
        DestinationIdentity::generate(&mut identity_rng).expect("destination identity");
    let registrations_in = coord.registrations(TunnelDirection::Inbound);
    let tunnel_expires = wall_secs() + 600;
    let lease_source = InboundLeaseSource::from_parts(
        registrations_in[0].slot(),
        Hash::from_bytes(*java_hash.as_bytes()),
        STREAM_IBGW_RECEIVE,
        tunnel_expires,
        tunnel_expires.saturating_sub(60),
    );
    let published = u32::try_from(wall_secs()).unwrap_or(u32::MAX);
    let mut local_ls2 =
        build_signed_lease_set2(&local_identity, &[lease_source], published).expect("local ls2");
    let local_dest_hash = local_identity.id().as_netdb_key();

    let store_message = i2pr_proto::DatabaseStoreMessage {
        key: local_ls2.key_hash().expect("key hash"),
        reply_token: 0,
        reply_tunnel_id: None,
        reply_gateway: None,
        data: i2pr_proto::DatabaseStoreData::LeaseSet2(Box::new(local_ls2.clone())),
    };
    let publication_id = dest
        .begin_ls2_publication(store_message, RouterHash::from_bytes(*java_hash.as_bytes()))
        .expect("begin publication");
    let (pub_dispatch, pub_proof) = dest
        .compose_ls2_publication_via_tunnel(
            publication_id,
            Hash::from_bytes(*java_hash.as_bytes()),
            destination_outbound.role(),
            STREAM_MSG_PUBLICATION,
            wall_ms() + 60_000,
            Deadline::new(Duration::from_secs(60)).expect("deadline"),
            &mut tunnel_rng,
            0,
        )
        .expect("compose publication");
    assert!(pub_proof.via_tunnel);
    for cell_delivery in &pub_dispatch.deliveries {
        let request = RouterDeliveryRequest::new(
            cell_delivery.target(),
            cell_delivery.message_bytes().to_vec(),
            DELIVERY_TIMEOUT,
        )
        .expect("delivery request");
        assert_eq!(
            delivery.deliver(request, &CancellationToken::new()),
            RouterDeliveryOutcome::Accepted
        );
    }
    append_evidence(
        &evidence_dir,
        "ls2-publication-tunnel",
        &format!("cells={}", pub_proof.cell_count),
    );

    // Direction A: i2pr StreamingManager.connect -> Java STREAM.
    let mut routing = DestinationRouting::new(DestinationRoutingConfig::balanced());
    let cached = dest
        .lease_store()
        .get(&reference_hash)
        .expect("cached reference ls2")
        .clone();
    let remote = routing
        .install_remote_lease_set2(cached)
        .expect("install remote ls2");
    assert_eq!(remote, reference_hash);
    let mut session = EciesSessionManager::new(EciesSessionConfig::balanced());

    let reference_destination =
        i2pr_proto::Destination::decode(&reference_bytes, 65535).expect("decode reference dest");
    let mut static_key = [0u8; 32];
    let enc_bytes = reference_destination.public_key().as_bytes();
    if enc_bytes.len() == 32 {
        static_key.copy_from_slice(enc_bytes);
    }
    let remote_desc = RemoteDestination {
        destination_hash: *reference_hash.as_bytes(),
        signing_public_key: reference_destination.signing_key().clone(),
        static_public_key: static_key,
    };

    let mut streaming = StreamingManager::new(StreamingConfig::balanced());
    assert_eq!(stream_control.command("START_ACCEPT").await, "STARTED");
    let outcome = streaming
        .connect(
            &local_identity,
            &remote_desc,
            LOCAL_STREAM_PORT,
            REMOTE_STREAM_PORT,
            DEFAULT_ADVERTISED_MAX_PAYLOAD,
            wall_ms(),
            &mut ChaCha8Rng::seed_from_u64(wall_secs()),
        )
        .expect("connect SYN");
    let ConnectOutcome::SynSent { connection_id, .. } = outcome else {
        panic!("expected SynSent");
    };
    let mut syn_queue = streaming.drain_outbound();
    assert_eq!(syn_queue.len(), 1, "connect must emit exactly one SYN");
    let syn_request = syn_queue.remove(0);
    let mut send_rng = ChaCha8Rng::seed_from_u64(wall_ms().wrapping_add(11));
    send_transport_request(
        &syn_request,
        &routing,
        &mut session,
        &destination_outbound,
        &local_identity,
        &local_ls2,
        &delivery,
        &mut send_rng,
    )
    .await;
    append_evidence(&evidence_dir, "streaming-syn-sent", "true");

    let mut dispatcher = DestinationDispatcher::new();
    dispatcher
        .register_destination(local_identity.id())
        .expect("register destination");
    dispatcher
        .bind_destination_hash(local_identity.id(), local_dest_hash)
        .expect("bind destination hash");
    let syn_ack_deadline = tokio::time::Instant::now() + SYN_ACK_WAIT;
    let mut syn_accepted = false;
    let mut established = false;
    let mut pump_error = 0u64;
    while tokio::time::Instant::now() < syn_ack_deadline && !established {
        let next = tokio::time::timeout(POLL_INTERVAL, handle.next_inbound()).await;
        let Ok(Some(inbound)) = next else {
            continue;
        };
        let message = match decode_inbound_i2np(&inbound.bytes) {
            Ok(message) => message,
            Err(_) => {
                pump_error += 1;
                continue;
            }
        };
        let cell = match message.body() {
            I2npBody::TunnelData(cell) => cell.clone(),
            _ => continue,
        };
        let bytes = match dest.recover_garlic_bytes(coord.registry_mut(), &cell, wall_ms()) {
            Ok(bytes) => bytes,
            Err(_) => continue,
        };
        let now_secs = u32::try_from(wall_secs()).unwrap_or(u32::MAX);
        dispatcher.dispatch_garlic_envelope(
            &mut session,
            local_identity.id(),
            local_identity.static_secret_bytes(),
            &local_identity.static_public_bytes(),
            now_secs,
            &I2npMessage::decode_standard(&bytes, MAX_I2NP_PAYLOAD_SIZE).expect("decode garlic"),
            routing.lease_set2_store_mut(),
        );
        let Some(queued) = dispatcher.pop_payload(local_identity.id()) else {
            continue;
        };
        match StreamingDestinationAdapter::receive(
            queued.bytes(),
            &local_identity,
            &mut streaming,
            reference_hash.as_bytes(),
            wall_ms(),
        ) {
            Ok(i2pr_client::InboundStreamingOutcome::StreamingDispatched { .. }) => {
                syn_accepted = true;
            }
            Ok(_) => {}
            Err(_) => {
                pump_error += 1;
                continue;
            }
        }
        if let Some(conn) = streaming.get_connection(connection_id)
            && conn.state() == i2pr_client::streaming::connection::ConnectionState::Established
        {
            established = true;
        }
        let pending = streaming.drain_outbound();
        for request in &pending {
            send_transport_request(
                request,
                &routing,
                &mut session,
                &destination_outbound,
                &local_identity,
                &local_ls2,
                &delivery,
                &mut send_rng,
            )
            .await;
        }
    }
    if !syn_accepted || !established {
        record_stop(
            &evidence_dir,
            &format!(
                "Plan 194 §11 stop: SYN-ACK never established (syn_accepted={syn_accepted} established={established} pump_error={pump_error})"
            ),
        );
        handle.shutdown();
        let _ = scope.shutdown().await;
        return;
    }
    append_evidence(&evidence_dir, "streaming-syn-accepted", "true");
    append_evidence(&evidence_dir, "streaming-established", "true");

    let accept_id = loop {
        let response = stream_control.command("ACCEPT_STATUS 0").await;
        if let Some(id) = response.strip_prefix("ACCEPTED ") {
            break id.parse::<usize>().expect("accepted socket id");
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    };

    // Direction A small payload.
    let app_small = b"plan194-streaming-probe-a";
    let data_request = streaming
        .send_data(
            connection_id,
            &local_identity,
            &remote_desc,
            LOCAL_STREAM_PORT,
            REMOTE_STREAM_PORT,
            app_small,
            wall_ms(),
        )
        .expect("send small data");
    send_transport_request(
        &data_request,
        &routing,
        &mut session,
        &destination_outbound,
        &local_identity,
        &local_ls2,
        &delivery,
        &mut send_rng,
    )
    .await;

    let peer_len = 0;
    let observed = stream_control
        .read_stream(accept_id, app_small.len())
        .await
        .unwrap_or_default();
    if observed == app_small {
        append_evidence(
            &evidence_dir,
            "streaming-data-digest",
            &format!(
                "payload_len={} digest={} peer_line_len={peer_len} match=true",
                observed.len(),
                sha256_hex(&observed),
            ),
        );
    } else {
        record_stop(
            &evidence_dir,
            &format!(
                "phase=streaming-data expected_len={} observed_len={} peer_line_len={peer_len}",
                app_small.len(),
                observed.len(),
            ),
        );
        handle.shutdown();
        let _ = scope.shutdown().await;
        return;
    }

    // Direction A multi-packet.
    let mut app_multi = Vec::with_capacity(8192);
    for index in 0..8192 {
        app_multi.push((index % 251) as u8);
    }
    let mut offset = 0;
    let mut fragments = 0u64;
    while offset < app_multi.len() {
        let end = (offset + 1024).min(app_multi.len());
        let fragment_request = streaming
            .send_data(
                connection_id,
                &local_identity,
                &remote_desc,
                LOCAL_STREAM_PORT,
                REMOTE_STREAM_PORT,
                &app_multi[offset..end],
                wall_ms(),
            )
            .expect("send multi fragment");
        send_transport_request(
            &fragment_request,
            &routing,
            &mut session,
            &destination_outbound,
            &local_identity,
            &local_ls2,
            &delivery,
            &mut send_rng,
        )
        .await;
        offset = end;
        fragments += 1;
    }
    let observed_multi = stream_control
        .read_stream(accept_id, app_multi.len())
        .await
        .unwrap_or_default();
    if observed_multi == app_multi {
        append_evidence(
            &evidence_dir,
            "streaming-multipacket-digest",
            &format!(
                "payload_len={} fragments={fragments} digest={} match=true",
                observed_multi.len(),
                sha256_hex(&observed_multi),
            ),
        );
    } else {
        record_stop(
            &evidence_dir,
            &format!(
                "phase=streaming-multipacket expected_len={} observed_len={} fragments={fragments}",
                app_multi.len(),
                observed_multi.len(),
            ),
        );
        handle.shutdown();
        let _ = scope.shutdown().await;
        return;
    }

    // Direction A reverse (Java -> i2pr).
    let app_rev_small = b"plan194-reverse-probe-b";
    assert!(stream_control.write_stream(accept_id, app_rev_small).await);
    let mut rev_collected = Vec::new();
    pump_until_streaming(
        &mut handle,
        &mut coord,
        &mut dest,
        &mut dispatcher,
        &mut session,
        &mut routing,
        &mut streaming,
        &destination_outbound,
        &local_identity,
        &local_ls2,
        &delivery,
        &mut send_rng,
        reference_hash.as_bytes(),
        &evidence_dir,
        tokio::time::Instant::now() + STREAM_WAIT,
        "streaming-reverse",
        |manager| {
            for delivered in manager.drain_delivered_for(connection_id) {
                rev_collected.extend_from_slice(&delivered.bytes);
            }
            rev_collected.as_slice() == app_rev_small.as_slice()
        },
    )
    .await;
    if rev_collected == app_rev_small {
        append_evidence(
            &evidence_dir,
            "streaming-reverse-data-digest",
            &format!(
                "payload_len={} digest={} match=true",
                rev_collected.len(),
                sha256_hex(&rev_collected),
            ),
        );
    } else {
        record_stop(&evidence_dir, "phase=streaming-reverse digest mismatch");
        handle.shutdown();
        let _ = scope.shutdown().await;
        return;
    }

    let mut app_rev_multi = Vec::with_capacity(4096);
    for index in 0..4096 {
        app_rev_multi.push((index % 233) as u8);
    }
    assert!(stream_control.write_stream(accept_id, &app_rev_multi).await);
    let mut rev_multi_collected = Vec::new();
    pump_until_streaming_with_state(
        &mut handle,
        &mut coord,
        &mut dest,
        &mut dispatcher,
        &mut session,
        &mut routing,
        &mut streaming,
        &destination_outbound,
        &local_identity,
        &local_ls2,
        &delivery,
        &mut send_rng,
        reference_hash.as_bytes(),
        &evidence_dir,
        tokio::time::Instant::now() + STREAM_WAIT,
        "streaming-reverse-multipacket",
        Some(connection_id),
        |manager| {
            for delivered in manager.drain_delivered_for(connection_id) {
                rev_multi_collected.extend_from_slice(&delivered.bytes);
            }
            rev_multi_collected.len() >= app_rev_multi.len()
        },
    )
    .await;
    if rev_multi_collected == app_rev_multi {
        append_evidence(
            &evidence_dir,
            "streaming-reverse-multipacket-digest",
            &format!(
                "payload_len={} digest={} match=true",
                rev_multi_collected.len(),
                sha256_hex(&rev_multi_collected),
            ),
        );
    } else {
        record_stop(
            &evidence_dir,
            &format!(
                "phase=streaming-reverse-multipacket expected_len={} observed_len={}",
                app_rev_multi.len(),
                rev_multi_collected.len(),
            ),
        );
        handle.shutdown();
        let _ = scope.shutdown().await;
        return;
    }

    // Sibling + close + isolation. Direction A only on the
    // outbound side; Direction B reverses through the normal
    // listener/accept path (Direction B below).
    assert_eq!(stream_control.command("START_ACCEPT").await, "STARTED");
    let sibling_outcome = streaming
        .connect(
            &local_identity,
            &remote_desc,
            LOCAL_STREAM_PORT,
            REMOTE_STREAM_PORT,
            DEFAULT_ADVERTISED_MAX_PAYLOAD,
            wall_ms(),
            &mut send_rng,
        )
        .expect("sibling connect");
    let ConnectOutcome::SynSent {
        connection_id: sibling_id,
        ..
    } = sibling_outcome
    else {
        panic!("expected sibling SynSent");
    };
    assert_ne!(sibling_id, connection_id, "siblings need distinct ids");
    let mut sibling_queue = streaming.drain_outbound();
    assert_eq!(sibling_queue.len(), 1, "sibling SYN is one request");
    send_transport_request(
        &sibling_queue.remove(0),
        &routing,
        &mut session,
        &destination_outbound,
        &local_identity,
        &local_ls2,
        &delivery,
        &mut send_rng,
    )
    .await;
    pump_until_streaming(
        &mut handle,
        &mut coord,
        &mut dest,
        &mut dispatcher,
        &mut session,
        &mut routing,
        &mut streaming,
        &destination_outbound,
        &local_identity,
        &local_ls2,
        &delivery,
        &mut send_rng,
        reference_hash.as_bytes(),
        &evidence_dir,
        tokio::time::Instant::now() + SYN_ACK_WAIT,
        "streaming-sibling-establish",
        |manager| {
            manager
                .get_connection(sibling_id)
                .is_some_and(|conn| conn.state() == ConnectionState::Established)
        },
    )
    .await;
    if streaming
        .get_connection(sibling_id)
        .is_some_and(|conn| conn.state() == ConnectionState::Established)
    {
        append_evidence(&evidence_dir, "streaming-sibling-established", "true");
    } else {
        record_stop(&evidence_dir, "phase=streaming-sibling-establish");
        handle.shutdown();
        let _ = scope.shutdown().await;
        return;
    }

    let app_sibling = b"plan194-sibling-probe-c";
    let sibling_data = streaming
        .send_data(
            sibling_id,
            &local_identity,
            &remote_desc,
            LOCAL_STREAM_PORT,
            REMOTE_STREAM_PORT,
            app_sibling,
            wall_ms(),
        )
        .expect("send sibling data");
    send_transport_request(
        &sibling_data,
        &routing,
        &mut session,
        &destination_outbound,
        &local_identity,
        &local_ls2,
        &delivery,
        &mut send_rng,
    )
    .await;
    let sibling_socket_id = loop {
        let response = stream_control.command("ACCEPT_STATUS 1").await;
        if let Some(id) = response.strip_prefix("ACCEPTED ") {
            break id.parse::<usize>().expect("sibling socket id");
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    };
    let peer2_len = 0;
    let observed_sibling = stream_control
        .read_stream(sibling_socket_id, app_sibling.len())
        .await
        .unwrap_or_default();
    if observed_sibling.as_slice() != app_sibling.as_slice() {
        record_stop(
            &evidence_dir,
            &format!(
                "phase=streaming-sibling-data expected_len={} observed_len={} peer_line_len={}",
                app_sibling.len(),
                observed_sibling.len(),
                peer2_len,
            ),
        );
        handle.shutdown();
        let _ = scope.shutdown().await;
        return;
    }
    append_evidence(
        &evidence_dir,
        "streaming-sibling-data-digest",
        &format!(
            "payload_len={} digest={} peer_line_len={} match=true",
            observed_sibling.len(),
            sha256_hex(&observed_sibling),
            peer2_len,
        ),
    );

    let close_request = streaming
        .send_close(
            connection_id,
            &local_identity,
            &remote_desc,
            LOCAL_STREAM_PORT,
            REMOTE_STREAM_PORT,
            wall_ms(),
        )
        .expect("send close");
    send_transport_request(
        &close_request,
        &routing,
        &mut session,
        &destination_outbound,
        &local_identity,
        &local_ls2,
        &delivery,
        &mut send_rng,
    )
    .await;
    pump_until_streaming(
        &mut handle,
        &mut coord,
        &mut dest,
        &mut dispatcher,
        &mut session,
        &mut routing,
        &mut streaming,
        &destination_outbound,
        &local_identity,
        &local_ls2,
        &delivery,
        &mut send_rng,
        reference_hash.as_bytes(),
        &evidence_dir,
        tokio::time::Instant::now() + Duration::from_secs(30),
        "streaming-close",
        |manager| {
            manager
                .get_connection(connection_id)
                .is_some_and(|conn| conn.state() == ConnectionState::Closed)
        },
    )
    .await;
    let close_eof = stream_control.stream_eof(accept_id).await;
    if !close_eof {
        record_stop(
            &evidence_dir,
            "phase=streaming-close local state Closed but reference socket never reached EOF",
        );
        handle.shutdown();
        let _ = scope.shutdown().await;
        return;
    }
    append_evidence(&evidence_dir, "streaming-close", "state=Closed eof=true");

    let app_sibling2 = b"plan194-sibling-alive-d";
    let sibling_data2 = streaming
        .send_data(
            sibling_id,
            &local_identity,
            &remote_desc,
            LOCAL_STREAM_PORT,
            REMOTE_STREAM_PORT,
            app_sibling2,
            wall_ms(),
        )
        .expect("send sibling data after close");
    send_transport_request(
        &sibling_data2,
        &routing,
        &mut session,
        &destination_outbound,
        &local_identity,
        &local_ls2,
        &delivery,
        &mut send_rng,
    )
    .await;
    let observed_sibling2 = stream_control
        .read_stream(sibling_socket_id, app_sibling2.len())
        .await
        .unwrap_or_default();
    if observed_sibling2.as_slice() != app_sibling2.as_slice() {
        record_stop(
            &evidence_dir,
            &format!(
                "phase=streaming-sibling-isolated expected_len={} observed_len={}",
                app_sibling2.len(),
                observed_sibling2.len(),
            ),
        );
        handle.shutdown();
        let _ = scope.shutdown().await;
        return;
    }
    append_evidence(
        &evidence_dir,
        "streaming-sibling-isolated",
        &format!(
            "payload_len={} digest={} match=true",
            observed_sibling2.len(),
            sha256_hex(&observed_sibling2),
        ),
    );

    // Direction B: Java STREAM CONNECT -> i2pr listener/accept.
    let fresh_expires = wall_secs() + 1800;
    let fresh_published = u32::try_from(wall_secs()).unwrap_or(u32::MAX);
    let fresh_registrations = coord.registrations(TunnelDirection::Inbound);
    let fresh_lease = InboundLeaseSource::from_parts(
        fresh_registrations[0].slot(),
        Hash::from_bytes(*java_hash.as_bytes()),
        STREAM_IBGW_RECEIVE,
        fresh_expires,
        fresh_expires.saturating_sub(60),
    );
    local_ls2 = build_signed_lease_set2(&local_identity, &[fresh_lease], fresh_published)
        .expect("fresh ls2");
    let fresh_store = i2pr_proto::DatabaseStoreMessage {
        key: local_ls2.key_hash().expect("key hash"),
        reply_token: 0,
        reply_tunnel_id: None,
        reply_gateway: None,
        data: i2pr_proto::DatabaseStoreData::LeaseSet2(Box::new(local_ls2.clone())),
    };
    let fresh_publication = dest
        .begin_ls2_publication(fresh_store, RouterHash::from_bytes(*java_hash.as_bytes()))
        .expect("begin republication");
    let (fresh_dispatch, fresh_proof) = dest
        .compose_ls2_publication_via_tunnel(
            fresh_publication,
            Hash::from_bytes(*java_hash.as_bytes()),
            destination_outbound.role(),
            STREAM_MSG_REPUBLICATION,
            wall_ms() + 60_000,
            Deadline::new(Duration::from_secs(60)).expect("deadline"),
            &mut tunnel_rng,
            0,
        )
        .expect("compose republication");
    assert!(fresh_proof.via_tunnel);
    for cell_delivery in &fresh_dispatch.deliveries {
        let request = RouterDeliveryRequest::new(
            cell_delivery.target(),
            cell_delivery.message_bytes().to_vec(),
            DELIVERY_TIMEOUT,
        )
        .expect("delivery request");
        assert_eq!(
            delivery.deliver(request, &CancellationToken::new()),
            RouterDeliveryOutcome::Accepted
        );
    }
    append_evidence(
        &evidence_dir,
        "ls2-republication-tunnel",
        &format!("cells={}", fresh_proof.cell_count),
    );
    tokio::time::sleep(Duration::from_secs(5)).await;

    assert!(
        matches!(
            streaming.listen(0).expect("listen"),
            ListenerOutcome::Listening { .. }
        ),
        "wildcard listener must bind"
    );
    let local_b64 = i2pr_api::sam::base64::encode(
        &local_identity
            .destination()
            .encode_to_vec(65535)
            .expect("encode local destination"),
    );
    let mut connect_attempts = 0u32;
    let mut b_connection: Option<i2pr_client::streaming::connection::ConnectionId> = None;
    assert_eq!(
        stream_control
            .command(&format!("START_CONNECT {local_b64}"))
            .await,
        "STARTED"
    );
    let mut b_last_result = "waiting".to_owned();
    for _ in 0..2 {
        connect_attempts += 1;
        let connect_deadline = tokio::time::Instant::now() + Duration::from_secs(120);
        let mut status: Option<String> = None;
        let mut pump_errors = 0u64;
        while tokio::time::Instant::now() < connect_deadline && status.is_none() {
            pump_one_streaming_inbound(
                &mut handle,
                &mut coord,
                &mut dest,
                &mut dispatcher,
                &mut session,
                &mut routing,
                &mut streaming,
                &destination_outbound,
                &local_identity,
                &local_ls2,
                &delivery,
                &mut send_rng,
                reference_hash.as_bytes(),
                &mut pump_errors,
                &evidence_dir,
            )
            .await;
            drain_streaming_timers(
                &mut streaming,
                &mut routing,
                &mut session,
                &destination_outbound,
                &local_identity,
                &local_ls2,
                &delivery,
                &mut send_rng,
                &evidence_dir,
            )
            .await;
            if b_connection.is_none() {
                b_connection = accept_inbound_syn_response(
                    &mut streaming,
                    &local_identity,
                    &routing,
                    &mut session,
                    &destination_outbound,
                    &local_ls2,
                    &delivery,
                    &mut send_rng,
                )
                .await;
            }
            let line = stream_control.command("CONNECT_STATUS 0").await;
            if line.starts_with("CONNECTED ") {
                b_last_result = "OK".to_owned();
                status = Some(line);
            }
        }
        match &status {
            Some(line) if line.contains("RESULT=OK") => break,
            _ => {
                append_evidence(
                    &evidence_dir,
                    "streaming-b-connect-debug",
                    &format!(
                        "attempt={connect_attempts} syn_arrived={} backlog={} result={b_last_result} pump_errors={pump_errors}",
                        b_connection.is_some(),
                        streaming.listener_backlog(0),
                    ),
                );
                if connect_attempts >= 2 {
                    record_stop(
                        &evidence_dir,
                        &format!(
                            "phase=streaming-b-connect attempts={connect_attempts} syn_arrived={} result={b_last_result} pump_errors={pump_errors}",
                            b_connection.is_some(),
                        ),
                    );
                    handle.shutdown();
                    let _ = scope.shutdown().await;
                    return;
                }
            }
        }
    }
    if b_connection.is_none() {
        let accept_deadline = tokio::time::Instant::now() + Duration::from_secs(60);
        let mut accept_errors = 0u64;
        while tokio::time::Instant::now() < accept_deadline && b_connection.is_none() {
            pump_one_streaming_inbound(
                &mut handle,
                &mut coord,
                &mut dest,
                &mut dispatcher,
                &mut session,
                &mut routing,
                &mut streaming,
                &destination_outbound,
                &local_identity,
                &local_ls2,
                &delivery,
                &mut send_rng,
                reference_hash.as_bytes(),
                &mut accept_errors,
                &evidence_dir,
            )
            .await;
            drain_streaming_timers(
                &mut streaming,
                &mut routing,
                &mut session,
                &destination_outbound,
                &local_identity,
                &local_ls2,
                &delivery,
                &mut send_rng,
                &evidence_dir,
            )
            .await;
            b_connection = accept_inbound_syn_response(
                &mut streaming,
                &local_identity,
                &routing,
                &mut session,
                &destination_outbound,
                &local_ls2,
                &delivery,
                &mut send_rng,
            )
            .await;
        }
    }
    let b_id = match b_connection {
        Some(id) => id,
        None => {
            record_stop(
                &evidence_dir,
                "phase=streaming-b-accept STATUS OK but inbound SYN never reached the backlog",
            );
            handle.shutdown();
            let _ = scope.shutdown().await;
            return;
        }
    };
    assert!(
        streaming
            .get_connection(b_id)
            .is_some_and(|conn| conn.state() == ConnectionState::Established),
        "inbound connection must be Established after accept"
    );
    append_evidence(
        &evidence_dir,
        "streaming-b-established",
        &format!("attempts={connect_attempts}"),
    );
    let b_peer_len = 0;
    let b_socket_id = loop {
        let line = stream_control.command("CONNECT_STATUS 0").await;
        if let Some(id) = line.strip_prefix("CONNECTED ") {
            break id.parse::<usize>().expect("connected socket id");
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    };

    // Direction B small.
    let app_b_small = b"plan194-b-probe-e";
    assert!(stream_control.write_stream(b_socket_id, app_b_small).await);
    let mut b_collected = Vec::new();
    pump_until_streaming(
        &mut handle,
        &mut coord,
        &mut dest,
        &mut dispatcher,
        &mut session,
        &mut routing,
        &mut streaming,
        &destination_outbound,
        &local_identity,
        &local_ls2,
        &delivery,
        &mut send_rng,
        reference_hash.as_bytes(),
        &evidence_dir,
        tokio::time::Instant::now() + STREAM_WAIT,
        "streaming-b-data",
        |manager| {
            for delivered in manager.drain_delivered_for(b_id) {
                b_collected.extend_from_slice(&delivered.bytes);
            }
            b_collected.as_slice() == app_b_small.as_slice()
        },
    )
    .await;
    if b_collected == app_b_small {
        append_evidence(
            &evidence_dir,
            "streaming-b-data-digest",
            &format!(
                "payload_len={} digest={} peer_line_len={b_peer_len} match=true",
                b_collected.len(),
                sha256_hex(&b_collected),
            ),
        );
    } else {
        record_stop(&evidence_dir, "phase=streaming-b-data digest mismatch");
        handle.shutdown();
        let _ = scope.shutdown().await;
        return;
    }

    // Direction B reverse (i2pr -> Java over the accepted stream).
    let (b_local_port, b_remote_port) = {
        let inbound = streaming.get_connection(b_id).expect("inbound connection");
        (inbound.local_port(), inbound.remote_port())
    };
    let mut app_b_rev = Vec::with_capacity(2048);
    for index in 0..2048 {
        app_b_rev.push((index % 199) as u8);
    }
    let b_reply_desc = {
        let inbound = streaming.get_connection(b_id).expect("inbound connection");
        let peer_bytes = inbound
            .peer_destination()
            .cloned()
            .expect("peer destination retained")
            .encode_to_vec(65535)
            .expect("encode peer destination");
        RemoteDestination {
            destination_hash: *i2pr_crypto::sha256(&peer_bytes).as_bytes(),
            signing_public_key: inbound.peer_signing_key().clone(),
            static_public_key: [0u8; 32],
        }
    };
    let mut b_rev_offset = 0;
    let mut b_rev_fragments = 0u64;
    while b_rev_offset < app_b_rev.len() {
        let end = (b_rev_offset + 1024).min(app_b_rev.len());
        let fragment = streaming
            .send_data(
                b_id,
                &local_identity,
                &b_reply_desc,
                b_local_port,
                b_remote_port,
                &app_b_rev[b_rev_offset..end],
                wall_ms(),
            )
            .expect("send B reverse fragment");
        send_transport_request(
            &fragment,
            &routing,
            &mut session,
            &destination_outbound,
            &local_identity,
            &local_ls2,
            &delivery,
            &mut send_rng,
        )
        .await;
        b_rev_offset = end;
        b_rev_fragments += 1;
    }
    let observed_b_rev = stream_control
        .read_stream(b_socket_id, app_b_rev.len())
        .await
        .unwrap_or_default();
    if observed_b_rev != app_b_rev {
        record_stop(
            &evidence_dir,
            &format!(
                "phase=streaming-b-reverse expected_len={} observed_len={}",
                app_b_rev.len(),
                observed_b_rev.len(),
            ),
        );
        handle.shutdown();
        let _ = scope.shutdown().await;
        return;
    }
    append_evidence(
        &evidence_dir,
        "streaming-b-reverse-data-digest",
        &format!(
            "payload_len={} fragments={b_rev_fragments} digest={} match=true",
            observed_b_rev.len(),
            sha256_hex(&observed_b_rev),
        ),
    );

    let b_close_request = streaming
        .send_close(
            b_id,
            &local_identity,
            &b_reply_desc,
            b_local_port,
            b_remote_port,
            wall_ms(),
        )
        .expect("send B close");
    send_transport_request(
        &b_close_request,
        &routing,
        &mut session,
        &destination_outbound,
        &local_identity,
        &local_ls2,
        &delivery,
        &mut send_rng,
    )
    .await;
    pump_until_streaming(
        &mut handle,
        &mut coord,
        &mut dest,
        &mut dispatcher,
        &mut session,
        &mut routing,
        &mut streaming,
        &destination_outbound,
        &local_identity,
        &local_ls2,
        &delivery,
        &mut send_rng,
        reference_hash.as_bytes(),
        &evidence_dir,
        tokio::time::Instant::now() + Duration::from_secs(30),
        "streaming-b-close",
        |manager| {
            manager
                .get_connection(b_id)
                .is_some_and(|conn| conn.state() == ConnectionState::Closed)
        },
    )
    .await;
    let b_close_eof = stream_control.stream_eof(b_socket_id).await;
    if !b_close_eof {
        record_stop(
            &evidence_dir,
            "phase=streaming-b-close local state Closed but reference socket never reached EOF",
        );
        handle.shutdown();
        let _ = scope.shutdown().await;
        return;
    }
    append_evidence(&evidence_dir, "streaming-b-close", "state=Closed eof=true");

    let cleanup_connections = streaming.connection_count();
    let cleanup_queued = streaming.outbound_queue_len();
    let cleanup_delivered = streaming.pending_delivered_bytes();
    assert_eq!(cleanup_queued, 0, "no queued transport after close");
    assert_eq!(cleanup_delivered, 0, "no undrained bytes after close");
    append_evidence(
        &evidence_dir,
        "manager-cleanup",
        &format!(
            "connections={cleanup_connections} queued={cleanup_queued} delivered={cleanup_delivered}"
        ),
    );

    assert!(dest.note_direct_transport_attempt().is_err());
    append_evidence(&evidence_dir, "direct-rejected", "true");

    let mut scheduler = TunnelLivenessScheduler::new(LivenessConfig::plan_185_defaults());
    scheduler.advance_time(wall_ms());
    scheduler
        .register_pair(
            i2pr_tunnel::pool::TunnelSlot::from_raw(1),
            i2pr_tunnel::pool::TunnelSlot::from_raw(2),
        )
        .expect("pair");
    scheduler.advance_time(wall_ms() + FIRST_LIVENESS_DELAY_MS + 1);
    let action = scheduler.drive();
    assert!(matches!(action, LivenessAction::SendTest { .. }));
    append_evidence(&evidence_dir, "liveness-first-test", "passed");

    handle.shutdown();
    let _ = scope.shutdown().await;
    let final_snapshot = handle.snapshot();
    assert_eq!(final_snapshot.pending_outbound, 0);
    assert_eq!(final_snapshot.pending_inbound, 0);
    assert_eq!(final_snapshot.active_sessions, 0);
    append_evidence(&evidence_dir, "shutdown-baseline", "true");
    let _ = PeerId::from_hash(java_hash);
}

/// Plan 197 — CI-fast regression proving the daemon-owned
/// `verify_reference_router_info` path surfaces the typed
/// `pq_capabilities()` for a Java I2P 2.13.0 SSU2 address that
/// carries `pq=4,3`. The in-tree fixture is built from the
/// production codec via `Mapping::from_entries`, so it does not
/// depend on a live Java runtime. The exact-pinned Java router
/// publishes `pq=4,3` unconditionally via
/// `UDPTransport.addSSU2Options` (`router/java/src/net/i2p/router/
/// transport/udp/UDPTransport.java:148-153,1011-1021`); this
/// regression is the parser-only counterpart to that wire form.
#[test]
fn java_pq_capabilities_surfaced() {
    let static_pub: [u8; 32] = [0xa1; 32];
    let intro_bytes: [u8; 32] = [0x24; 32];
    let options = Mapping::from_entries(vec![
        ("host".to_string(), "127.0.0.1".to_string()),
        ("port".to_string(), "44002".to_string()),
        ("v".to_string(), "2".to_string()),
        ("s".to_string(), i2p_b64_encode(&static_pub)),
        ("i".to_string(), i2p_b64_encode(&intro_bytes)),
        ("caps".to_string(), "46BC".to_string()),
        ("mtu".to_string(), "1280".to_string()),
        // The exact-pinned Java 2.13.0 RouterInfo carries this
        // option unconditionally for the high-MTU publish form.
        ("pq".to_string(), "4,3".to_string()),
    ])
    .expect("options");
    let bundle =
        i2pr_crypto::RouterIdentityBundle::generate(&mut rand_core::OsRng).expect("identity");
    let address = RouterAddress::new(
        10,
        Date::from_millis(9_999_999_999_999),
        "SSU2".to_string(),
        options,
    )
    .expect("address");
    let ri_options = Mapping::from_entries(vec![
        ("router.version".to_string(), "0.9.66".to_string()),
        ("netId".to_string(), "2".to_string()),
    ])
    .expect("ri options");
    let info = bundle
        .sign_router_info(
            Date::from_millis(wall_secs().saturating_mul(1000)),
            vec![address],
            Vec::new(),
            ri_options,
        )
        .expect("sign");
    let router_info = info
        .encode_to_vec(i2pr_runtime::constants::MAX_ESTABLISHMENT_ROUTER_INFO_BYTES)
        .expect("encode");

    let (_hash, ssu2) = verify_reference_router_info(&router_info).expect("verify java-style");
    assert_eq!(
        ssu2.pq_capabilities().schemes(),
        &[Ssu2PqKem::MlKem768, Ssu2PqKem::MlKem512]
    );
    assert_eq!(ssu2.pq_capabilities().as_wire(), "4,3");
    assert!(ssu2.pq_capabilities().is_supported());
}

/// Local alphabet-only I2P base64 helper, mirroring the existing
/// `i2p_b64_encode` in `ssu2_daemon_preflight.rs` without pulling in
/// that test's private helper.
fn i2p_b64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-~";
    let mut output = String::new();
    for chunk in bytes.chunks(3) {
        let mut n: u32 = 0;
        for byte in chunk {
            n = (n << 8) | u32::from(*byte);
        }
        n <<= 8 * (3 - chunk.len());
        let digits = match chunk.len() {
            1 => 2,
            2 => 3,
            _ => 4,
        };
        for index in 0..digits {
            output.push(ALPHABET[((n >> (18 - 6 * index)) & 0x3f) as usize] as char);
        }
        for _ in digits..4 {
            output.push('=');
        }
    }
    output
}

// ---- Plan 220 §19 unit rows ------------------------------------------------
// Focused local rows locking the corrected diagnostic semantics. They
// run in the ordinary workspace floor (no external environment).

/// Builds an all-`Known(pass)` fact set whose terminal must be
/// `P220-REVERSE-DELIVERY-PASSED`. Each row below mutates one
/// stage and asserts the honest terminal for that mutation.
fn p220_all_passing_facts() -> P220Facts {
    let known_bool = |v: bool| P220Observed::Known(v);
    P220Facts {
        rust_b_hash_hex: "aa".repeat(32),
        java_b_self_hex: P220Observed::Known("aa".repeat(32)),
        java_b_self_b64: P220Observed::Known("correlate-only".to_owned()),
        hash_match: known_bool(true),
        b_live_has_f: known_bool(true),
        b_live_sha256: P220Observed::Known("bb".repeat(32)),
        b_live_published: P220Observed::Known(1_700_000_000),
        a_stored_present: known_bool(true),
        a_stored_identity_match: known_bool(true),
        a_stored_sha256: P220Observed::Known("bb".repeat(32)),
        a_stored_published: P220Observed::Known(1_700_000_000),
        a_stored_has_f: known_bool(true),
        a_main_router_count: P220Observed::Known(2),
        a_peermanager_b_indexed: known_bool(true),
        selector_observable: known_bool(true),
        selector_kbucket_size: P220Observed::Known(2),
        selector_count: P220Observed::Known(1),
        selector_contains_b: known_bool(true),
        client_lookup_started: known_bool(true),
        client_lookup_peer_selected: known_bool(true),
        client_lookup_succeeded: known_bool(true),
        target_leaseset_present: known_bool(true),
        target_lease_selected: known_bool(true),
        outbound_client_tunnel_selected: known_bool(true),
        garlic_constructed: known_bool(true),
        tunnel_dispatch_submitted: known_bool(true),
        forward_i2pr_to_java_received: known_bool(true),
        reverse_java_send_admitted: known_bool(true),
        reverse_i2pr_tunneldata_observed: known_bool(true),
        reverse_i2pr_payload_recovered: known_bool(true),
    }
}

#[test]
fn p220_reverse_delivery_passed_when_every_stage_known_pass() {
    let facts = p220_all_passing_facts();
    assert_eq!(facts.derive(), P220Terminal::ReverseDeliveryPassed);
}

#[test]
fn p220_hash_mismatch_yields_diagnostic_gap_not_a_missing_b() {
    let mut facts = p220_all_passing_facts();
    facts.hash_match = P220Observed::Known(false);
    // Even though every later stage is Known(pass), the identity
    // itself is untrusted: a diagnostic gap, never "A lacks B".
    assert_eq!(facts.derive(), P220Terminal::ObservabilityGap("HASH"));
    assert!(
        facts
            .derive()
            .token()
            .starts_with("P220-OBSERVABILITY-GAP-HASH")
    );
}

#[test]
fn p220_stored_presence_and_f_are_independent() {
    // Absent record: the A-lacks-B-RI root cause.
    let mut facts = p220_all_passing_facts();
    facts.a_stored_present = P220Observed::Known(false);
    assert_eq!(
        facts.derive(),
        P220Terminal::CorrectedAttribution("A-LACKS-B-RI")
    );
    // Present record without `f`: the stored-lacks-`f` root cause.
    // Presence alone never implies `f` (D220-4).
    let mut facts = p220_all_passing_facts();
    facts.a_stored_present = P220Observed::Known(true);
    facts.a_stored_has_f = P220Observed::Known(false);
    assert_eq!(
        facts.derive(),
        P220Terminal::CorrectedAttribution("A-STORED-B-RI-NOT-F")
    );
}

#[test]
fn p220_stored_live_sha_and_published_are_explicit() {
    let facts = p220_all_passing_facts();
    let detail = facts.detail(&P220Terminal::ReverseDeliveryPassed);
    assert!(detail.contains("bb".repeat(32).as_str()));
    assert!(detail.contains("1700000000"));
    assert!(detail.contains(P220_EPOCH_AUTHORITATIVE));
    assert!(detail.contains(P220_EPOCH_DISPATCH));
}

#[test]
fn p220_peermanager_and_selector_are_independent() {
    // PeerManager indexes B but the probe is unavailable: the
    // selector gap fires; membership is never converted into
    // selector output (D220-5).
    let mut facts = p220_all_passing_facts();
    facts.reverse_i2pr_payload_recovered = P220Observed::Known(false);
    facts.selector_observable = P220Observed::Unknown("selector-unavailable");
    assert_eq!(facts.derive(), P220Terminal::ObservabilityGap("SELECTOR"));
    // Probe observable and excluding B while PeerManager indexes
    // B: the genuine selector boundary.
    let mut facts = p220_all_passing_facts();
    facts.reverse_i2pr_payload_recovered = P220Observed::Known(false);
    facts.client_lookup_started = P220Observed::Known(true);
    facts.selector_contains_b = P220Observed::Known(false);
    assert_eq!(
        facts.derive(),
        P220Terminal::CorrectedAttribution("SELECTOR-EXCLUDES-B")
    );
}

#[test]
fn p220_unknown_stage_yields_its_gap() {
    // PeerManager unobserved with everything earlier passing.
    let mut facts = p220_all_passing_facts();
    facts.reverse_i2pr_payload_recovered = P220Observed::Known(false);
    facts.a_peermanager_b_indexed = P220Observed::Unknown("peermanager-query-failed");
    assert_eq!(
        facts.derive(),
        P220Terminal::ObservabilityGap("PEERMANAGER")
    );
    // Client-NetDB unobserved with selector passing.
    let mut facts = p220_all_passing_facts();
    facts.reverse_i2pr_payload_recovered = P220Observed::Known(false);
    facts.client_lookup_started = P220Observed::Unknown("no-read-only-per-message-observation");
    assert_eq!(
        facts.derive(),
        P220Terminal::ObservabilityGap("CLIENT-NETDB")
    );
}

#[test]
fn p220_earliest_known_fail_wins_after_earlier_known_pass() {
    // B live lacks `f`: fires before the stored stage is read.
    let mut facts = p220_all_passing_facts();
    facts.b_live_has_f = P220Observed::Known(false);
    facts.a_stored_present = P220Observed::Known(false);
    assert_eq!(
        facts.derive(),
        P220Terminal::CorrectedAttribution("B-LIVE-RI-NOT-F")
    );
}

#[test]
fn p220_missing_fact_cannot_yield_root_cause() {
    // Stored present but identity correlation missing: gap, even
    // though `has_f` is Known(false).
    let mut facts = p220_all_passing_facts();
    facts.a_stored_identity_match = P220Observed::Unknown("stored-identity-absent");
    facts.a_stored_has_f = P220Observed::Known(false);
    assert_eq!(
        facts.derive(),
        P220Terminal::ObservabilityGap("A-STORED-RI")
    );
}

#[test]
fn p220_forward_receipt_cannot_satisfy_reverse_dispatch() {
    // Forward receipt observed, reverse payload absent, OCMOSJ
    // stages unobserved: the OCMOSJ gap fires — forward success
    // never becomes reverse success (D220-7).
    let mut facts = p220_all_passing_facts();
    facts.forward_i2pr_to_java_received = P220Observed::Known(true);
    facts.reverse_i2pr_payload_recovered = P220Observed::Known(false);
    facts.client_lookup_started = P220Observed::Known(true);
    facts.client_lookup_peer_selected = P220Observed::Known(true);
    facts.client_lookup_succeeded = P220Observed::Known(true);
    facts.target_leaseset_present = P220Observed::Known(true);
    facts.target_lease_selected = P220Observed::Known(true);
    facts.outbound_client_tunnel_selected = P220Observed::Known(true);
    facts.garlic_constructed = P220Observed::Known(true);
    facts.tunnel_dispatch_submitted = P220Observed::Unknown("no-read-only-per-message-observation");
    assert_eq!(facts.derive(), P220Terminal::ObservabilityGap("OCMOSJ"));
    assert_ne!(facts.derive(), P220Terminal::ReverseDeliveryPassed);
}

#[test]
fn p220_terminal_tokens_are_canonical() {
    assert_eq!(
        P220Terminal::CorrectedAttribution("A-LACKS-B-RI").token(),
        "P220-CORRECTED-ATTRIBUTION A-LACKS-B-RI"
    );
    assert_eq!(
        P220Terminal::ObservabilityGap("SELECTOR").token(),
        "P220-OBSERVABILITY-GAP-SELECTOR"
    );
    assert_eq!(
        P220Terminal::ReverseDeliveryPassed.token(),
        "P220-REVERSE-DELIVERY-PASSED"
    );
}

#[test]
fn p220_kv_parser_handles_quoted_capabilities() {
    let kv = p220_parse_kv(
        "P220-EV kind=stored-ri query_hash_hex=aa present=true capabilities=\"Lf\" has_floodfill_capability=true",
    );
    assert_eq!(kv.get("kind").map(String::as_str), Some("stored-ri"));
    assert_eq!(kv.get("capabilities").map(String::as_str), Some("Lf"));
    assert_eq!(
        kv.get("has_floodfill_capability").map(String::as_str),
        Some("true")
    );
    assert!(p220_parse_kv("P220-ERROR invalid-hex-hash").is_empty());
    assert!(p220_parse_kv("J219-EV kind=snapshot").is_empty());
}

// ---- Plan 222 WP J unit rows ------------------------------------------------
// Focused local rows locking the corrected client-NetDB/OCMOSJ semantics.
// They run in the ordinary workspace floor (no external environment).

fn p222_base_facts() -> P222Facts {
    P222Facts {
        selector_equivalent: P220Observed::Known(true),
        selector_nonempty: P220Observed::Known(true),
        target_leaseset_present_pre_send: P220Observed::Known(false),
        tracked_nonce: P220Observed::Known(7),
        status_accepted: P220Observed::Known(true),
        status_no_leaseset: P220Observed::Known(false),
        status_bad_leaseset: P220Observed::Known(false),
        status_expired_leaseset: P220Observed::Known(false),
        status_unsupported_encryption: P220Observed::Known(false),
        status_no_tunnels: P220Observed::Known(false),
        status_best_effort_failure: P220Observed::Known(false),
        status_guaranteed_success: P220Observed::Known(false),
        status_other_terminal: P220Observed::Known(false),
        reverse_i2pr_tunneldata_observed_45s: P220Observed::Known(false),
        reverse_i2pr_payload_recovered_45s: P220Observed::Known(false),
        ordered_statuses: vec![1],
    }
}

fn p222_preflight_full() -> P222Preflight {
    P222Preflight {
        observable: true,
        client_db_resolved: true,
        client_db_is_client: true,
        target_hash_hex: "aa".repeat(32),
        routing_key_hex: "bb".repeat(32),
        routing_key_differs: true,
        target_ls_present_before_send: false,
        facade_floodfill_enabled: false,
        router_uptime_ms: 60_000,
        netdb_search_limit_effective: 5,
        selector_extra_peers: 1,
        selector_width: 6,
        selector_kbucket_size: 4,
        selector_count: 2,
        selector_contains_b: true,
        selector_empty: false,
    }
}

/// Plan 222 §7 A2 — feeding only the old P220 selector observation into
/// the P222 classifier yields the selector-equivalence gap. The frozen
/// P220 row (`selector_contains_b=true` from raw hash + N=3) directly
/// satisfies no P222 terminal.
#[test]
fn p222_old_p220_selector_alone_yields_equivalence_gap() {
    // No preflight at all: the historical P220 observation cannot prove
    // client DBID, routing key, or effective width.
    assert_eq!(
        p222_selector_equivalence(None),
        P220Observed::Unknown("preflight-unreachable")
    );
    let mut facts = p222_base_facts();
    facts.selector_equivalent = P220Observed::Unknown("preflight-unreachable");
    assert_eq!(
        facts.derive(),
        P222Terminal::ObservabilityGapSelectorEquivalence
    );
    assert_eq!(
        facts.derive().token(),
        "P222-OBSERVABILITY-GAP-SELECTOR-EQUIVALENCE"
    );
}

/// J1 — routing-key vs raw-key distinction: the raw target hash is
/// rejected as selector key when the routing key is available.
#[test]
fn p222_routing_key_vs_raw_key_distinction() {
    let mut pre = p222_preflight_full();
    // Distinct keys: equivalent.
    assert_eq!(
        p222_selector_equivalence(Some(&pre)),
        P220Observed::Known(true)
    );
    // Same key for target and routing key: not production-equivalent.
    pre.routing_key_hex = pre.target_hash_hex.clone();
    pre.routing_key_differs = false;
    assert_eq!(
        p222_selector_equivalence(Some(&pre)),
        P220Observed::Known(false)
    );
}

/// J2 — dynamic selector width computation matches pinned
/// `IterativeSearchJob` logic (`total + EXTRA_PEERS`, never hard-coded
/// N=3).
#[test]
fn p222_dynamic_selector_width_computation() {
    let mut pre = p222_preflight_full();
    pre.netdb_search_limit_effective = 5;
    pre.selector_extra_peers = 1;
    pre.selector_width = 6;
    assert_eq!(
        p222_selector_equivalence(Some(&pre)),
        P220Observed::Known(true)
    );
    // Hard-coded N=3 with effective 5 is rejected.
    pre.selector_width = 3;
    assert_eq!(
        p222_selector_equivalence(Some(&pre)),
        P220Observed::Known(false)
    );
    // EXTRA_PEERS must be 1.
    pre.selector_width = 6;
    pre.selector_extra_peers = 2;
    assert_eq!(
        p222_selector_equivalence(Some(&pre)),
        P220Observed::Known(false)
    );
}

/// J3 — client DBID required; fallback-to-main is not a pass.
#[test]
fn p222_client_dbid_required() {
    let mut pre = p222_preflight_full();
    pre.client_db_resolved = false;
    assert!(matches!(
        p222_selector_equivalence(Some(&pre)),
        P220Observed::Unknown(_)
    ));
    let mut pre = p222_preflight_full();
    pre.client_db_is_client = false;
    assert!(matches!(
        p222_selector_equivalence(Some(&pre)),
        P220Observed::Unknown(_)
    ));
    let mut facts = p222_base_facts();
    facts.selector_equivalent = P220Observed::Unknown("client-db-not-proven");
    assert_eq!(
        facts.derive(),
        P222Terminal::ObservabilityGapSelectorEquivalence
    );
}

/// J4 — selector empty maps to no-lookup-peer (client `getAllRouters()`
/// is empty in pinned source, so no fallback rescues the lookup).
#[test]
fn p222_selector_empty_maps_to_no_lookup_peer() {
    let mut facts = p222_base_facts();
    facts.selector_nonempty = P220Observed::Known(false);
    assert_eq!(
        facts.derive(),
        P222Terminal::CorrectedAttributionClientNetdbNoLookupPeer
    );
}

/// J5 — selector nonempty with B absent does not root-cause by itself.
#[test]
fn p222_selector_nonempty_b_absent_is_not_root_cause() {
    // Nonempty selector: the selector stage passes as "lookup has
    // candidates" even when B is absent. With no decisive status and no
    // payload, the terminal is the post-accept gap, never a B-absence
    // root cause.
    let mut facts = p222_base_facts();
    facts.selector_nonempty = P220Observed::Known(true);
    assert_eq!(
        facts.derive(),
        P222Terminal::ObservabilityGapOcmOSJPostAccept
    );
}

/// J6 — tracked-send parser accepts a valid nonce/digest.
#[test]
fn p222_tracked_send_parser_accepts_valid() {
    let digest = "ab".repeat(32);
    let parsed = p222_parse_tracked_sent(&format!(
        "TRACKED_SENT nonce=42 payload_len=27 digest={digest}"
    ));
    assert_eq!(
        parsed,
        Some(TrackedSend {
            nonce: 42,
            payload_len: 27,
            digest,
        })
    );
    let events =
        p222_parse_tracked_status("TRACKED_STATUS nonce=42 count=2 events=1:10,3:59000", 42);
    assert_eq!(
        events,
        Some(vec![
            TrackedStatusEvent {
                status: 1,
                elapsed_ms: 10
            },
            TrackedStatusEvent {
                status: 3,
                elapsed_ms: 59000
            },
        ])
    );
    assert_eq!(
        p222_parse_tracked_status("TRACKED_STATUS nonce=42 count=0 events=none", 42),
        Some(Vec::new())
    );
}

/// J7 — tracked-send parser rejects malformed nonce/digest/count.
#[test]
fn p222_tracked_send_parser_rejects_malformed() {
    assert_eq!(
        p222_parse_tracked_sent("TRACKED_SENT payload_len=27 digest=ab"),
        None
    );
    assert_eq!(
        p222_parse_tracked_sent(&format!(
            "TRACKED_SENT nonce=xx payload_len=27 digest={}",
            "ab".repeat(32)
        )),
        None
    );
    assert_eq!(
        p222_parse_tracked_sent("TRACKED_SENT nonce=1 payload_len=27 digest=ZZ"),
        None
    );
    // More than 16 events rejected.
    let many = (0..17)
        .map(|i| format!("1:{i}"))
        .collect::<Vec<_>>()
        .join(",");
    assert_eq!(
        p222_parse_tracked_status(&format!("TRACKED_STATUS nonce=1 count=17 events={many}"), 1),
        None
    );
    // Unknown nonce is Unknown, never a status.
    assert_eq!(
        p222_parse_tracked_status("TRACKED_STATUS_UNKNOWN nonce=1", 1),
        None
    );
}

/// J8 — ordered status sequence retained (no collapsing to last).
#[test]
fn p222_ordered_status_sequence_retained() {
    let events = vec![
        TrackedStatusEvent {
            status: 1,
            elapsed_ms: 5,
        },
        TrackedStatusEvent {
            status: 3,
            elapsed_ms: 59000,
        },
        TrackedStatusEvent {
            status: 4,
            elapsed_ms: 59500,
        },
    ];
    let (_, _, _, _, _, _, _, _, _, ordered) = p222_status_facts(&events);
    assert_eq!(ordered, vec![1, 3, 4]);
}

/// J9 — ACCEPTED alone does not pass client-NetDB.
#[test]
fn p222_accepted_alone_does_not_pass_client_netdb() {
    let mut facts = p222_base_facts();
    facts.status_accepted = P220Observed::Known(true);
    facts.status_no_leaseset = P220Observed::Known(false);
    // No decisive failure, no payload: post-accept gap, never a pass.
    assert_eq!(
        facts.derive(),
        P222Terminal::ObservabilityGapOcmOSJPostAccept
    );
    assert_ne!(facts.derive(), P222Terminal::ReverseDeliveryPassed);
}

/// J10 — NO_LEASESET maps to client-NetDB usable-LS failure.
#[test]
fn p222_no_leaseset_maps_to_client_netdb_failure() {
    let mut facts = p222_base_facts();
    facts.status_no_leaseset = P220Observed::Known(true);
    assert_eq!(
        facts.derive(),
        P222Terminal::CorrectedAttributionClientNetdbNoUsableLeaseset
    );
    assert_eq!(
        facts.derive().token(),
        "P222-CORRECTED-ATTRIBUTION CLIENT-NETDB-NO-USABLE-LEASESET"
    );
}

/// J11 — NO_TUNNELS maps only to the combined OCMOSJ boundary.
#[test]
fn p222_no_tunnels_maps_to_combined_boundary() {
    let mut facts = p222_base_facts();
    facts.status_no_tunnels = P220Observed::Known(true);
    assert_eq!(
        facts.derive(),
        P222Terminal::CorrectedAttributionOcmOSJNoUsableTunnelOrGarlicPath
    );
    // The terminal does not distinguish outbound vs inbound-ACK vs
    // garlic-constructor sub-causes; the token is the combined boundary.
    assert!(
        facts
            .derive()
            .token()
            .contains("OCMOSJ-NO-USABLE-TUNNEL-OR-GARLIC-PATH")
    );
}

/// J12 — BEST_EFFORT_FAILURE + no 45 s TunnelData maps to the
/// dispatch-path-reached boundary.
#[test]
fn p222_best_effort_failure_without_tunneldata_maps_to_dispatch_boundary() {
    let mut facts = p222_base_facts();
    facts.status_best_effort_failure = P220Observed::Known(true);
    facts.reverse_i2pr_tunneldata_observed_45s = P220Observed::Known(false);
    assert_eq!(
        facts.derive(),
        P222Terminal::CorrectedAttributionJavaDispatchPathReachedI2prTunneldataNotObserved
    );
}

/// J13 — GUARANTEED_SUCCESS + no payload maps to evidence contradiction.
#[test]
fn p222_guaranteed_success_without_payload_is_contradiction() {
    let mut facts = p222_base_facts();
    facts.status_guaranteed_success = P220Observed::Known(true);
    facts.reverse_i2pr_payload_recovered_45s = P220Observed::Known(false);
    assert_eq!(
        facts.derive(),
        P222Terminal::EvidenceContradictionAckSuccessWithoutI2prPayload
    );
}

/// J14 — no terminal callback maps to the observability gap.
#[test]
fn p222_no_terminal_callback_maps_to_gap() {
    let (accepted, _, _, _, _, _, _, _, other, ordered) = p222_status_facts(&[]);
    assert!(matches!(accepted, P220Observed::Unknown(_)));
    assert!(matches!(other, P220Observed::Unknown(_)));
    assert!(ordered.is_empty());
    let mut facts = p222_base_facts();
    facts.status_accepted = accepted;
    facts.status_other_terminal = other;
    facts.ordered_statuses = ordered;
    assert_eq!(
        facts.derive(),
        P222Terminal::ObservabilityGapOcmOSJPostAccept
    );
}

/// J15 — later success overrides probable failure for
/// strongest-evidence evaluation while preserving order.
#[test]
fn p222_later_success_overrides_probable_failure() {
    let mut facts = p222_base_facts();
    facts.status_best_effort_failure = P220Observed::Known(true);
    facts.status_guaranteed_success = P220Observed::Known(true);
    facts.reverse_i2pr_tunneldata_observed_45s = P220Observed::Known(false);
    facts.reverse_i2pr_payload_recovered_45s = P220Observed::Known(false);
    // Guaranteed success is stronger later evidence than the earlier
    // best-effort failure; with no payload it is a contradiction, not
    // the dispatch boundary.
    assert_eq!(
        facts.derive(),
        P222Terminal::EvidenceContradictionAckSuccessWithoutI2prPayload
    );
    // With payload recovered the run passes regardless of the earlier
    // probable failure.
    facts.reverse_i2pr_payload_recovered_45s = P220Observed::Known(true);
    assert_eq!(facts.derive(), P222Terminal::ReverseDeliveryPassed);
}

/// J16 — the 70-second diagnostic deadline cannot alter the frozen
/// 45-second payload outcome.
#[test]
fn p222_status_deadline_cannot_alter_payload_outcome() {
    assert_eq!(P222_STATUS_OBSERVATION_DEADLINE, Duration::from_secs(70));
    assert!(P222_STATUS_OBSERVATION_DEADLINE >= Duration::from_secs(60));
    assert!(P222_STATUS_OBSERVATION_DEADLINE <= Duration::from_secs(75));
    assert_eq!(DATAGRAM_WAIT, Duration::from_secs(45));
    // A frozen 45-second failure stays failed for payload purposes even
    // when a later status arrives; the classifier never turns it into
    // `P222-REVERSE-DELIVERY-PASSED` without payload recovery.
    let mut facts = p222_base_facts();
    facts.reverse_i2pr_payload_recovered_45s = P220Observed::Known(false);
    facts.status_guaranteed_success = P220Observed::Known(false);
    assert_ne!(facts.derive(), P222Terminal::ReverseDeliveryPassed);
}

/// Plan 222 terminal tokens are canonical and no extra terminal may be
/// invented during execution.
#[test]
fn p222_terminal_tokens_are_canonical() {
    assert_eq!(
        P222Terminal::ObservabilityGapSelectorEquivalence.token(),
        "P222-OBSERVABILITY-GAP-SELECTOR-EQUIVALENCE"
    );
    assert_eq!(
        P222Terminal::CorrectedAttributionClientNetdbNoLookupPeer.token(),
        "P222-CORRECTED-ATTRIBUTION CLIENT-NETDB-NO-LOOKUP-PEER"
    );
    assert_eq!(
        P222Terminal::ReverseDeliveryPassed.token(),
        "P222-REVERSE-DELIVERY-PASSED"
    );
}

// ---- Plan 224 unit rows (WP-F) --------------------------------------------
// Attribution-only classifier, snapshot parsers, and whitelist-only
// log sanitizer. Every terminal below derives from an exact
// fixed-string observation; no network, no Java, no timing.

fn p224_test_target_hex() -> String {
    "ab".repeat(32)
}

fn p224_test_b_hex() -> String {
    "cd".repeat(32)
}

fn p224_test_target_b64() -> String {
    format!("{}=", "T".repeat(43))
}

fn p224_test_b_b64() -> String {
    format!("{}=", "B".repeat(43))
}

fn p224_test_helper_b64() -> String {
    format!("{}=", "H".repeat(43))
}

fn p224_main_row(target: &str, raw: bool, validated: bool, rap: &str, current: &str) -> String {
    format!(
        "P224-EV kind=main-ls target_hash_hex={target} observable=true raw_present={raw} validated_present={validated} entry_type=3 received_as_published={rap} received_as_reply=false received_by_hex=none ls2_unpublished=false lease_count=1 key_count=1 key_types=4 latest_lease_ms=1700000000000 current={current}"
    )
}

fn p224_client_row(
    client: &str,
    target: &str,
    resolved: bool,
    is_client: bool,
    validated: bool,
) -> String {
    format!(
        "P224-EV kind=client-ls client_dbid_hex={client} target_hash_hex={target} observable=true client_db_resolved={resolved} client_db_is_client={is_client} raw_present={validated} validated_present={validated} entry_type=3 received_as_published=true received_as_reply=false received_by_hex={client} ls2_unpublished=false lease_count=1 key_count=1 key_types=4 latest_lease_ms=1700000000000 current=true"
    )
}

fn p224_answerable_main(target: &str) -> P224MainLs {
    p224_parse_main_ls(&p224_main_row(target, true, true, "true", "true"))
        .expect("answerable main snapshot parses")
}

fn p224_present_client(client: &str, target: &str) -> P224ClientLs {
    p224_parse_client_ls(&p224_client_row(client, target, true, true, true))
        .expect("present client snapshot parses")
}

fn p224_absent_client(client: &str, target: &str) -> P224ClientLs {
    p224_parse_client_ls(&p224_client_row(client, target, true, true, false))
        .expect("absent client snapshot parses")
}

fn p224_observable_trace(target: &str) -> P224Trace {
    P224Trace {
        observable: true,
        target_hash_hex: target.to_owned(),
        ..P224Trace::default()
    }
}

fn p224_test_tmpdir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("i2pr-p224-test-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("test tmpdir");
    dir
}

const P224_LOGGER_CONFIG: &str = "logger.defaultLevel=ERROR\nlogger.minimumOnScreenLevel=CRIT\nlogger.flushInterval=1\nlogger.record.net.i2p.router.networkdb.kademlia.IterativeSearchJob=INFO\nlogger.record.net.i2p.router.networkdb.HandleDatabaseLookupMessageJob=DEBUG\nlogger.record.net.i2p.router.tunnel.InboundMessageDistributor=INFO\n";

fn p224_write_log_tree(
    root: &Path,
    side: &str,
    log_name: &str,
    log_body: &str,
    with_logger_config: bool,
) -> PathBuf {
    let side_dir = root.join(side);
    let logs_dir = side_dir.join("logs");
    std::fs::create_dir_all(&logs_dir).expect("logs dir");
    std::fs::write(logs_dir.join(log_name), log_body).expect("log file");
    if with_logger_config {
        std::fs::write(side_dir.join("logger.config"), P224_LOGGER_CONFIG).expect("logger.config");
    }
    logs_dir
}

/// Plan 224 §15.1 — the main-LS snapshot distinguishes raw absent
/// from validated absent (raw store without a validated LeaseSet is
/// not answerable, but it is a different stage than absence).
#[test]
fn p224_main_snapshot_distinguishes_raw_absent_from_validated_absent() {
    let target = p224_test_target_hex();
    let raw_only = p224_parse_main_ls(&p224_main_row(&target, true, false, "unknown", "unknown"))
        .expect("raw-only snapshot parses");
    assert!(raw_only.raw_present);
    assert!(!raw_only.validated_present);
    let absent = p224_parse_main_ls(&p224_main_row(&target, false, false, "unknown", "unknown"))
        .expect("absent snapshot parses");
    assert!(!absent.raw_present);
    assert!(!absent.validated_present);
    // Raw-only is invalid-or-stale, never absent.
    let trace = p224_observable_trace(&target);
    assert_eq!(
        p224_classify(
            &target,
            Some(&raw_only),
            None,
            None,
            None,
            &trace,
            true,
            &[1, 21],
            false
        ),
        P224Terminal::AttributionBMainLsInvalidOrStale
    );
    assert_eq!(
        p224_classify(
            &target,
            Some(&absent),
            None,
            None,
            None,
            &trace,
            true,
            &[1, 21],
            false
        ),
        P224Terminal::AttributionBMainLsAbsent
    );
}

/// Plan 224 §15.2 — the main-LS snapshot records
/// received-as-published exactly (the pinned answerability flag).
#[test]
fn p224_main_snapshot_records_received_as_published() {
    let target = p224_test_target_hex();
    for (rap, expected) in [
        ("true", Some(true)),
        ("false", Some(false)),
        ("unknown", None),
    ] {
        let snap = p224_parse_main_ls(&p224_main_row(&target, true, true, rap, "true"))
            .expect("snapshot parses");
        assert_eq!(snap.received_as_published, expected, "rap={rap}");
    }
    let answerable = p224_answerable_main(&target);
    assert!(p224_b_answerable(&answerable));
    let not_rap =
        p224_parse_main_ls(&p224_main_row(&target, true, true, "false", "true")).expect("parses");
    assert!(!p224_b_answerable(&not_rap));
}

/// Plan 224 §15.3 — the client-LS snapshot rejects a main-DB
/// fallback: `observable=false` or a non-client facade never parses
/// to usable client facts.
#[test]
fn p224_client_snapshot_rejects_main_db_fallback() {
    let target = p224_test_target_hex();
    let client = "ef".repeat(32);
    assert!(
        p224_parse_client_ls(&format!(
            "P224-EV kind=client-ls client_dbid_hex={client} target_hash_hex={target} observable=false reason=client-db-fallback-to-main client_db_resolved=true client_db_is_client=false"
        ))
        .is_none()
    );
    assert!(p224_parse_client_ls(&p224_client_row(&client, &target, true, false, true)).is_none());
    assert!(
        p224_parse_client_ls(&p224_client_row(&client, &target, false, false, false)).is_none()
    );
    let present = p224_present_client(&client, &target);
    assert!(present.client_db_resolved && present.client_db_is_client);
}

/// Plan 224 §15.4 — the LS2 snapshot exposes type/counts only, never
/// key bytes: every key-related field is a short numeric code.
#[test]
fn p224_ls2_snapshot_exposes_type_counts_only_not_key_bytes() {
    let target = p224_test_target_hex();
    let snap = p224_answerable_main(&target);
    assert_eq!(snap.entry_type, 3);
    assert_eq!(snap.key_count, 1);
    for code in snap.key_types.split(',') {
        assert!(!code.is_empty());
        assert!(
            code.len() <= 2,
            "key type code must be numeric, got {code:?}"
        );
        assert!(code.bytes().all(|b| b.is_ascii_digit()), "code {code:?}");
    }
    // The only 64-hex fields are hashes (target + received-by), never keys.
    assert!(p224_is_hex64(&snap.target_hash_hex));
    assert!(snap.received_by_hex == "none" || p224_is_hex64(&snap.received_by_hex));
}

/// Plan 224 §15.5 — B absent maps to `B-MAIN-LS-ABSENT`.
#[test]
fn p224_b_absent_maps_to_absent_terminal() {
    let target = p224_test_target_hex();
    let absent = p224_parse_main_ls(&p224_main_row(&target, false, false, "unknown", "unknown"))
        .expect("parses");
    let trace = p224_observable_trace(&target);
    assert_eq!(
        p224_classify(
            &target,
            Some(&absent),
            None,
            None,
            None,
            &trace,
            true,
            &[1, 21],
            false
        ),
        P224Terminal::AttributionBMainLsAbsent
    );
}

/// Plan 224 §15.6 — B invalid/stale maps correctly (validated
/// absent, current false, or current unknown while raw present).
#[test]
fn p224_b_invalid_or_stale_mapping() {
    let target = p224_test_target_hex();
    let trace = p224_observable_trace(&target);
    for (rap, current) in [
        ("true", "false"),
        ("true", "unknown"),
        ("unknown", "unknown"),
    ] {
        let snap =
            p224_parse_main_ls(&p224_main_row(&target, true, true, rap, current)).expect("parses");
        // validated+RAP but not current (or unknown RAP/current) is stale,
        // except RAP=false which is the not-answerable stage below.
        if rap == "true" || current != "true" {
            assert_eq!(
                p224_classify(
                    &target,
                    Some(&snap),
                    None,
                    None,
                    None,
                    &trace,
                    true,
                    &[1, 21],
                    false
                ),
                P224Terminal::AttributionBMainLsInvalidOrStale,
                "rap={rap} current={current}"
            );
        }
    }
    let unvalidated =
        p224_parse_main_ls(&p224_main_row(&target, true, false, "unknown", "unknown"))
            .expect("parses");
    assert_eq!(
        p224_classify(
            &target,
            Some(&unvalidated),
            None,
            None,
            None,
            &trace,
            true,
            &[1, 21],
            false
        ),
        P224Terminal::AttributionBMainLsInvalidOrStale
    );
}

/// Plan 224 §15.7 — B present but not received-as-published maps to
/// not-query-answerable (pinned Java never answers from it).
#[test]
fn p224_b_present_but_not_published_maps_correctly() {
    let target = p224_test_target_hex();
    let trace = p224_observable_trace(&target);
    for rap in ["false", "unknown"] {
        let snap =
            p224_parse_main_ls(&p224_main_row(&target, true, true, rap, "true")).expect("parses");
        // current=true + validated + RAP!=true: unknown RAP falls to
        // stale only when current is not proven; here current is true
        // so RAP=false/unknown both mean not-query-answerable... except
        // unknown RAP with proven current: the answerability helper is
        // false, and the classifier orders stale (current check) before
        // RAP. current==Some(true) here, so we reach the RAP gate.
        let terminal = p224_classify(
            &target,
            Some(&snap),
            None,
            None,
            None,
            &trace,
            true,
            &[1, 21],
            false,
        );
        if rap == "false" {
            assert_eq!(terminal, P224Terminal::AttributionBMainLsNotQueryAnswerable);
        } else {
            // Unknown RAP with proven current: not answerable by the
            // pinned rule (answerability requires proven RAP).
            assert_eq!(terminal, P224Terminal::AttributionBMainLsNotQueryAnswerable);
        }
    }
}

/// Plan 224 §15.8 — B answerable + no inbound client tunnel maps
/// correctly (and wins over the crypto terminal: earliest stage).
#[test]
fn p224_answerable_plus_no_inbound_tunnel_maps_correctly() {
    let target = p224_test_target_hex();
    let client = "ef".repeat(32);
    let b_pre = p224_answerable_main(&target);
    let a_pre = p224_absent_client(&client, &target);
    let mut trace = p224_observable_trace(&target);
    trace.no_ib_client_tunnel = true;
    trace.no_ratchet_or_elg_support = true;
    assert_eq!(
        p224_classify(
            &target,
            Some(&b_pre),
            None,
            Some(&a_pre),
            None,
            &trace,
            true,
            &[1, 21],
            false
        ),
        P224Terminal::AttributionANoInboundClientReplyTunnel
    );
}

/// Plan 224 §15.9 — B answerable + no reply crypto maps correctly.
#[test]
fn p224_answerable_plus_no_reply_crypto_maps_correctly() {
    let target = p224_test_target_hex();
    let client = "ef".repeat(32);
    let b_pre = p224_answerable_main(&target);
    let a_pre = p224_absent_client(&client, &target);
    let mut trace = p224_observable_trace(&target);
    trace.no_ratchet_or_elg_support = true;
    assert_eq!(
        p224_classify(
            &target,
            Some(&b_pre),
            None,
            Some(&a_pre),
            None,
            &trace,
            true,
            &[1, 21],
            false
        ),
        P224Terminal::AttributionALookupReplyCryptoUnavailable
    );
}

/// Plan 224 §15.10 — selector-contains-B alone never satisfies
/// `query_to_b`: only an exact-target ISJ try line carrying Router
/// B's hash does. End-to-end through the fixed-substring sanitizer.
#[test]
fn p224_selector_membership_alone_does_not_satisfy_query_to_b() {
    let target = p224_test_target_hex();
    let target_b64 = p224_test_target_b64();
    let b_b64 = p224_test_b_b64();
    // A different peer's hash on the try line: the lookup started
    // and failed, but B was never queried.
    let other_b64 = format!("{}=", "Q".repeat(43));
    let dir = p224_test_tmpdir("selector-not-query");
    let a_logs = p224_write_log_tree(
        &dir,
        "a",
        "log-router-0.txt",
        &format!(
            "2026-01-01 00:00:01 INFO  Job-1: ISJ try 0 for LS {target_b64} to {other_b64} direct? false reply via client tunnel? true\n2026-01-01 00:00:16 INFO  Job-1: ISJ for {target_b64} failed with 0 remaining after 15000\n"
        ),
        true,
    );
    let b_logs = p224_write_log_tree(
        &dir,
        "b",
        "log-router-0.txt",
        "2026-01-01 00:00:02 DEBUG Handling database lookup message for some-other-key with replies going to from\n",
        true,
    );
    let scan_a = p224_scan_log_dir(&a_logs, &target_b64, &b_b64, Some(&p224_test_helper_b64()))
        .expect("scan A");
    let scan_b = p224_scan_log_dir(&b_logs, &target_b64, &b_b64, Some(&p224_test_helper_b64()))
        .expect("scan B");
    assert_eq!(scan_a.isj_try, 1);
    assert_eq!(scan_a.isj_try_to_b, 0);
    let trace = p224_build_trace(
        &target,
        Some(&target_b64),
        Some(&b_b64),
        Some(&p224_test_helper_b64()),
        Some(&scan_a),
        Some(&scan_b),
        p224_logger_config_installed(&a_logs),
        p224_logger_config_installed(&b_logs),
    );
    assert!(trace.observable);
    assert!(trace.query_started);
    assert!(!trace.query_to_b);
    assert!(trace.search_failed);
    let b_pre = p224_answerable_main(&target);
    let client = "ef".repeat(32);
    let a_pre = p224_absent_client(&client, &target);
    assert_eq!(
        p224_classify(
            &target,
            Some(&b_pre),
            None,
            Some(&a_pre),
            None,
            &trace,
            true,
            &[1, 21],
            false
        ),
        P224Terminal::AttributionASearchExhaustedWithoutQueryingB
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Plan 224 §15.11 — query-to-B true + B receive false maps to
/// lookup-not-received (B-side DLM logging proven live by an
/// unrelated lookup, so the absence is delivery failure, not
/// logging-off).
#[test]
fn p224_query_to_b_without_b_receipt_maps_correctly() {
    let target = p224_test_target_hex();
    let target_b64 = p224_test_target_b64();
    let b_b64 = p224_test_b_b64();
    let dir = p224_test_tmpdir("to-b-not-received");
    let a_logs = p224_write_log_tree(
        &dir,
        "a",
        "log-router-0.txt",
        &format!(
            "INFO Job-7: ISJ try 0 for LS {target_b64} to {b_b64} direct? false reply via client tunnel? true\nINFO Job-7: ISJ for {target_b64} failed with 0 remaining after 15000\n"
        ),
        true,
    );
    // B handled an unrelated lookup (proves DLM logging live) but
    // never saw our target.
    let b_logs = p224_write_log_tree(
        &dir,
        "b",
        "log-router-0.txt",
        "DEBUG Handling database lookup message for UnrelatedKeyAAAAAAAAAAAAAAAAAAAAAAAAAA= with replies going to from\n",
        true,
    );
    let scan_a = p224_scan_log_dir(&a_logs, &target_b64, &b_b64, Some(&p224_test_helper_b64()))
        .expect("scan A");
    let scan_b = p224_scan_log_dir(&b_logs, &target_b64, &b_b64, Some(&p224_test_helper_b64()))
        .expect("scan B");
    let trace = p224_build_trace(
        &target,
        Some(&target_b64),
        Some(&b_b64),
        Some(&p224_test_helper_b64()),
        Some(&scan_a),
        Some(&scan_b),
        p224_logger_config_installed(&a_logs),
        p224_logger_config_installed(&b_logs),
    );
    assert!(trace.observable && trace.query_to_b && !trace.b_lookup_received);
    assert!(trace.b_dlm_proven_live);
    let b_pre = p224_answerable_main(&target);
    let client = "ef".repeat(32);
    let a_pre = p224_absent_client(&client, &target);
    assert_eq!(
        p224_classify(
            &target,
            Some(&b_pre),
            None,
            Some(&a_pre),
            None,
            &trace,
            true,
            &[1, 21],
            false
        ),
        P224Terminal::AttributionAToBLookupNotReceived
    );
    // Without the B-side liveness proof the same shape is honestly a gap.
    let mut trace_no_proof = trace.clone();
    trace_no_proof.b_dlm_proven_live = false;
    assert_eq!(
        p224_classify(
            &target,
            Some(&b_pre),
            None,
            Some(&a_pre),
            None,
            &trace_no_proof,
            true,
            &[1, 21],
            false
        ),
        P224Terminal::ObservabilityGapLookupPath
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Plan 224 §15.12 — B answer true + A inbound false maps to
/// reply-not-observed (the non-target-specific encryption-error bit
/// is supporting evidence only and never changes the terminal).
#[test]
fn p224_b_answer_without_a_inbound_maps_to_reply_not_observed() {
    let target = p224_test_target_hex();
    let target_b64 = p224_test_target_b64();
    let b_b64 = p224_test_b_b64();
    let dir = p224_test_tmpdir("reply-not-observed");
    let a_logs = p224_write_log_tree(
        &dir,
        "a",
        "log-router-0.txt",
        &format!(
            "INFO Job-9: ISJ try 1 for LS {target_b64} to {b_b64} direct? false reply via client tunnel? true\nINFO Job-9: ISJ for {target_b64} failed with 0 remaining after 15000\n"
        ),
        true,
    );
    let b_logs = p224_write_log_tree(
        &dir,
        "b",
        "log-router-0.txt",
        &format!(
            "DEBUG Handling database lookup message for {target_b64} with replies going to fromKey\nINFO We have the published LS {target_b64}, answering query\nERROR DLM reply encryption error\n"
        ),
        true,
    );
    let scan_a = p224_scan_log_dir(&a_logs, &target_b64, &b_b64, Some(&p224_test_helper_b64()))
        .expect("scan A");
    let scan_b = p224_scan_log_dir(&b_logs, &target_b64, &b_b64, Some(&p224_test_helper_b64()))
        .expect("scan B");
    let trace = p224_build_trace(
        &target,
        Some(&target_b64),
        Some(&b_b64),
        Some(&p224_test_helper_b64()),
        Some(&scan_a),
        Some(&scan_b),
        p224_logger_config_installed(&a_logs),
        p224_logger_config_installed(&b_logs),
    );
    assert!(trace.b_published_ls_answered && !trace.a_client_tunnel_ls_received);
    assert!(trace.reply_encryption_error_seen);
    let b_pre = p224_answerable_main(&target);
    let client = "ef".repeat(32);
    let a_pre = p224_absent_client(&client, &target);
    assert_eq!(
        p224_classify(
            &target,
            Some(&b_pre),
            None,
            Some(&a_pre),
            None,
            &trace,
            true,
            &[1, 21],
            false
        ),
        P224Terminal::AttributionBReplyNotObservedOnAClientTunnel
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Plan 224 §15.13 — A inbound true + client DB absent maps to the
/// store-path contradiction (never an assumed scheduling race: the
/// pinned source stores the DSM inline before the reply job runs).
#[test]
fn p224_a_inbound_but_client_db_absent_is_contradiction() {
    let target = p224_test_target_hex();
    let target_b64 = p224_test_target_b64();
    let b_b64 = p224_test_b_b64();
    let helper_b64 = p224_test_helper_b64();
    let dir = p224_test_tmpdir("tunnel-dsm-not-in-db");
    let a_logs = p224_write_log_tree(
        &dir,
        "a",
        "log-router-0.txt",
        &format!(
            "INFO Job-3: ISJ try 0 for LS {target_b64} to {b_b64} direct? false reply via client tunnel? true\nINFO Storing garlic LS down tunnel for: {target_b64} sent to: {helper_b64}\nINFO Job-3: ISJ for {target_b64} failed with 0 remaining after 15000\n"
        ),
        true,
    );
    let b_logs = p224_write_log_tree(
        &dir,
        "b",
        "log-router-0.txt",
        &format!(
            "DEBUG Handling database lookup message for {target_b64} with replies going to fromKey\nINFO We have the published LS {target_b64}, answering query\n"
        ),
        true,
    );
    let scan_a =
        p224_scan_log_dir(&a_logs, &target_b64, &b_b64, Some(&helper_b64)).expect("scan A");
    let scan_b =
        p224_scan_log_dir(&b_logs, &target_b64, &b_b64, Some(&helper_b64)).expect("scan B");
    let trace = p224_build_trace(
        &target,
        Some(&target_b64),
        Some(&b_b64),
        Some(&helper_b64),
        Some(&scan_a),
        Some(&scan_b),
        p224_logger_config_installed(&a_logs),
        p224_logger_config_installed(&b_logs),
    );
    assert!(trace.a_client_tunnel_ls_received);
    let b_pre = p224_answerable_main(&target);
    let client = "ef".repeat(32);
    let a_pre = p224_absent_client(&client, &target);
    let a_post = p224_absent_client(&client, &target);
    assert_eq!(
        p224_classify(
            &target,
            Some(&b_pre),
            Some(&b_pre),
            Some(&a_pre),
            Some(&a_post),
            &trace,
            true,
            &[1, 21],
            false
        ),
        P224Terminal::EvidenceContradictionClientTunnelDsmNotInClientDb
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Plan 224 §15.14 — client DB present + status 21 maps to the
/// usable-LS contradiction.
#[test]
fn p224_client_db_present_with_status21_is_contradiction() {
    let target = p224_test_target_hex();
    let client = "ef".repeat(32);
    let b_pre = p224_answerable_main(&target);
    let a_pre = p224_absent_client(&client, &target);
    let a_post = p224_present_client(&client, &target);
    let trace = p224_observable_trace(&target);
    assert_eq!(
        p224_classify(
            &target,
            Some(&b_pre),
            Some(&b_pre),
            Some(&a_pre),
            Some(&a_post),
            &trace,
            true,
            &[1, 21],
            false
        ),
        P224Terminal::EvidenceContradictionNoLeasesetWithUsableClientLs
    );
}

/// Plan 224 §15.15 — a status change away from 21 maps to
/// NEXT-BOUNDARY (a successor plan owns the new boundary).
#[test]
fn p224_status_change_maps_to_next_boundary() {
    let target = p224_test_target_hex();
    let client = "ef".repeat(32);
    let b_pre = p224_answerable_main(&target);
    let a_pre = p224_absent_client(&client, &target);
    let a_post = p224_absent_client(&client, &target);
    let trace = p224_observable_trace(&target);
    assert_eq!(
        p224_classify(
            &target,
            Some(&b_pre),
            Some(&b_pre),
            Some(&a_pre),
            Some(&a_post),
            &trace,
            false,
            &[1, 19],
            false
        ),
        P224Terminal::NextBoundary
    );
    // ACCEPTED alone is admission, not a terminal: still a gap.
    assert_eq!(
        p224_classify(
            &target,
            Some(&b_pre),
            Some(&b_pre),
            Some(&a_pre),
            Some(&a_post),
            &trace,
            false,
            &[1],
            false
        ),
        P224Terminal::ObservabilityGapLookupPath
    );
    // An empty sequence is Unknown, never a boundary.
    assert_eq!(
        p224_classify(
            &target,
            Some(&b_pre),
            Some(&b_pre),
            Some(&a_pre),
            Some(&a_post),
            &trace,
            false,
            &[],
            false
        ),
        P224Terminal::ObservabilityGapLookupPath
    );
}

/// Plan 224 §15.16 — digest-matched reverse payload inside the
/// frozen window maps to REVERSE-DELIVERY-PASSED first, regardless
/// of snapshot/trace shape.
#[test]
fn p224_payload_delivery_maps_to_passed() {
    let target = p224_test_target_hex();
    let absent = p224_parse_main_ls(&p224_main_row(&target, false, false, "unknown", "unknown"))
        .expect("parses");
    let trace = P224Trace::default();
    assert_eq!(
        p224_classify(
            &target,
            Some(&absent),
            None,
            None,
            None,
            &trace,
            true,
            &[1, 21],
            true
        ),
        P224Terminal::ReverseDeliveryPassed
    );
}

/// Plan 224 §15.17 — a missing/unreadable log file yields the
/// observability gap, never false protocol facts.
#[test]
fn p224_missing_log_file_yields_gap_not_false_facts() {
    assert!(
        p224_scan_log_dir(
            Path::new("/nonexistent-p224-log-dir"),
            &p224_test_target_b64(),
            &p224_test_b_b64(),
            None,
        )
        .is_none()
    );
    let target = p224_test_target_hex();
    let trace = p224_build_trace(&target, None, None, None, None, None, false, false);
    assert!(!trace.observable);
    assert!(!trace.query_started && !trace.query_to_b && !trace.b_lookup_received);
    let b_pre = p224_answerable_main(&target);
    let client = "ef".repeat(32);
    let a_pre = p224_absent_client(&client, &target);
    assert_eq!(
        p224_classify(
            &target,
            Some(&b_pre),
            None,
            Some(&a_pre),
            None,
            &trace,
            true,
            &[1, 21],
            false
        ),
        P224Terminal::ObservabilityGapLookupPath
    );
    // A truncated scan is likewise Unknown, never facts.
    let mut truncated = P224LogScan::default();
    truncated.truncated = true;
    truncated.files_read = 1;
    truncated.a_isj_any = 5;
    let trace = p224_build_trace(
        &target,
        Some(&p224_test_target_b64()),
        Some(&p224_test_b_b64()),
        Some(&p224_test_helper_b64()),
        Some(&truncated),
        Some(&P224LogScan::default()),
        true,
        true,
    );
    assert!(!trace.observable);
}

/// Plan 224 §15.18 — the sanitizer never emits session key/tag
/// material: a hostile reply-key line changes no fact, and the
/// recorded evidence row carries only the whitelisted booleans.
#[test]
fn p224_sanitizer_censors_session_key_material() {
    let target = p224_test_target_hex();
    let target_b64 = p224_test_target_b64();
    let b_b64 = p224_test_b_b64();
    // Pinned `HandleDatabaseLookupMessageJob` INFO shape carries the
    // ephemeral reply key and ratchet tag on one line (Plan 224
    // §8.1). It must match no target fact and never reach evidence.
    let hostile_key = "deadbeefcafef00d0123456789abcdef0123456789abcdef0123456789abcdef";
    let hostile_tag = "session-tag-9f8e7d6c5b4a39281706f5e4d3c2b1a";
    let dir = p224_test_tmpdir("sanitizer-censors-keys");
    let b_logs = p224_write_log_tree(
        &dir,
        "b",
        "log-router-0.txt",
        &format!(
            "INFO Sending AEAD reply to peerBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB= {hostile_key} {hostile_tag}\nDEBUG Handling database lookup message for UnrelatedKeyCCCCCCCCCCCCCCCCCCCCCCCCCCCCCC= with replies going to from\n"
        ),
        true,
    );
    let a_logs = p224_write_log_tree(
        &dir,
        "a",
        "log-router-0.txt",
        &format!(
            "INFO Job-5: ISJ try 0 for LS {target_b64} to {b_b64} direct? false reply via client tunnel? true\n"
        ),
        true,
    );
    let scan_a = p224_scan_log_dir(&a_logs, &target_b64, &b_b64, Some(&p224_test_helper_b64()))
        .expect("scan A");
    let scan_b = p224_scan_log_dir(&b_logs, &target_b64, &b_b64, Some(&p224_test_helper_b64()))
        .expect("scan B");
    assert_eq!(scan_b.b_answered, 0);
    let trace = p224_build_trace(
        &target,
        Some(&target_b64),
        Some(&b_b64),
        Some(&p224_test_helper_b64()),
        Some(&scan_a),
        Some(&scan_b),
        p224_logger_config_installed(&a_logs),
        p224_logger_config_installed(&b_logs),
    );
    let evidence_dir = dir.join("evidence");
    std::fs::create_dir_all(&evidence_dir).expect("evidence dir");
    record_p224_trace(&evidence_dir, &trace);
    let tsv = std::fs::read_to_string(evidence_dir.join("driver-evidence.tsv")).expect("read tsv");
    assert!(
        tsv.contains("p224-lookup-trace"),
        "trace row must be recorded"
    );
    assert!(
        !tsv.contains(hostile_key),
        "session key must never reach evidence"
    );
    assert!(
        !tsv.contains(hostile_tag),
        "session tag must never reach evidence"
    );
    assert!(
        !tsv.contains("Sending AEAD reply"),
        "raw source line must never reach evidence"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Plan 224 §9.3 — a target-only garlic-store line is not enough to
/// prove delivery to the helper client. The sanitizer requires the
/// exact `sent to: <helper>` hash, preventing another client tunnel's
/// DSM from being attributed to this lookup.
#[test]
fn p224_client_tunnel_receipt_requires_exact_helper_hash() {
    let target_b64 = p224_test_target_b64();
    let helper_b64 = p224_test_helper_b64();
    let other_helper_b64 = format!("{}=", "O".repeat(43));
    let dir = p224_test_tmpdir("client-tunnel-helper-correlation");
    let logs_dir = p224_write_log_tree(
        &dir,
        "a",
        "log-router-0.txt",
        &format!(
            "INFO Storing garlic LS down tunnel for: {target_b64} sent to: {other_helper_b64}\n"
        ),
        true,
    );
    let scan = p224_scan_log_dir(
        &logs_dir,
        &target_b64,
        &p224_test_b_b64(),
        Some(&helper_b64),
    )
    .expect("scan target-only client line");
    assert_eq!(scan.a_client_tunnel_ls, 0);

    std::fs::write(
        logs_dir.join("log-router-0.txt"),
        format!("INFO Storing garlic LS down tunnel for: {target_b64} sent to: {helper_b64}\n"),
    )
    .expect("write exact helper line");
    let scan = p224_scan_log_dir(
        &logs_dir,
        &target_b64,
        &p224_test_b_b64(),
        Some(&helper_b64),
    )
    .expect("scan exact client line");
    assert_eq!(scan.a_client_tunnel_ls, 1);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Plan 224 §15.19 — the sanitizer emits only bounded typed facts:
/// the per-router file cap holds and every count is a plain integer.
#[test]
fn p224_sanitizer_emits_only_bounded_typed_facts() {
    assert_eq!(P224_MAX_LOG_FILES_PER_ROUTER, 16);
    assert_eq!(P224_MAX_LOG_BYTES_PER_ROUTER, 96 * 1024 * 1024);
    let dir = p224_test_tmpdir("sanitizer-bounded");
    let logs_dir = dir.join("a").join("logs");
    std::fs::create_dir_all(&logs_dir).expect("logs dir");
    for i in 0..20 {
        std::fs::write(
            logs_dir.join(format!("log-router-{i}.txt")),
            "INFO unrelated line\n",
        )
        .expect("log file");
    }
    let target_b64 = p224_test_target_b64();
    let scan = p224_scan_log_dir(
        &logs_dir,
        &target_b64,
        &p224_test_b_b64(),
        Some(&p224_test_helper_b64()),
    )
    .expect("scan");
    assert_eq!(scan.files_read, P224_MAX_LOG_FILES_PER_ROUTER);
    assert!(!scan.truncated);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Plan 224 §15.22 — the frozen 45-second result is an input to the
/// classifier, never derived from the trace: later trace collection
/// cannot flip it either way.
#[test]
fn p224_frozen_payload_cannot_be_altered_by_trace() {
    let target = p224_test_target_hex();
    let client = "ef".repeat(32);
    let b_pre = p224_answerable_main(&target);
    let a_pre = p224_absent_client(&client, &target);
    let a_post = p224_present_client(&client, &target);
    // Maximally positive trace with frozen failure: contradiction
    // (D8), never delivery-passed.
    let mut trace = p224_observable_trace(&target);
    trace.query_started = true;
    trace.query_to_b = true;
    trace.b_lookup_received = true;
    trace.b_published_ls_answered = true;
    trace.a_client_tunnel_ls_received = true;
    assert_eq!(
        p224_classify(
            &target,
            Some(&b_pre),
            Some(&b_pre),
            Some(&a_pre),
            Some(&a_post),
            &trace,
            true,
            &[1, 21],
            false
        ),
        P224Terminal::EvidenceContradictionNoLeasesetWithUsableClientLs
    );
    // Frozen success with absent publication: delivery-passed anyway
    // (payload is the authoritative delivery proof).
    let absent = p224_parse_main_ls(&p224_main_row(&target, false, false, "unknown", "unknown"))
        .expect("parses");
    let empty_trace = P224Trace::default();
    assert_eq!(
        p224_classify(
            &target,
            Some(&absent),
            None,
            None,
            None,
            &empty_trace,
            false,
            &[],
            true
        ),
        P224Terminal::ReverseDeliveryPassed
    );
}

/// D5 — B received but did not answer with a still-answerable
/// post-send entry is an evidence contradiction; a post-send state
/// change re-derives the B terminal instead, and a missing post
/// snapshot is a gap.
#[test]
fn p224_b_received_but_not_answered_mapping() {
    let target = p224_test_target_hex();
    let client = "ef".repeat(32);
    let b_pre = p224_answerable_main(&target);
    let a_pre = p224_absent_client(&client, &target);
    let mut trace = p224_observable_trace(&target);
    trace.query_started = true;
    trace.query_to_b = true;
    trace.search_failed = true;
    trace.b_lookup_received = true;
    // Still answerable post-send: contradiction.
    assert_eq!(
        p224_classify(
            &target,
            Some(&b_pre),
            Some(&b_pre),
            Some(&a_pre),
            None,
            &trace,
            true,
            &[1, 21],
            false
        ),
        P224Terminal::EvidenceContradictionBAnswerableButNotAnswered
    );
    // Post-send RAP lost: re-derive from post state (transition, not
    // a handler-bug claim).
    let b_post_lost_rap =
        p224_parse_main_ls(&p224_main_row(&target, true, true, "false", "true")).expect("parses");
    assert_eq!(
        p224_classify(
            &target,
            Some(&b_pre),
            Some(&b_post_lost_rap),
            Some(&a_pre),
            None,
            &trace,
            true,
            &[1, 21],
            false
        ),
        P224Terminal::AttributionBMainLsNotQueryAnswerable
    );
    // Post-send missing: gap.
    assert_eq!(
        p224_classify(
            &target,
            Some(&b_pre),
            None,
            Some(&a_pre),
            None,
            &trace,
            true,
            &[1, 21],
            false
        ),
        P224Terminal::ObservabilityGapLookupPath
    );
}

/// A query that never started with status 21 has no authorized
/// terminal: the honest result is the gap, never an invented stage.
#[test]
fn p224_query_never_started_with_status21_is_gap() {
    let target = p224_test_target_hex();
    let client = "ef".repeat(32);
    let b_pre = p224_answerable_main(&target);
    let a_pre = p224_absent_client(&client, &target);
    let trace = p224_observable_trace(&target);
    assert_eq!(
        p224_classify(
            &target,
            Some(&b_pre),
            None,
            Some(&a_pre),
            None,
            &trace,
            true,
            &[1, 21],
            false
        ),
        P224Terminal::ObservabilityGapLookupPath
    );
}

/// A target-hash mismatch across snapshots is incomplete evidence,
/// never attribution (snapshots are never reused across attempts).
#[test]
fn p224_target_mismatch_yields_gap() {
    let target = p224_test_target_hex();
    let other = p224_test_b_hex();
    let b_pre = p224_answerable_main(&other);
    let trace = p224_observable_trace(&target);
    assert_eq!(
        p224_classify(
            &target,
            Some(&b_pre),
            None,
            None,
            None,
            &trace,
            true,
            &[1, 21],
            false
        ),
        P224Terminal::ObservabilityGapLookupPath
    );
}

/// Deep terminals require the pre-send helper client snapshot with a
/// proven client facade; without it the honest result is the gap.
#[test]
fn p224_client_pre_snapshot_required_for_deep_terminals() {
    let target = p224_test_target_hex();
    let b_pre = p224_answerable_main(&target);
    let mut trace = p224_observable_trace(&target);
    trace.b_published_ls_answered = true;
    // D6 shape but no client pre snapshot: gap, not attribution.
    assert_eq!(
        p224_classify(
            &target,
            Some(&b_pre),
            None,
            None,
            None,
            &trace,
            true,
            &[1, 21],
            false
        ),
        P224Terminal::ObservabilityGapLookupPath
    );
}

/// Snapshot and hash parsers reject malformed input (Unknown, never
/// a protocol fact).
#[test]
fn p224_parsers_reject_malformed() {
    assert!(p224_parse_main_ls("P224-ERROR invalid-hex-hash").is_none());
    assert!(p224_parse_main_ls("P220-EV kind=selector target_hash_hex=ab").is_none());
    assert!(p224_parse_main_ls("").is_none());
    assert!(
        p224_parse_main_ls("P224-EV kind=main-ls target_hash_hex=xyz observable=true").is_none()
    );
    assert!(p224_parse_client_ls("P224-ERROR invalid-hex-hash").is_none());
    assert!(p224_parse_client_ls("").is_none());
    assert!(
        p224_parse_hash_b64(
            "P224-EV kind=hash-b64 hash_hex=ab observable=false reason=render-failed",
            "ab"
        )
        .is_none()
    );
}

/// `P224-HASH-B64` accepts a well-formed rendering and rejects an
/// echoed-hex mismatch, a non-alphabet rendering, or an error line.
#[test]
fn p224_hash_b64_parser_accepts_valid_and_rejects_mismatch() {
    let hex = p224_test_target_hex();
    let b64 = p224_test_target_b64();
    let line = format!("P224-EV kind=hash-b64 hash_hex={hex} observable=true hash_b64={b64}");
    assert_eq!(
        p224_parse_hash_b64(&line, &hex).as_deref(),
        Some(b64.as_str())
    );
    // Echoed-hex mismatch: Unknown, never a guessed rendering.
    let other = p224_test_b_hex();
    assert!(p224_parse_hash_b64(&line, &other).is_none());
    // Non-I2P-alphabet rendering rejected (standard-alphabet `+`
    // and `/` are never valid I2P Base64, even without whitespace).
    let bad_alpha = format!(
        "P224-EV kind=hash-b64 hash_hex={hex} observable=true hash_b64=not+valid/base64*chars!!"
    );
    assert!(p224_parse_hash_b64(&bad_alpha, &hex).is_none());
    // Error line rejected.
    assert!(p224_parse_hash_b64("P224-ERROR invalid-hex-hash", &hex).is_none());
}

/// Plan 224 terminal tokens are canonical (§13 D1–D11): exactly one
/// terminal per attempt, no invented strings.
#[test]
fn p224_terminal_tokens_are_canonical() {
    assert_eq!(
        P224Terminal::AttributionBMainLsAbsent.token(),
        "P224-ATTRIBUTION-B-MAIN-LS-ABSENT"
    );
    assert_eq!(
        P224Terminal::AttributionBMainLsInvalidOrStale.token(),
        "P224-ATTRIBUTION-B-MAIN-LS-INVALID-OR-STALE"
    );
    assert_eq!(
        P224Terminal::AttributionBMainLsNotQueryAnswerable.token(),
        "P224-ATTRIBUTION-B-MAIN-LS-NOT-QUERY-ANSWERABLE"
    );
    assert_eq!(
        P224Terminal::AttributionANoInboundClientReplyTunnel.token(),
        "P224-ATTRIBUTION-A-NO-INBOUND-CLIENT-REPLY-TUNNEL"
    );
    assert_eq!(
        P224Terminal::AttributionALookupReplyCryptoUnavailable.token(),
        "P224-ATTRIBUTION-A-LOOKUP-REPLY-CRYPTO-UNAVAILABLE"
    );
    assert_eq!(
        P224Terminal::AttributionASearchExhaustedWithoutQueryingB.token(),
        "P224-ATTRIBUTION-A-SEARCH-EXHAUSTED-WITHOUT-QUERYING-B"
    );
    assert_eq!(
        P224Terminal::AttributionAToBLookupNotReceived.token(),
        "P224-ATTRIBUTION-A-TO-B-LOOKUP-NOT-RECEIVED"
    );
    assert_eq!(
        P224Terminal::EvidenceContradictionBAnswerableButNotAnswered.token(),
        "P224-EVIDENCE-CONTRADICTION-B-ANSWERABLE-BUT-NOT-ANSWERED"
    );
    assert_eq!(
        P224Terminal::AttributionBReplyNotObservedOnAClientTunnel.token(),
        "P224-ATTRIBUTION-B-REPLY-NOT-OBSERVED-ON-A-CLIENT-TUNNEL"
    );
    assert_eq!(
        P224Terminal::EvidenceContradictionClientTunnelDsmNotInClientDb.token(),
        "P224-EVIDENCE-CONTRADICTION-CLIENT-TUNNEL-DSM-NOT-IN-CLIENT-DB"
    );
    assert_eq!(
        P224Terminal::EvidenceContradictionNoLeasesetWithUsableClientLs.token(),
        "P224-EVIDENCE-CONTRADICTION-NO-LEASESET-WITH-USABLE-CLIENT-LS"
    );
    assert_eq!(P224Terminal::NextBoundary.token(), "P224-NEXT-BOUNDARY");
    assert_eq!(
        P224Terminal::ReverseDeliveryPassed.token(),
        "P224-REVERSE-DELIVERY-PASSED"
    );
    assert_eq!(
        P224Terminal::ObservabilityGapLookupPath.token(),
        "P224-OBSERVABILITY-GAP-LOOKUP-PATH"
    );
}

/// Every run emits exactly one `p224-classification` row, including
/// pre-epoch stops (gap, never attribution).
#[test]
fn p224_record_emits_exactly_one_classification() {
    let dir = p224_test_tmpdir("record-once");
    let evidence_dir = dir.join("evidence");
    std::fs::create_dir_all(&evidence_dir).expect("evidence dir");
    let token = record_p224_classification(
        &evidence_dir,
        P224Terminal::AttributionBMainLsAbsent,
        &p224_test_target_hex(),
        Some(false),
        &[1, 21],
        false,
        false,
        false,
        true,
    );
    assert_eq!(token, "P224-ATTRIBUTION-B-MAIN-LS-ABSENT");
    let tsv = std::fs::read_to_string(evidence_dir.join("driver-evidence.tsv")).expect("read tsv");
    assert_eq!(
        tsv.lines()
            .filter(|l| l.starts_with("p224-classification\t"))
            .count(),
        1
    );
    record_p224_early_stop_gap(&evidence_dir, "test-stop");
    let tsv = std::fs::read_to_string(evidence_dir.join("driver-evidence.tsv")).expect("read tsv");
    assert_eq!(
        tsv.lines()
            .filter(|l| l.starts_with("p224-classification\t"))
            .count(),
        2
    );
    assert!(tsv.contains("P224-OBSERVABILITY-GAP-LOOKUP-PATH"));
    let _ = std::fs::remove_dir_all(&dir);
}
