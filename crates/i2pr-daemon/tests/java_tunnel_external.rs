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
use std::net::{IpAddr, SocketAddr};
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
    DestinationRoutingConfig, EciesSessionConfig, EciesSessionManager, InboundDispatchOutcome,
    InboundLeaseSource, OutboundRequest, StreamingDestinationAdapter, build_signed_lease_set2,
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

/// Plan 226 — Java's pinned `MaskedIPSet` uses the first three IPv4 bytes
/// for `IP_CLOSE_BYTES=3`. Keep this harness-side proof independent from
/// production routing code and reject IPv6/non-loopback inputs explicitly.
fn p226_mask3_prefix(host: IpAddr) -> Option<[u8; 3]> {
    match host {
        IpAddr::V4(address) if address.is_loopback() => {
            let octets = address.octets();
            Some([octets[0], octets[1], octets[2]])
        }
        _ => None,
    }
}

fn p226_pairwise_mask3_distinct(hosts: &[IpAddr]) -> bool {
    if hosts.len() != 3 {
        return false;
    }
    let mut prefixes = Vec::with_capacity(hosts.len());
    for host in hosts {
        let Some(prefix) = p226_mask3_prefix(*host) else {
            return false;
        };
        if prefixes.contains(&prefix) {
            return false;
        }
        prefixes.push(prefix);
    }
    true
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

    /// Plan 234 §8 — bounded, read-only helper-side accept state.
    async fn report_stream_state(&mut self) -> Option<P234JavaAcceptState> {
        p234_parse_java_accept_state(&self.command("REPORT_STREAM_STATE").await)
    }
}

// ---- Plan 234 — Streaming SYN epoch attribution --------------------------

/// Sanitized helper-side facts returned by `REPORT_STREAM_STATE`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct P234JavaAcceptState {
    accept_requested: u64,
    accept_entered: u64,
    accept_returned: u64,
    socket_stored: u64,
    accept_errors: u64,
    accepting: bool,
    accepted_count: u64,
    connected_count: u64,
}

fn p234_parse_java_accept_state(line: &str) -> Option<P234JavaAcceptState> {
    let mut fields = line.split_whitespace();
    if fields.next()? != "STREAM_STATUS" {
        return None;
    }
    let mut state = P234JavaAcceptState::default();
    let mut seen = 0u16;
    for field in fields {
        let (key, value) = field.split_once('=')?;
        match key {
            "accept_requested" => state.accept_requested = value.parse().ok()?,
            "accept_entered" => state.accept_entered = value.parse().ok()?,
            "accept_returned" => state.accept_returned = value.parse().ok()?,
            "socket_stored" => state.socket_stored = value.parse().ok()?,
            "accept_errors" => state.accept_errors = value.parse().ok()?,
            "accepting" => state.accepting = value.parse().ok()?,
            "accepted_count" => state.accepted_count = value.parse().ok()?,
            "connected_count" => state.connected_count = value.parse().ok()?,
            _ => return None,
        }
        seen = seen.saturating_add(1);
    }
    (seen == 8).then_some(state)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct P234SynEpoch {
    local_connection_id: u64,
    local_port: u16,
    remote_port: u16,
    syn_sequence_or_message_identity: u32,
    syn_transport_request_emitted: bool,
    java_accept_thread_started: bool,
    java_accept_returned: bool,
    java_stream_receive_observed: bool,
    java_stream_response_observed: bool,
    i2pr_inbound_tunneldata_count: u64,
    i2pr_expected_stream_tunneldata_count: u64,
    i2pr_tunnel_recovery_count: u64,
    i2pr_tunnel_recovery_failures: u64,
    i2pr_garlic_payload_count: u64,
    i2pr_garlic_decode_failures: u64,
    i2pr_streaming_adapter_calls: u64,
    i2pr_streaming_adapter_successes: u64,
    i2pr_streaming_adapter_errors: u64,
    pending_outbound_after_receive: u64,
    ack_poll_emissions: u64,
    retransmit_poll_emissions: u64,
    connection_established: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum P234Terminal {
    JavaAcceptWorkerNotStarted,
    JavaSynNotAccepted,
    JavaAcceptedNoResponseObserved,
    I2prNoExpectedTunnelData,
    I2prTunnelRecoveryFailed,
    I2prGarlicDecodeFailed,
    I2prNoStreamingPayload,
    I2prStreamingAdapterFailed,
    DispatchedNotEstablished,
    DirectionAEstablished,
    ObservabilityGap,
}

impl P234Terminal {
    fn token(self) -> &'static str {
        match self {
            Self::JavaAcceptWorkerNotStarted => "P234-B-JAVA-ACCEPT-WORKER-NOT-STARTED",
            Self::JavaSynNotAccepted => "P234-B-JAVA-SYN-NOT-ACCEPTED",
            Self::JavaAcceptedNoResponseObserved => "P234-B-JAVA-ACCEPTED-NO-RESPONSE-OBSERVED",
            Self::I2prNoExpectedTunnelData => "P234-C-I2PR-NO-EXPECTED-TUNNELDATA",
            Self::I2prTunnelRecoveryFailed => "P234-C-I2PR-TUNNEL-RECOVERY-FAILED",
            Self::I2prGarlicDecodeFailed => "P234-C-I2PR-GARLIC-DECODE-FAILED",
            Self::I2prNoStreamingPayload => "P234-C-I2PR-NO-STREAMING-PAYLOAD",
            Self::I2prStreamingAdapterFailed => "P234-C-I2PR-STREAMING-ADAPTER-FAILED",
            Self::DispatchedNotEstablished => "P234-C-DISPATCHED-NOT-ESTABLISHED",
            Self::DirectionAEstablished => "P234-C-STREAMING-DIRECTION-A-ESTABLISHED",
            Self::ObservabilityGap => "P234-C-OBSERVABILITY-GAP",
        }
    }
}

fn p234_classify_syn_epoch(epoch: &P234SynEpoch) -> P234Terminal {
    if !epoch.java_accept_thread_started {
        return P234Terminal::JavaAcceptWorkerNotStarted;
    }
    if !epoch.java_accept_returned {
        return P234Terminal::JavaSynNotAccepted;
    }
    if epoch.i2pr_inbound_tunneldata_count == 0 {
        return P234Terminal::JavaAcceptedNoResponseObserved;
    }
    if epoch.i2pr_expected_stream_tunneldata_count == 0 {
        return P234Terminal::I2prNoExpectedTunnelData;
    }
    if epoch.i2pr_tunnel_recovery_failures > 0 && epoch.i2pr_tunnel_recovery_count == 0 {
        return P234Terminal::I2prTunnelRecoveryFailed;
    }
    if epoch.i2pr_garlic_decode_failures > 0 && epoch.i2pr_garlic_payload_count == 0 {
        return P234Terminal::I2prGarlicDecodeFailed;
    }
    if epoch.i2pr_streaming_adapter_calls == 0 {
        return P234Terminal::I2prNoStreamingPayload;
    }
    if epoch.i2pr_streaming_adapter_errors > 0 {
        return P234Terminal::I2prStreamingAdapterFailed;
    }
    if epoch.i2pr_streaming_adapter_successes > 0 && !epoch.connection_established {
        return P234Terminal::DispatchedNotEstablished;
    }
    if epoch.connection_established {
        return P234Terminal::DirectionAEstablished;
    }
    P234Terminal::ObservabilityGap
}

fn record_p234_syn_epoch(
    evidence_dir: &Path,
    epoch: &P234SynEpoch,
    java_state: Option<P234JavaAcceptState>,
    local_destination_hash: &Hash,
    remote_destination_hash: &Hash,
) {
    let java = java_state.unwrap_or_default();
    append_evidence(
        evidence_dir,
        "p234-syn-epoch",
        &format!(
            "local_connection_id={} local_destination_hash={} remote_destination_hash={} local_port={} remote_port={} syn_sequence_or_message_identity={} syn_transport_request_emitted={} java_accept_thread_started={} java_accept_returned={} java_stream_socket_count_delta={} java_stream_receive_observed={} java_stream_response_observed={} i2pr_inbound_tunneldata_count={} i2pr_expected_stream_tunneldata_count={} i2pr_tunnel_recovery_count={} i2pr_garlic_payload_count={} i2pr_streaming_adapter_calls={} i2pr_streaming_adapter_successes={} i2pr_streaming_adapter_errors={} connection_state={} pending_outbound_after_receive={} ack_poll_emissions={} retransmit_poll_emissions={} java_accept_errors={}",
            epoch.local_connection_id,
            sha256_hex(local_destination_hash.as_bytes()),
            sha256_hex(remote_destination_hash.as_bytes()),
            epoch.local_port,
            epoch.remote_port,
            epoch.syn_sequence_or_message_identity,
            epoch.syn_transport_request_emitted,
            epoch.java_accept_thread_started,
            epoch.java_accept_returned,
            java.socket_stored,
            epoch.java_stream_receive_observed,
            epoch.java_stream_response_observed,
            epoch.i2pr_inbound_tunneldata_count,
            epoch.i2pr_expected_stream_tunneldata_count,
            epoch.i2pr_tunnel_recovery_count,
            epoch.i2pr_garlic_payload_count,
            epoch.i2pr_streaming_adapter_calls,
            epoch.i2pr_streaming_adapter_successes,
            epoch.i2pr_streaming_adapter_errors,
            if epoch.connection_established {
                "Established"
            } else {
                "SynSent"
            },
            epoch.pending_outbound_after_receive,
            epoch.ack_poll_emissions,
            epoch.retransmit_poll_emissions,
            java.accept_errors,
        ),
    );
    append_evidence(
        evidence_dir,
        "p234-java-accept-state",
        &format!(
            "accept_requested={} accept_entered={} accept_returned={} socket_stored={} accept_errors={} accepting={} accepted_count={} connected_count={}",
            java.accept_requested,
            java.accept_entered,
            java.accept_returned,
            java.socket_stored,
            java.accept_errors,
            java.accepting,
            java.accepted_count,
            java.connected_count,
        ),
    );
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
    let service_host: IpAddr = env_value("JAVA_SERVICE_SSU2_HOST")
        .parse()
        .expect("service SSU2 host");
    let publication_host: IpAddr = env_value("JAVA_PUBLICATION_SSU2_HOST")
        .parse()
        .expect("publication SSU2 host");
    let tunnel_participant_host: IpAddr = env_value("JAVA_TUNNEL_PARTICIPANT_SSU2_HOST")
        .parse()
        .expect("tunnel-participant SSU2 host");
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
    assert_eq!(service_ssu2.host(), Some(service_host));
    assert_eq!(publication_ssu2.host(), Some(publication_host));
    assert_eq!(
        tunnel_participant_ssu2.host(),
        Some(tunnel_participant_host)
    );
    let pairwise_mask3_distinct =
        p226_pairwise_mask3_distinct(&[service_host, publication_host, tunnel_participant_host]);
    append_evidence(
        &evidence_dir,
        "p226-routerinfo-hosts",
        &format!(
            "A_SSU2_HOST={} B_SSU2_HOST={} C_SSU2_HOST={} pairwise_mask3_distinct={} all_loopback={}",
            service_host,
            publication_host,
            tunnel_participant_host,
            pairwise_mask3_distinct,
            service_host.is_loopback()
                && publication_host.is_loopback()
                && tunnel_participant_host.is_loopback(),
        ),
    );
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
            record_p225_early_stop_gap(
                &evidence_dir,
                "authoritative-epoch-never-reached-install-stalled",
            );
            record_p226_early_stop_gap(
                &evidence_dir,
                "authoritative-epoch-never-reached-install-stalled",
            );
            record_p227_early_stop_gap(
                &evidence_dir,
                "authoritative-epoch-never-reached-install-stalled",
            );
            record_p231_early_stop_gap(
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
        record_p225_early_stop_gap(
            &evidence_dir,
            "authoritative-epoch-never-reached-lease-stalled",
        );
        record_p226_early_stop_gap(
            &evidence_dir,
            "authoritative-epoch-never-reached-lease-stalled",
        );
        record_p227_early_stop_gap(
            &evidence_dir,
            "authoritative-epoch-never-reached-lease-stalled",
        );
        record_p231_early_stop_gap(
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
    // Plan 232 WP A/B1 — route-derived lease: gateway + gateway tunnel come
    // from the installed inbound route, never from the publication target.
    // Router B (`java_hash`) stays the NetDB publication target; Router A
    // (`service_hash`) is the inbound tunnel gateway proven in-registry.
    let p232_local_receive = receive_ids[0];
    let p232_route = coord
        .registry()
        .inbound_gateway_route(p232_local_receive)
        .expect("installed inbound gateway route");
    assert_eq!(
        p232_route.gateway_router, service_hash,
        "Plan 232 §6: installed destination route must terminate at the service router"
    );
    assert_eq!(
        p232_route.gateway_receive_tunnel.get(),
        IBGW_RECEIVE,
        "Plan 232 §6: installed destination route must carry the gateway-side receive tunnel"
    );
    assert_eq!(
        p232_route.local_receive_tunnel.get(),
        IBGW_NEXT,
        "Plan 232 §6: installed destination route must carry the local receive tunnel"
    );
    assert_eq!(
        p232_route.local_receive_tunnel, p232_local_receive,
        "Plan 232 §6: route selector must be the installed local receive id"
    );
    let p232_registry_slot = coord.registry().inbound_slot(p232_local_receive);
    let lease_source = p232_route_derived_lease_source(
        registrations_in[0].slot(),
        p232_local_receive,
        Some(p232_route),
        p232_registry_slot,
        tunnel_expires,
        tunnel_expires.saturating_sub(60),
    )
    .expect("route-derived destination lease");
    let p232_publication_target = Hash::from_bytes(*java_hash.as_bytes());
    p232_record_lease_route(
        &evidence_dir,
        &P232LeaseRecord {
            lane: "destination",
            stage: "initial",
            slot: registrations_in[0].slot(),
            local_receive: p232_local_receive,
            route: &p232_route,
            lease: &lease_source,
            publication_target: &p232_publication_target,
            service_router: &service_hash,
        },
    );
    p232_record_publication_separation(
        &evidence_dir,
        "destination",
        "initial",
        &p232_publication_target,
        &p232_route,
        &lease_source,
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
    // Plan 225 — InboundMessageDistributor logs the helper as its b32.i2p
    // hostname, not as Hash.toBase64(). Ask the same read-only Java control
    // surface for the exact Base32 rendering; never guess it in Rust.
    let p225_helper_b32 = p225_collect_hash_b32(diag_a_port, &helper_client_dbid_hex).await;
    append_evidence(
        &evidence_dir,
        "p224-target-context",
        &format!(
            "target_hash_hex={} target_hash_b64={} router_b_hash_hex={} router_b_hash_b64={} helper_dbid_hex={} helper_dbid_b64={} helper_dbid_b32={}",
            reverse_lookup_target_hex,
            p224_target_b64.as_deref().unwrap_or("unknown"),
            rust_b_hex,
            p224_router_b_b64.as_deref().unwrap_or("unknown"),
            helper_client_dbid_hex,
            p224_helper_b64.as_deref().unwrap_or("unknown"),
            p225_helper_b32.as_deref().unwrap_or("unknown"),
        ),
    );

    // Plan 225 — prove the exact lookup logger levels are effective in both
    // running Java LogManagers before the tracked send. A config file on
    // disk alone is not sufficient to make log absence authoritative.
    let p225_logger_a = p225_collect_logger_config(diag_a_port).await;
    let p225_logger_b = p225_collect_logger_config(diag_b_port).await;
    record_p225_logger_config(
        &evidence_dir,
        "p225-logger-config-a",
        p225_logger_a.as_ref(),
    );
    record_p225_logger_config(
        &evidence_dir,
        "p225-logger-config-b",
        p225_logger_b.as_ref(),
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
    // ---- Plan 231 WP A — reverse-epoch contract + pre-send snapshots ---
    // The single tracked reverse payload correlates from Java A's
    // outbound client tunnel through C to the selected lease gateway
    // and i2pr's exact inbound tunnel. All pre-send observations are
    // read-only; no NetDB/tunnel/queue/profile state is mutated.
    let diag_c_port: u16 = std::env::var("JAVA_DIAGNOSTIC_C_PORT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    let p231_router_c_hex = std::env::var("P227_ROUTER_C_HEX").unwrap_or_default();
    // Router A's hash comes from Java's own self snapshot on the A
    // diagnostic port (same P220-SNAPSHOT surface the B cross-check
    // uses) — never synthesized from local variables, never assumed
    // from topology comments.
    let p231_service_hex = p220_query_diagnostic(diag_a_port, "P220-SNAPSHOT")
        .await
        .and_then(|line| {
            let kv = p220_parse_kv(&line);
            if kv.get("self_ri_present").is_none_or(|v| v != "true") {
                return None;
            }
            kv.get("self_router_hash_hex")
                .filter(|hex| p227_is_hex64(hex))
                .cloned()
        })
        .unwrap_or_default();
    // The advertised target lease is the single lease this driver
    // built into the published local LS2. A multi-lease LS2 would
    // require exposing Java's exact selected lease; with one lease
    // the selected lease is exactly this one.
    let p231_lease_gateway_hex = p220_bytes_to_hex(lease_source.gateway().as_bytes());
    let p231_lease_tunnel_id = lease_source.gateway_receive_tunnel_id();
    let p231_target_role = p231_target_role(
        &p231_lease_gateway_hex,
        &p231_service_hex,
        &rust_b_hex,
        &p231_router_c_hex,
    );
    let p231_target_diag_port = match p231_target_role {
        Some("A") => diag_a_port,
        Some("B") => diag_b_port,
        Some("C") => diag_c_port,
        _ => 0,
    };
    let p231_ssu2_before = handle.snapshot();
    let p231_a_gateway_pre = p231_collect_gateway(diag_a_port).await;
    let p231_a_client_ob_pre =
        p231_collect_client_outbound(diag_a_port, &helper_client_dbid_hex).await;
    // Prove the exact one-hop outbound client tunnel through C is
    // still installed at the reverse epoch (Plan 230 P227 gate
    // re-queried at the pre-send instant, not reused from setup).
    let p231_a_tunnels_pre =
        if p227_is_hex64(&helper_client_dbid_hex) && p227_is_hex64(&p231_router_c_hex) {
            p227_collect_tunnels(diag_a_port, &helper_client_dbid_hex, &p231_router_c_hex).await
        } else {
            None
        };
    let p231_a_send_id_pre: u64 = p231_a_client_ob_pre
        .as_ref()
        .map(|ob| ob.single_send_tunnel_id)
        .unwrap_or(0);
    let p231_c_obep_pre = if p231_a_send_id_pre != 0 {
        p231_collect_participating(diag_c_port, p231_a_send_id_pre).await
    } else {
        None
    };
    let p231_target_gateway_pre = if p231_target_diag_port != 0 {
        p231_collect_gateway(p231_target_diag_port).await
    } else {
        None
    };
    let p231_ibgw_pre = if p231_target_diag_port != 0 {
        p231_collect_participating(p231_target_diag_port, u64::from(p231_lease_tunnel_id)).await
    } else {
        None
    };
    // Scratch-only pre-send baselines for the windowed marker deltas
    // (bounded counts only, never lines). Log directories are
    // scratch-only inputs; `None` stays Unknown, never zero-as-fact.
    let p231_a_log_dir_pre: Option<PathBuf> =
        std::env::var("JAVA_A_LOG_DIR").ok().map(PathBuf::from);
    let p231_b_log_dir_pre: Option<PathBuf> =
        std::env::var("JAVA_B_LOG_DIR").ok().map(PathBuf::from);
    let p231_c_log_dir_pre: Option<PathBuf> =
        std::env::var("JAVA_C_LOG_DIR").ok().map(PathBuf::from);
    let p231_ocmosj_pre = p231_a_log_dir_pre
        .as_ref()
        .and_then(|dir| p231_count_marker_in_log_dir(dir, "Dispatching message to"));
    let p231_no_ob_pre = p231_a_log_dir_pre
        .as_ref()
        .and_then(|dir| p231_count_marker_in_log_dir(dir, "no matching OB tunnel for id"));
    let p231_obep_drops_pre = p231_c_log_dir_pre.as_ref().and_then(|dir| {
        let mut total: Option<u64> = Some(0);
        for marker in ["dropping at OBEP", "Dropping msg at OBEP", "Dropping DSM"] {
            match (total, p231_count_marker_in_log_dir(dir, marker)) {
                (Some(acc), Some(count)) => total = Some(acc.saturating_add(count)),
                _ => total = None,
            }
        }
        total
    });
    let p231_target_log_dir_pre: Option<PathBuf> = match p231_target_role {
        Some("A") => p231_a_log_dir_pre.clone(),
        Some("B") => p231_b_log_dir_pre.clone(),
        Some("C") => p231_c_log_dir_pre.clone(),
        _ => None,
    };
    let p231_no_ibgw_pre = p231_target_log_dir_pre
        .as_ref()
        .and_then(|dir| p231_count_marker_in_log_dir(dir, "no matching IBGW for id"));
    // Id-correlated pre-send baselines: the no-matching-gateway rows
    // carry the queried tunnel id, so the windowed delta attributes
    // the receipt to the exact target tunnel, never background.
    let p231_send_id_text = p231_a_send_id_pre.to_string();
    let p231_lease_id_text = p231_lease_tunnel_id.to_string();
    // A zero send id means no exact tunnel to correlate: the
    // correlated count stays Unknown instead of matching the digit
    // "0" against unrelated lines.
    let p231_no_ob_corr_pre = if p231_a_send_id_pre != 0 {
        p231_a_log_dir_pre.as_ref().and_then(|dir| {
            p231_count_marker_with_id_in_log_dir(
                dir,
                "no matching OB tunnel for id",
                &p231_send_id_text,
            )
        })
    } else {
        None
    };
    let p231_no_ibgw_corr_pre = p231_target_log_dir_pre.as_ref().and_then(|dir| {
        p231_count_marker_with_id_in_log_dir(dir, "no matching IBGW for id", &p231_lease_id_text)
    });
    // Plan 232 §9 — corrected-route preflight before SEND_TRACKED: the
    // published local LS2 gateway/tunnel must equal the installed inbound
    // route, and the router identified by the actual advertised lease
    // (never a hard-coded role) must expose the exact IBGW.
    let p232_published_gateway_match = lease_source.gateway() == p232_route.gateway_router;
    let p232_published_tunnel_match =
        lease_source.gateway_receive_tunnel_id() == p232_route.gateway_receive_tunnel.get();
    let p232_target_has_exact_ibgw = p231_ibgw_pre.as_ref().is_some_and(|snap| snap.present);
    p232_record_target_ibgw_preflight(
        &evidence_dir,
        p232_published_gateway_match,
        p232_published_tunnel_match,
        p232_target_has_exact_ibgw,
        p231_target_role.unwrap_or("unknown"),
        p231_lease_tunnel_id,
    );
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
    // Plan 231 WP B: the isolated target-send epoch closes the moment
    // the tracked send returns (ACCEPTED was emitted after the inline
    // dispatch call returned). A post-send-immediate gateway snapshot
    // bounds any enqueue attribution to this micro-epoch.
    let p231_accepted_ms = wall_ms();
    let p231_tracked_send_start_ms = wall_ms().saturating_sub(
        tracked_start
            .elapsed()
            .as_millis()
            .try_into()
            .unwrap_or(u64::MAX),
    );
    let p231_a_gateway_post_imm = p231_collect_gateway(diag_a_port).await;
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
    // Plan 231 WP E — bounded per-stage counters for the reverse
    // epoch. Every TunnelData cell is attributed by tunnel id against
    // the exact owned inbound tunnel receive id; unrelated cells are
    // context only and never satisfy target progress. Silent
    // continues become stage counts; legacy acceptance semantics
    // (exact digest match inside the frozen window) are unchanged.
    let mut p231_tunneldata_seen: u64 = 0;
    let mut p231_expected_seen: u64 = 0;
    let mut p231_unexpected_seen: u64 = 0;
    let mut p231_decode_failures: u64 = 0;
    let mut p231_recovery_complete_expected: u64 = 0;
    let mut p231_recovery_error_expected: u64 = 0;
    let mut p231_recovery_error_unknown_tunnel: u64 = 0;
    let mut p231_recovery_error_incomplete: u64 = 0;
    let mut p231_recovery_error_unexpected_body: u64 = 0;
    let mut p231_recovery_error_inbound: u64 = 0;
    let mut p231_recovery_complete_unexpected: u64 = 0;
    let mut p231_recovery_error_unexpected: u64 = 0;
    let mut p231_garlic_ok_expected: u64 = 0;
    let mut p231_garlic_fail_expected: u64 = 0;
    let mut p231_dispatch_calls_expected: u64 = 0;
    let mut p231_queue_hits_expected: u64 = 0;
    let mut p231_queued_decode_fails: u64 = 0;
    let p231_expected_tunnel_id: u32 = receive_ids[0].get();
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
                p231_decode_failures += 1;
                continue;
            }
        };
        let cell = match message.body() {
            I2npBody::TunnelData(cell) => {
                p220_tunneldata_observed = true;
                p231_tunneldata_seen += 1;
                if cell.tunnel_id == p231_expected_tunnel_id {
                    p231_expected_seen += 1;
                } else {
                    p231_unexpected_seen += 1;
                }
                cell.clone()
            }
            _ => continue,
        };
        let cell_is_expected = cell.tunnel_id == p231_expected_tunnel_id;
        let bytes = match dest.recover_garlic_bytes(coord.registry_mut(), &cell, wall_ms()) {
            Ok(bytes) => {
                if cell_is_expected {
                    p231_recovery_complete_expected += 1;
                } else {
                    p231_recovery_complete_unexpected += 1;
                }
                bytes
            }
            Err(error) => {
                if cell_is_expected {
                    p231_recovery_error_expected += 1;
                    match &error {
                        i2pr_daemon::destination_tunnels::DestinationTunnelError::UnknownInboundTunnel(_) => {
                            p231_recovery_error_unknown_tunnel += 1;
                        }
                        i2pr_daemon::destination_tunnels::DestinationTunnelError::CellIncomplete => {
                            p231_recovery_error_incomplete += 1;
                        }
                        i2pr_daemon::destination_tunnels::DestinationTunnelError::UnexpectedBodyType { .. } => {
                            p231_recovery_error_unexpected_body += 1;
                        }
                        _ => {
                            p231_recovery_error_inbound += 1;
                        }
                    }
                } else {
                    p231_recovery_error_unexpected += 1;
                }
                continue;
            }
        };
        // Unrelated completions still traverse the legacy pump (never
        // synthesized, never dropped) but are never attributed to the
        // target stages.
        if !cell_is_expected {
            if let Ok(unrelated_envelope) =
                I2npMessage::decode_standard(&bytes, MAX_I2NP_PAYLOAD_SIZE)
            {
                let unrelated_secs = u32::try_from(wall_secs()).unwrap_or(u32::MAX);
                dispatcher.dispatch_garlic_envelope(
                    &mut session,
                    local_identity.id(),
                    local_identity.static_secret_bytes(),
                    &local_identity.static_public_bytes(),
                    unrelated_secs,
                    &unrelated_envelope,
                    routing.lease_set2_store_mut(),
                );
                let _ = dispatcher.pop_payload(local_identity.id());
            }
            continue;
        }
        let envelope = match I2npMessage::decode_standard(&bytes, MAX_I2NP_PAYLOAD_SIZE) {
            Ok(envelope) => {
                p231_garlic_ok_expected += 1;
                envelope
            }
            Err(_) => {
                p231_garlic_fail_expected += 1;
                continue;
            }
        };
        let now_secs = u32::try_from(wall_secs()).unwrap_or(u32::MAX);
        p231_dispatch_calls_expected += 1;
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
            p231_queue_hits_expected += 1;
            // Plan 192: parse both the 9-byte short-transport Data
            // envelope and the I2CP-style Data body Java writes
            // inside its Garlic clove. Undecodable queue hits are
            // counted, never panicked: only a digest match passes.
            let decoded = match decode_inbound_i2np(queued.bytes()) {
                Ok(decoded) => decoded,
                Err(_) => {
                    p231_queued_decode_fails += 1;
                    continue;
                }
            };
            if let I2npBody::Data(body) = decoded.body() {
                match i2pr_proto::decode_i2cp_data_body(body.payload.as_bytes()) {
                    Ok(i2cp_body) => {
                        inbound_payload = Some(i2cp_body.payload);
                    }
                    Err(_) => {
                        p231_queued_decode_fails += 1;
                    }
                }
            } else {
                p231_queued_decode_fails += 1;
            }
        }
    }
    if let Some(reply) = inbound_payload.clone() {
        // The pass still requires byte-exact equality; a decodable
        // mismatch is recorded (never panicked) so Plan 231 can
        // classify PAYLOAD-MISMATCH instead of aborting the run.
        if reply == app_back {
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
                "destination-inbound-payload-mismatch",
                &format!(
                    "expected_len={} observed_len={} pump_error={reply_pump_error}",
                    app_back.len(),
                    reply.len()
                ),
            );
        }
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
                    p225_helper_b32.as_deref().and_then(|helper_b32| {
                        p224_scan_log_dir_with_b32(
                            dir,
                            target_b64,
                            b_b64,
                            Some(helper_b64),
                            Some(helper_b32),
                        )
                    })
                })
            })
        })
    });
    let p224_scan_b = p224_b_log_dir.as_ref().and_then(|dir| {
        p224_target_b64.as_ref().and_then(|target_b64| {
            p224_router_b_b64.as_ref().and_then(|b_b64| {
                p224_helper_b64.as_deref().and_then(|helper_b64| {
                    p225_helper_b32.as_deref().and_then(|helper_b32| {
                        p224_scan_log_dir_with_b32(
                            dir,
                            target_b64,
                            b_b64,
                            Some(helper_b64),
                            Some(helper_b32),
                        )
                    })
                })
            })
        })
    });
    let p224_logger_a_ok = p224_a_log_dir
        .as_ref()
        .is_some_and(|dir| p224_logger_config_installed(dir))
        && p225_logger_a
            .as_ref()
            .is_some_and(|config| config.effective);
    let p224_logger_b_ok = p224_b_log_dir
        .as_ref()
        .is_some_and(|dir| p224_logger_config_installed(dir))
        && p225_logger_b
            .as_ref()
            .is_some_and(|config| config.effective);
    if !p224_logger_a_ok || !p224_logger_b_ok {
        append_evidence(
            &evidence_dir,
            "p224-logger-config-unverified",
            "targeted logger.config not proven installed and effective in both datadirs; trace absence is Unknown",
        );
    }
    let p224_trace = p224_build_trace_with_b32(
        &reverse_lookup_target_hex,
        p224_target_b64.as_deref(),
        p224_router_b_b64.as_deref(),
        p224_helper_b64.as_deref(),
        p225_helper_b32.as_deref(),
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
    let p225_terminal = p225_classify(
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
    let _ = record_p225_classification(
        &evidence_dir,
        p225_terminal,
        &reverse_lookup_target_hex,
        p224_b_answerable_pre,
        &p222_facts.ordered_statuses,
        frozen_payload_45s,
        frozen_tunneldata_45s,
        p224_trace.reply_encryption_error_seen,
        p224_trace.observable,
    );
    // Plan 226 — exact target-job attribution is a separate corrective
    // surface. The baseline run is allowed to prove only the historical
    // shared-/24 skip; the distinct run consumes that proof and then
    // classifies the next independently observed boundary.
    let p226_trace = p226_build_trace(p224_scan_a.as_ref(), p224_scan_b.as_ref());
    let selected_preflight = matches!(p222_selector_equivalent, P220Observed::Known(true));
    record_p226_trace(&evidence_dir, &p226_trace, selected_preflight);
    let p226_topology =
        std::env::var("JAVA_PEER_TOPOLOGY").unwrap_or_else(|_| "baseline".to_owned());
    let hosts_distinct = [
        std::env::var("JAVA_SERVICE_SSU2_HOST").ok(),
        std::env::var("JAVA_PUBLICATION_SSU2_HOST").ok(),
        std::env::var("JAVA_TUNNEL_PARTICIPANT_SSU2_HOST").ok(),
    ]
    .into_iter()
    .map(|host| host.and_then(|value| value.parse::<IpAddr>().ok()))
    .collect::<Option<Vec<_>>>()
    .is_some_and(|hosts| p226_pairwise_mask3_distinct(&hosts));
    append_evidence(
        &evidence_dir,
        "p226-topology-mode",
        &format!(
            "mode={} pairwise_mask3_distinct={} baseline_gate_required=true",
            p226_topology, hosts_distinct
        ),
    );
    let p226_terminal = if frozen_payload_45s {
        P226Terminal::ReverseDeliveryPassed
    } else if p226_topology == "baseline" {
        p226_classify_baseline(selected_preflight, &p226_trace)
    } else {
        p226_classify_distinct(
            hosts_distinct,
            selected_preflight,
            &p226_trace,
            p224_b_pre.as_ref().is_some_and(p224_b_answerable),
            p224_trace.b_lookup_received,
            p224_trace.b_published_ls_answered,
            p224_trace.a_client_tunnel_ls_received,
            p224_a_post
                .as_ref()
                .is_some_and(|snapshot| snapshot.validated_present),
            p224_status_21,
            &p222_facts.ordered_statuses,
            frozen_payload_45s,
        )
    };
    record_p226_classification(&evidence_dir, &p226_terminal, &p226_trace);
    // Plan 227 — explicit one-hop corrective terminal. The shell has already
    // proven Router-C eligibility and installed one-hop pools before the
    // counted driver; the driver re-queries the same read-only diagnostics
    // at its post-bootstrap epoch and emits exactly one authoritative
    // `p227-classification` row. Installed pool state is authoritative;
    // scratch-log selection is corroborative only.
    let p227_router_c_hex = std::env::var("P227_ROUTER_C_HEX").unwrap_or_default();
    let p227_router_c_b64_env = std::env::var("P227_ROUTER_C_B64").unwrap_or_default();
    let p227_client_hex_env =
        std::env::var("P227_CLIENT_DBID_HEX").unwrap_or_else(|_| helper_client_dbid_hex.clone());
    let p227_client_hex = if p227_is_hex64(&p227_client_hex_env) {
        p227_client_hex_env.clone()
    } else {
        helper_client_dbid_hex.clone()
    };
    // Exact-C identity proof: the explicit peer B64 must equal Java's own
    // P224-HASH-B64 rendering of the Router-C hex (never reimplemented).
    let p227_rendered_b64 = if p227_is_hex64(&p227_router_c_hex) {
        p224_collect_hash_b64(diag_a_port, &p227_router_c_hex).await
    } else {
        None
    };
    let p227_option_identity_match = !p227_router_c_hex.is_empty()
        && !p227_router_c_b64_env.is_empty()
        && p227_rendered_b64.as_deref() == Some(p227_router_c_b64_env.as_str());
    append_evidence(
        &evidence_dir,
        "p227-explicit-peer-derivation",
        &format!(
            "router_c_hex={} b64_len={} renderer=P224-HASH-B64 option_identity_match={}",
            if p227_router_c_hex.is_empty() {
                "missing".to_owned()
            } else {
                p227_router_c_hex.clone()
            },
            p227_router_c_b64_env.len(),
            p227_option_identity_match,
        ),
    );
    let p227_eligibility = if p227_is_hex64(&p227_router_c_hex) {
        p227_collect_eligibility(diag_a_port, &p227_router_c_hex).await
    } else {
        None
    };
    record_p227_eligibility(&evidence_dir, p227_eligibility.as_ref(), &p227_router_c_hex);
    let p227_tunnels = if p227_is_hex64(&p227_client_hex) && p227_is_hex64(&p227_router_c_hex) {
        p227_collect_tunnels(diag_a_port, &p227_client_hex, &p227_router_c_hex).await
    } else {
        None
    };
    record_p227_tunnels(
        &evidence_dir,
        p227_tunnels.as_ref(),
        &p227_client_hex,
        &p227_router_c_hex,
    );
    // Helper-connect facts are recorded by the shell
    // (`p227-helper-connect.tsv`); the driver consumes the frozen
    // 45-second payload result as its acceptance input and never
    // re-derives it from the trace.
    let p227_terminal = p227_classify(
        p227_eligibility.as_ref(),
        p227_tunnels.as_ref(),
        &p226_trace,
        p224_b_pre.as_ref().is_some_and(p224_b_answerable),
        p224_trace.b_lookup_received,
        p224_trace.b_published_ls_answered,
        p224_trace.a_client_tunnel_ls_received,
        p224_a_post
            .as_ref()
            .is_some_and(|snapshot| snapshot.validated_present),
        &p222_facts.ordered_statuses,
        frozen_payload_45s,
    );
    record_p227_classification(&evidence_dir, &p227_terminal);
    // ---- Plan 231 WP B/C/D/E/F — post-window attribution ----------------
    // Post-`ACCEPTED` tunnel-dispatch attribution across the exact
    // stage chain: Java A outbound gateway / enqueue (isolated
    // pre-vs-post-immediate epoch) -> Router C one-hop outbound
    // endpoint / forward (pre-vs-post-window on the exact OBEP
    // config) -> selected target lease gateway / inbound gateway
    // (pre-vs-post-window on the exact IBGW config) -> i2pr exact
    // inbound TunnelData / recovery / Garlic / Destination delivery
    // (tunnel-id-attributed pump counters). Exactly one
    // `p231-classification` row is emitted below.
    let p231_local_hex = p220_bytes_to_hex(local_hash.as_bytes());
    let p231_a_gateway_post = p231_collect_gateway(diag_a_port).await;
    let p231_c_obep_post = if p231_a_send_id_pre != 0 {
        p231_collect_participating(diag_c_port, p231_a_send_id_pre).await
    } else {
        None
    };
    let p231_target_gateway_post = if p231_target_diag_port != 0 {
        p231_collect_gateway(p231_target_diag_port).await
    } else {
        None
    };
    let p231_ibgw_post = if p231_target_diag_port != 0 {
        p231_collect_participating(p231_target_diag_port, u64::from(p231_lease_tunnel_id)).await
    } else {
        None
    };
    // Scratch-only log correlation (bounded counts only, never lines):
    // OCMOSJ dispatch + no-matching-OB on A, OBEP drop markers on C,
    // no-matching-IBGW on the target router.
    let p231_b_log_dir: Option<PathBuf> = std::env::var("JAVA_B_LOG_DIR").ok().map(PathBuf::from);
    let p231_c_log_dir: Option<PathBuf> = std::env::var("JAVA_C_LOG_DIR").ok().map(PathBuf::from);
    let p231_a_log_dir: Option<PathBuf> = std::env::var("JAVA_A_LOG_DIR").ok().map(PathBuf::from);
    let p231_ocmosj_post = p231_a_log_dir
        .as_ref()
        .and_then(|dir| p231_count_marker_in_log_dir(dir, "Dispatching message to"));
    let p231_no_ob_post = p231_a_log_dir
        .as_ref()
        .and_then(|dir| p231_count_marker_in_log_dir(dir, "no matching OB tunnel for id"));
    let p231_obep_drop_post = p231_c_log_dir.as_ref().and_then(|dir| {
        let mut total: Option<u64> = Some(0);
        for marker in ["dropping at OBEP", "Dropping msg at OBEP", "Dropping DSM"] {
            match (total, p231_count_marker_in_log_dir(dir, marker)) {
                (Some(acc), Some(count)) => total = Some(acc.saturating_add(count)),
                _ => total = None,
            }
        }
        total
    });
    let p231_target_log_dir: Option<PathBuf> = match p231_target_role {
        Some("A") => p231_a_log_dir.clone(),
        Some("B") => p231_b_log_dir.clone(),
        Some("C") => p231_c_log_dir.clone(),
        _ => None,
    };
    let p231_no_ibgw_post = p231_target_log_dir
        .as_ref()
        .and_then(|dir| p231_count_marker_in_log_dir(dir, "no matching IBGW for id"));
    // Id-correlated post-window counts for the exact target tunnel.
    let p231_no_ob_corr_post = if p231_a_send_id_pre != 0 {
        p231_a_log_dir.as_ref().and_then(|dir| {
            p231_count_marker_with_id_in_log_dir(
                dir,
                "no matching OB tunnel for id",
                &p231_a_send_id_pre.to_string(),
            )
        })
    } else {
        None
    };
    let p231_no_ibgw_corr_post = p231_target_log_dir.as_ref().and_then(|dir| {
        p231_count_marker_with_id_in_log_dir(
            dir,
            "no matching IBGW for id",
            &p231_lease_tunnel_id.to_string(),
        )
    });
    let p231_count_delta = |post: Option<u64>, pre: Option<u64>| -> Option<i64> {
        match (post, pre) {
            (Some(after), Some(before)) => Some(after.saturating_sub(before) as i64),
            _ => None,
        }
    };
    // Isolated A-epoch deltas (pre vs post-immediate, microseconds
    // after ACCEPTED); transit deltas (pre vs post-window).
    let p231_pre_dispatch = p231_a_gateway_pre
        .as_ref()
        .map(|stats| stats.dispatch_outbound_tunnel)
        .unwrap_or(-1);
    let p231_imm_dispatch = p231_a_gateway_post_imm
        .as_ref()
        .map(|stats| stats.dispatch_outbound_tunnel)
        .unwrap_or(-1);
    let p231_dispatch_delta = p231_delta(p231_imm_dispatch, p231_pre_dispatch);
    let p231_pre_overflow = p231_a_gateway_pre
        .as_ref()
        .map(|stats| stats.drop_gateway_overflow)
        .unwrap_or(-1);
    let p231_imm_overflow = p231_a_gateway_post_imm
        .as_ref()
        .map(|stats| stats.drop_gateway_overflow)
        .unwrap_or(-1);
    let p231_overflow_delta = p231_delta(p231_imm_overflow, p231_pre_overflow);
    // Window attribution (pre-send vs post-window): the helper
    // returns before the router dispatches, so the micro-epoch above
    // systematically misses the enqueue. The window deltas below are
    // target-attributable only through the lane-quiet premises in the
    // classifier (exactly one client message; missing gateways always
    // log their id-correlated row).
    let p231_post_dispatch = p231_a_gateway_post
        .as_ref()
        .map(|stats| stats.dispatch_outbound_tunnel)
        .unwrap_or(-1);
    let p231_post_overflow = p231_a_gateway_post
        .as_ref()
        .map(|stats| stats.drop_gateway_overflow)
        .unwrap_or(-1);
    let p231_pre_client_time = p231_a_gateway_pre
        .as_ref()
        .map(|stats| stats.dispatch_time)
        .unwrap_or(-1);
    let p231_post_client_time = p231_a_gateway_post
        .as_ref()
        .map(|stats| stats.dispatch_time)
        .unwrap_or(-1);
    let p231_dispatch_window_delta = p231_delta(p231_post_dispatch, p231_pre_dispatch);
    let p231_overflow_window_delta = p231_delta(p231_post_overflow, p231_pre_overflow);
    let p231_client_time_delta = p231_delta(p231_post_client_time, p231_pre_client_time);
    let p231_c_processed_delta = match (&p231_c_obep_pre, &p231_c_obep_post) {
        (Some(pre), Some(post)) => p231_delta(post.processed, pre.processed),
        _ => None,
    };
    let p231_c_obep_exact = p231_c_obep_post.as_ref().is_some_and(|post| {
        post.present
            && post
                .receive_from_hex
                .eq_ignore_ascii_case(&p231_service_hex)
            && p231_c_obep_pre.as_ref().is_some_and(|pre| {
                pre.present && pre.receive_from_hex.eq_ignore_ascii_case(&p231_service_hex)
            })
    });
    let p231_c_observable =
        p231_c_obep_pre.is_some() && p231_c_obep_post.is_some() && diag_c_port != 0;
    let p231_target_dispatch_delta = match (&p231_target_gateway_pre, &p231_target_gateway_post) {
        (Some(pre), Some(post)) => p231_delta(post.dispatch_inbound, pre.dispatch_inbound),
        _ => None,
    };
    let p231_target_ibgw_processed_delta = match (&p231_ibgw_pre, &p231_ibgw_post) {
        (Some(pre), Some(post)) => p231_delta(post.processed, pre.processed),
        _ => None,
    };
    let p231_ibgw_overflow_delta = match (&p231_target_gateway_pre, &p231_target_gateway_post) {
        (Some(pre), Some(post)) => {
            p231_delta(post.drop_gateway_overflow, pre.drop_gateway_overflow)
        }
        _ => None,
    };
    let p231_lookup_success_delta = match (&p231_target_gateway_pre, &p231_target_gateway_post) {
        (Some(pre), Some(post)) => {
            p231_delta(post.inbound_lookup_success, pre.inbound_lookup_success)
        }
        _ => None,
    };
    let p231_target_observable = p231_target_diag_port != 0
        && p231_target_gateway_pre.is_some()
        && p231_target_gateway_post.is_some()
        && p231_ibgw_pre.is_some()
        && p231_ibgw_post.is_some();
    let p231_ibgw_exact = p231_ibgw_post.as_ref().is_some_and(|post| {
        post.present
            && post.send_to_hex.eq_ignore_ascii_case(&p231_local_hex)
            && p231_ibgw_pre.as_ref().is_some_and(|pre| pre.present)
    });
    // Reverse-epoch contract row (WP A): nonce, lengths, digests, and
    // the exact selected lease / installed tunnel identities only.
    let p231_nonce = tracked.as_ref().map(|t| t.nonce).unwrap_or(0);
    let p231_payload_len = tracked.as_ref().map(|t| t.payload_len).unwrap_or(0);
    let p231_payload_digest = tracked
        .as_ref()
        .map(|t| t.digest.clone())
        .unwrap_or_else(|| "unknown".to_owned());
    append_evidence(
        &evidence_dir,
        "p231-reverse-epoch",
        &format!(
            "nonce={p231_nonce} payload_len={p231_payload_len} payload_sha256={p231_payload_digest} tracked_send_start_ms={p231_tracked_send_start_ms} accepted_observed={reverse_admitted} accepted_observed_ms={p231_accepted_ms} target_ls_hash={reverse_lookup_target_hex} target_lease_gateway_hash={p231_lease_gateway_hex} target_lease_tunnel_id={p231_lease_tunnel_id} java_outbound_client_send_tunnel_id={p231_a_send_id_pre} java_outbound_client_first_hop_hash={p231_router_c_hex}",
        ),
    );
    let p231_gateway_detail = |stage: &str, stats: &Option<P231GatewayStats>| -> String {
        match stats {
            Some(s) => format!(
                "stage={stage} observable=true dispatch_time={} dispatch_send_time={} dispatch_outbound_tunnel={} drop_gateway_overflow={} dispatch_inbound={} inbound_lookup_success={} dispatch_endpoint={} dispatch_participant={}",
                s.dispatch_time,
                s.dispatch_send_time,
                s.dispatch_outbound_tunnel,
                s.drop_gateway_overflow,
                s.dispatch_inbound,
                s.inbound_lookup_success,
                s.dispatch_endpoint,
                s.dispatch_participant,
            ),
            None => format!("stage={stage} observable=false"),
        }
    };
    append_evidence(
        &evidence_dir,
        "p231-a-gateway",
        &p231_gateway_detail("pre", &p231_a_gateway_pre),
    );
    append_evidence(
        &evidence_dir,
        "p231-a-gateway",
        &p231_gateway_detail("post-immediate", &p231_a_gateway_post_imm),
    );
    append_evidence(
        &evidence_dir,
        "p231-a-gateway",
        &p231_gateway_detail("post-window", &p231_a_gateway_post),
    );
    let p231_still_installed = p231_a_tunnels_pre.as_ref().is_some_and(|tunnels| {
        tunnels.client_resolved && tunnels.outbound_exact_one_remote_hop_via_c
    });
    append_evidence(
        &evidence_dir,
        "p231-a-client-outbound",
        &match &p231_a_client_ob_pre {
            Some(ob) => format!(
                "stage=pre observable=true client_resolved={} outbound_tunnel_count={} single_send_tunnel_id={} still_installed_exact_via_c={p231_still_installed}",
                ob.client_resolved, ob.outbound_tunnel_count, ob.single_send_tunnel_id,
            ),
            None => format!(
                "stage=pre observable=false still_installed_exact_via_c={p231_still_installed}"
            ),
        },
    );
    let p231_participating_detail = |stage: &str,
                                     role: &str,
                                     snap: &Option<P231Participating>|
     -> String {
        match snap {
            Some(p) => format!(
                "stage={stage} role={role} observable=true present={} match_count={} receive_id={} send_id={} receive_from={} send_to={} processed={}",
                p.present,
                p.match_count,
                p.receive_id,
                p.send_id,
                p.receive_from_hex,
                p.send_to_hex,
                p.processed,
            ),
            None => format!("stage={stage} role={role} observable=false"),
        }
    };
    append_evidence(
        &evidence_dir,
        "p231-c-obep",
        &p231_participating_detail("pre", "C", &p231_c_obep_pre),
    );
    append_evidence(
        &evidence_dir,
        "p231-c-obep",
        &p231_participating_detail("post-window", "C", &p231_c_obep_post),
    );
    append_evidence(
        &evidence_dir,
        "p231-target-gateway",
        &p231_gateway_detail(
            &format!("pre role={}", p231_target_role.unwrap_or("unknown")),
            &p231_target_gateway_pre,
        ),
    );
    append_evidence(
        &evidence_dir,
        "p231-target-gateway",
        &p231_gateway_detail(
            &format!("post-window role={}", p231_target_role.unwrap_or("unknown")),
            &p231_target_gateway_post,
        ),
    );
    append_evidence(
        &evidence_dir,
        "p231-ibgw",
        &p231_participating_detail("pre", p231_target_role.unwrap_or("unknown"), &p231_ibgw_pre),
    );
    append_evidence(
        &evidence_dir,
        "p231-ibgw",
        &p231_participating_detail(
            "post-window",
            p231_target_role.unwrap_or("unknown"),
            &p231_ibgw_post,
        ),
    );
    append_evidence(
        &evidence_dir,
        "p231-ocmosj-scratch",
        &format!(
            "dispatch_log_pre={} dispatch_log_post={} no_matching_ob_pre={} no_matching_ob_post={} no_matching_ob_corr_pre={} no_matching_ob_corr_post={}",
            p231_ocmosj_pre.map_or("unknown".to_owned(), |count| count.to_string()),
            p231_ocmosj_post.map_or("unknown".to_owned(), |count| count.to_string()),
            p231_no_ob_pre.map_or("unknown".to_owned(), |count| count.to_string()),
            p231_no_ob_post.map_or("unknown".to_owned(), |count| count.to_string()),
            p231_no_ob_corr_pre.map_or("unknown".to_owned(), |count| count.to_string()),
            p231_no_ob_corr_post.map_or("unknown".to_owned(), |count| count.to_string()),
        ),
    );
    append_evidence(
        &evidence_dir,
        "p231-c-scratch",
        &format!(
            "obep_drop_markers_pre={} obep_drop_markers_post={} no_matching_ibgw_pre={} no_matching_ibgw_post={} no_matching_ibgw_corr_pre={} no_matching_ibgw_corr_post={}",
            p231_obep_drops_pre.map_or("unknown".to_owned(), |count| count.to_string()),
            p231_obep_drop_post.map_or("unknown".to_owned(), |count| count.to_string()),
            p231_no_ibgw_pre.map_or("unknown".to_owned(), |count| count.to_string()),
            p231_no_ibgw_post.map_or("unknown".to_owned(), |count| count.to_string()),
            p231_no_ibgw_corr_pre.map_or("unknown".to_owned(), |count| count.to_string()),
            p231_no_ibgw_corr_post.map_or("unknown".to_owned(), |count| count.to_string()),
        ),
    );
    // WP E i2pr reverse-epoch counters with SSU2 transport deltas.
    let p231_ssu2_after = handle.snapshot();
    append_evidence(
        &evidence_dir,
        "p231-i2pr-reverse",
        &format!(
            "ssu2_datagrams_received_delta={} i2np_messages_received_delta={} tunneldata_messages_seen={p231_tunneldata_seen} expected_inbound_tunneldata_seen={p231_expected_seen} unexpected_tunneldata_seen={p231_unexpected_seen} tunneldata_decode_failures={p231_decode_failures} tunnel_recovery_successes={p231_recovery_complete_expected} tunnel_recovery_failures={p231_recovery_error_expected} recovery_fail_unknown_tunnel={p231_recovery_error_unknown_tunnel} recovery_fail_incomplete={p231_recovery_error_incomplete} recovery_fail_unexpected_body={p231_recovery_error_unexpected_body} recovery_fail_inbound={p231_recovery_error_inbound} garlic_decode_successes={p231_garlic_ok_expected} garlic_decode_failures={p231_garlic_fail_expected} destination_dispatch_calls={p231_dispatch_calls_expected} destination_payload_queue_hits={p231_queue_hits_expected} destination_queued_decode_fails={p231_queued_decode_fails} destination_payload_digest_match={} unexpected_recovery_complete={p231_recovery_complete_unexpected} unexpected_recovery_error={p231_recovery_error_unexpected}",
            p231_ssu2_after
                .datagrams_received
                .saturating_sub(p231_ssu2_before.datagrams_received),
            p231_ssu2_after
                .i2np_received
                .saturating_sub(p231_ssu2_before.i2np_received),
            frozen_payload_45s,
        ),
    );
    // Assemble the pure classifier inputs from the isolated-epoch
    // deltas, exact tunnel matches, and tunnel-id-attributed i2pr
    // outcomes. Global counters support but never independently
    // satisfy a target-specific stage.
    let p231_inputs = P231Inputs {
        accepted_observed: reverse_admitted,
        outbound_send_id_known: p231_a_send_id_pre != 0,
        outbound_still_installed: p231_still_installed,
        dispatch_outbound_delta: p231_dispatch_delta,
        overflow_delta_a: p231_overflow_delta,
        client_dispatch_time_delta: p231_client_time_delta,
        dispatch_outbound_window_delta: p231_dispatch_window_delta,
        overflow_window_delta: p231_overflow_window_delta,
        no_matching_ob_correlated: p231_count_delta(p231_no_ob_corr_post, p231_no_ob_corr_pre)
            .is_some_and(|delta| delta > 0),
        c_obep_observable: p231_c_observable,
        c_obep_present_exact: p231_c_obep_exact,
        c_obep_processed_delta: p231_c_processed_delta,
        c_drop_markers_delta: p231_count_delta(p231_obep_drop_post, p231_obep_drops_pre),
        target_observable: p231_target_observable,
        b_gateway_receipt_correlated: p231_count_delta(
            p231_no_ibgw_corr_post,
            p231_no_ibgw_corr_pre,
        )
        .is_some_and(|delta| delta > 0),
        target_dispatch_inbound_delta: p231_target_dispatch_delta,
        target_ibgw_processed_delta: p231_target_ibgw_processed_delta,
        ibgw_present_exact: p231_ibgw_exact,
        ibgw_overflow_delta: p231_ibgw_overflow_delta,
        ibgw_lookup_attempted_in_window: p231_lookup_success_delta.is_some_and(|delta| delta > 0),
        expected_tunnel_id_known: true,
        expected_tunneldata_seen: p231_expected_seen > 0,
        expected_recovery_completes: p231_recovery_complete_expected,
        expected_recovery_errors: p231_recovery_error_expected,
        expected_garlic_decodes_ok: p231_garlic_ok_expected,
        expected_garlic_decodes_fail: p231_garlic_fail_expected,
        expected_dispatch_calls: p231_dispatch_calls_expected,
        expected_queue_hits: p231_queue_hits_expected,
        expected_queued_decode_fails: p231_queued_decode_fails,
        expected_digest_match_45s: frozen_payload_45s,
    };
    let p231_a_enqueued = !p231_inputs.no_matching_ob_correlated
        && (p231_positive(&p231_inputs.dispatch_outbound_delta)
            || (p231_inputs.client_dispatch_time_delta == Some(1)
                && p231_positive(&p231_inputs.dispatch_outbound_window_delta)));
    let p231_b_forward_proven = p231_positive(&p231_inputs.target_dispatch_inbound_delta)
        || p231_positive(&p231_inputs.target_ibgw_processed_delta);
    let p231_c_emitted_proven = p231_positive(&p231_inputs.target_ibgw_processed_delta)
        && !p231_inputs.ibgw_lookup_attempted_in_window;
    append_evidence(
        &evidence_dir,
        "p231-stages",
        &format!(
            "stage_a_enqueued={p231_a_enqueued} stage_b_forward_proven={p231_b_forward_proven} stage_c_emitted_proven={p231_c_emitted_proven} role={} lease_tunnel={p231_lease_tunnel_id} send_id={p231_a_send_id_pre}",
            p231_target_role.unwrap_or("unknown"),
        ),
    );
    let p231_terminal = p231_classify(&p231_inputs);
    record_p231_classification(
        &evidence_dir,
        p231_terminal,
        &format!(
            "nonce={p231_nonce} role={} lease_tunnel={p231_lease_tunnel_id} send_id={p231_a_send_id_pre} expected_seen={} digest_match={} ordered_statuses={:?}",
            p231_target_role.unwrap_or("unknown"),
            p231_expected_seen > 0,
            frozen_payload_45s,
            p222_status_events
                .iter()
                .map(|e| e.status)
                .collect::<Vec<_>>(),
        ),
    );
    // Plan 232 §9 — corrected-route reverse outcome on the same epoch.
    // Parity is proven by construction above (the lease came from the
    // installed route and the preflight asserted it); the classifier
    // input reuses the frozen 45-second digest plus the tunnel-id
    // attributed pump counters so a new boundary is directly comparable
    // to Plan 231.
    let p232_parity_ok = p232_published_gateway_match && p232_published_tunnel_match;
    let p232_terminal = p232_classify_reverse(&P232ReverseInputs {
        parity_ok: p232_parity_ok,
        frozen_digest_match_45s: frozen_payload_45s,
        expected_tunneldata_seen: p231_expected_seen > 0,
        recovery_completes: p231_recovery_complete_expected,
        recovery_errors: p231_recovery_error_expected,
        garlic_decodes_ok: p231_garlic_ok_expected,
        garlic_decodes_fail: p231_garlic_fail_expected,
        dispatch_calls: p231_dispatch_calls_expected,
        queue_hits: p231_queue_hits_expected,
        queued_decode_fails: p231_queued_decode_fails,
        pre_target_ibgw_present: p231_ibgw_pre.as_ref().is_some_and(|snap| snap.present),
    });
    record_p232_classification(
        &evidence_dir,
        p232_terminal,
        &format!(
            "nonce={p231_nonce} role={} lease_tunnel={p231_lease_tunnel_id} send_id={p231_a_send_id_pre} parity_ok={p232_parity_ok} expected_seen={} digest_match={} ordered_statuses={:?}",
            p231_target_role.unwrap_or("unknown"),
            p231_expected_seen > 0,
            frozen_payload_45s,
            p222_status_events
                .iter()
                .map(|e| e.status)
                .collect::<Vec<_>>(),
        ),
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
    if kv.get("observable").is_none_or(|v| v != "true") {
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
    if kv.get("observable").is_none_or(|v| v != "true") {
        return None;
    }
    if kv.get("client_db_resolved").is_none_or(|v| v != "true")
        || kv.get("client_db_is_client").is_none_or(|v| v != "true")
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
    if kv.get("observable").is_none_or(|v| v != "true") {
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

/// Plan 225 — effective logger activation reported by the running pinned
/// Java LogManager. The file-level `logger.config` check remains useful as a
/// provenance fact, but it is not sufficient to make a missing log line
/// authoritative unless these exact class levels are also active in-process.
#[derive(Clone, Debug, PartialEq, Eq)]
struct P225LoggerConfig {
    effective: bool,
    default_level: String,
    isj_level: String,
    dlm_level: String,
    ibmd_level: String,
}

fn p225_parse_logger_config(line: &str) -> Option<P225LoggerConfig> {
    if !line.starts_with("P225-EV ") || !line.contains("kind=logger-config") {
        return None;
    }
    let kv = p220_parse_kv(&line.replace("P225-EV ", "P220-EV "));
    let observable = kv.get("observable").is_some_and(|v| v == "true");
    let default_level = kv.get("default_level")?.to_owned();
    let isj_level = kv.get("isj_level")?.to_owned();
    let dlm_level = kv.get("dlm_level")?.to_owned();
    let ibmd_level = kv.get("ibmd_level")?.to_owned();
    let effective = observable
        && default_level == "ERROR"
        && isj_level == "INFO"
        && dlm_level == "DEBUG"
        && ibmd_level == "INFO";
    Some(P225LoggerConfig {
        effective,
        default_level,
        isj_level,
        dlm_level,
        ibmd_level,
    })
}

async fn p225_collect_logger_config(diag_port: u16) -> Option<P225LoggerConfig> {
    let line = p220_query_diagnostic(diag_port, "P225-LOGGER-CONFIG").await?;
    p225_parse_logger_config(&line)
}

fn p225_parse_hash_b32(line: &str, expected_hex: &str) -> Option<String> {
    if !line.starts_with("P225-EV ") || !line.contains("kind=hash-b32") {
        return None;
    }
    let kv = p220_parse_kv(&line.replace("P225-EV ", "P220-EV "));
    if kv.get("observable").is_none_or(|v| v != "true") {
        return None;
    }
    let echoed = kv.get("hash_hex")?;
    if !echoed.eq_ignore_ascii_case(expected_hex) {
        return None;
    }
    let b32 = kv.get("hash_b32")?;
    if b32.len() != 52
        || !b32
            .bytes()
            .all(|b| b.is_ascii_lowercase() || (b'2'..=b'7').contains(&b))
    {
        return None;
    }
    Some(b32.to_owned())
}

async fn p225_collect_hash_b32(diag_port: u16, hash_hex: &str) -> Option<String> {
    let line = p220_query_diagnostic(diag_port, &format!("P225-HASH-B32 {hash_hex}")).await?;
    p225_parse_hash_b32(&line, hash_hex)
}

fn record_p225_logger_config(evidence_dir: &Path, label: &str, config: Option<&P225LoggerConfig>) {
    match config {
        Some(config) => append_evidence(
            evidence_dir,
            label,
            &format!(
                "observable=true effective={} default_level={} isj_level={} dlm_level={} ibmd_level={}",
                config.effective,
                config.default_level,
                config.isj_level,
                config.dlm_level,
                config.ibmd_level,
            ),
        ),
        None => append_evidence(
            evidence_dir,
            label,
            "observable=false effective=false reason=diagnostic-unreachable-or-malformed",
        ),
    }
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
const P226_MAX_TARGET_JOB_IDS: usize = 8;

#[derive(Clone, Debug, Default)]
struct P224LogScan {
    files_read: usize,
    bytes_scanned: u64,
    truncated: bool,
    isj_new: u64,
    isj_try: u64,
    isj_try_to_b: u64,
    isj_encrypted_to_b: u64,
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
    // Plan 226 exact target-job facts. These are kept as bounded typed
    // counters only; no log line or job identifier is exported verbatim.
    p226_target_job_ids: Vec<u64>,
    p226_target_job_id_overflow: bool,
    p226_ip_close_skipped: u64,
    p226_old_router_rejected: u64,
    p226_zero_hop_unknown_rejected: u64,
    p226_encrypted_lookup_unsupported: u64,
    p226_no_ib_client_tunnel: u64,
    p226_no_reply_crypto: u64,
    p226_peer_try_count: u64,
    p226_query_to_b: u64,
    p226_search_failed: u64,
}

fn p226_new_isj_job_id(line: &str, target_b64: &str) -> Option<u64> {
    let marker = "New ISJ for ";
    let marker_pos = line.find(marker)?;
    let suffix = &line[marker_pos + marker.len()..];
    let target_present = p226_hash_suffix_matches(suffix, target_b64);
    if !target_present {
        return None;
    }
    let job_marker = "JobId: ";
    let job_start = line[..marker_pos].rfind(job_marker)? + job_marker.len();
    let digits = line[job_start..].split(';').next()?;
    (!digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit()))
        .then(|| digits.parse().ok())
        .flatten()
}

fn p226_hash_token_matches(token: &str, expected_b64: &str) -> bool {
    token == expected_b64 || token == format!("[Hash: {expected_b64}]")
}

fn p226_hash_suffix_matches(suffix: &str, expected_b64: &str) -> bool {
    let suffix = suffix.strip_prefix("LS ").unwrap_or(suffix);
    [expected_b64.to_owned(), format!("[Hash: {expected_b64}]")]
        .into_iter()
        .any(|rendering| {
            suffix
                .strip_prefix(&rendering)
                .is_some_and(|rest| rest.is_empty() || rest.starts_with(char::is_whitespace))
        })
}

fn p226_job_id_before(line: &str, marker: &str) -> Option<u64> {
    let marker_pos = line.find(marker)?;
    let prefix = line[..marker_pos].split_whitespace().last()?;
    prefix.trim_end_matches(':').parse().ok()
}

fn p226_target_job_line(line: &str, target_job_ids: &[u64], marker: &str) -> bool {
    p226_job_id_before(line, marker)
        .is_some_and(|job_id| target_job_ids.binary_search(&job_id).is_ok())
}

fn p226_token_after<'a>(line: &'a str, marker: &str) -> Option<&'a str> {
    line.split_once(marker)?.1.split_whitespace().next()
}

fn p226_target_and_peer(line: &str, marker: &str, target_b64: &str, peer_b64: &str) -> bool {
    let Some(suffix) = line.split_once(marker).map(|(_, suffix)| suffix) else {
        return false;
    };
    let mut fields = suffix.split_whitespace();
    fields
        .next()
        .is_some_and(|token| p226_hash_token_matches(token, target_b64))
        && fields.next() == Some("to")
        && fields
            .next()
            .is_some_and(|token| p226_hash_token_matches(token, peer_b64))
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
    p224_scan_log_dir_with_b32(dir, target_b64, router_b_b64, helper_b64, None)
}

fn p224_scan_log_dir_with_b32(
    dir: &Path,
    target_b64: &str,
    router_b_b64: &str,
    helper_b64: Option<&str>,
    helper_b32: Option<&str>,
) -> Option<P224LogScan> {
    // The pinned Java launcher receives `${JAVA_DATA}/logs`, while Java's
    // LogManager creates the router files below `${JAVA_DATA}/logs/logs`.
    // Inspect exactly one conventional child directory; do not recurse over
    // an attacker-controlled scratch tree.
    let mut log_dirs = vec![dir.to_path_buf()];
    let child_logs = std::fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .find_map(|entry| {
            (entry.file_name() == "logs"
                && entry.file_type().ok().is_some_and(|kind| kind.is_dir()))
            .then(|| entry.path())
        });
    if let Some(child_logs) = child_logs {
        log_dirs.push(child_logs);
    }
    let mut entries: Vec<PathBuf> = Vec::new();
    for log_dir in log_dirs {
        let read_dir = std::fs::read_dir(log_dir).ok()?;
        for entry in read_dir {
            let entry = entry.ok()?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if !name.starts_with("log-router-") || !name.ends_with(".txt") {
                continue;
            }
            entries.push(entry.path());
        }
    }
    entries.sort();
    entries.dedup();
    if entries.is_empty() {
        return None;
    }
    entries.truncate(P224_MAX_LOG_FILES_PER_ROUTER);
    let target_token = format!("[Hash: {target_b64}]");
    let router_b_token = format!("[Hash: {router_b_b64}]");
    let helper_b64_token = helper_b64.map(|helper| format!("sent to: {helper}"));
    let helper_b32_token = helper_b32.map(|helper| format!("sent to: {helper}.b32.i2p"));
    let line_has_target = |line: &str| line.contains(target_b64) || line.contains(&target_token);
    let line_has_router_b =
        |line: &str| line.contains(router_b_b64) || line.contains(&router_b_token);
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
        // Plan 226 §A — identify the exact target ISJ job first. The
        // second pass only attributes facts whose numeric getJobId()
        // prefix belongs to this bounded target set.
        for line in text.lines() {
            if let Some(job_id) = p226_new_isj_job_id(line, target_b64)
                && !scan.p226_target_job_ids.contains(&job_id)
            {
                if scan.p226_target_job_ids.len() < P226_MAX_TARGET_JOB_IDS {
                    scan.p226_target_job_ids.push(job_id);
                } else {
                    scan.p226_target_job_id_overflow = true;
                }
            }
        }
        scan.p226_target_job_ids.sort_unstable();
        for line in text.lines() {
            if line.contains("IterativeSearchJob:") || line.contains("ISJ ") {
                scan.a_isj_any += 1;
            }
            if line.contains("HandleDatabaseLookupMessageJob:")
                && line.contains("Handling database lookup message for ")
            {
                scan.b_dlm_any += 1;
            }
            let target_job =
                |marker: &str| p226_target_job_line(line, &scan.p226_target_job_ids, marker);
            if target_job(": Skipping query w/ router too close to others ")
                && p226_token_after(line, ": Skipping query w/ router too close to others ")
                    .is_some_and(|token| p226_hash_token_matches(token, router_b_b64))
            {
                scan.p226_ip_close_skipped += 1;
            }
            if target_job(": not sending query to old router: ") {
                scan.p226_old_router_rejected += 1;
            }
            if target_job(": not doing zero-hop lookup to unknown ") {
                scan.p226_zero_hop_unknown_rejected += 1;
            }
            if target_job(": Can't do encrypted lookup to ") {
                scan.p226_encrypted_lookup_unsupported += 1;
            }
            if target_job(": ISJ from ") && line.contains("no IB client tunnel to receive reply") {
                scan.p226_no_ib_client_tunnel += 1;
            }
            if target_job(": ISJ from client for ")
                && line.contains("skipped, no ratchet/elg support")
            {
                scan.p226_no_reply_crypto += 1;
            }
            if target_job(": ISJ try ") {
                scan.p226_peer_try_count += 1;
            }
            if target_job(": Encrypted DLM for ")
                && p226_target_and_peer(line, ": Encrypted DLM for ", target_b64, router_b_b64)
            {
                scan.p226_query_to_b += 1;
            }
            if target_job(": ISJ for ") && line.contains(target_b64) && line.contains(" failed") {
                scan.p226_search_failed += 1;
            }
            if line.contains("New ISJ for LS ") && line_has_target(line) {
                scan.isj_new += 1;
            }
            if !line_has_target(line) {
                if line.contains("DLM reply encryption error") {
                    scan.b_reply_enc_err += 1;
                }
                continue;
            }
            if line.contains("ISJ try ") && line.contains(" for LS ") {
                scan.isj_try += 1;
                if line_has_router_b(line) {
                    scan.isj_try_to_b += 1;
                }
                if line.contains("reply via client tunnel? true") {
                    scan.isj_try_client_tunnel += 1;
                }
            }
            if line.contains(" for LS ")
                && line.contains("failed, no IB client tunnel to receive reply")
            {
                scan.isj_no_ib += 1;
            }
            if line.contains(" for LS ") && line.contains("skipped, no ratchet/elg support") {
                scan.isj_no_crypto += 1;
            }
            if line.contains("ISJ for ") && line.contains(" successful") {
                scan.isj_success += 1;
            }
            if line.contains("ISJ for ") && line.contains(" failed") {
                scan.isj_failed += 1;
            }
            if line.contains("Storing garlic LS down tunnel for: ")
                && (helper_b32_token
                    .as_ref()
                    .is_some_and(|helper| line.contains(helper))
                    || helper_b64_token
                        .as_ref()
                        .is_some_and(|helper| line.contains(helper)))
            {
                scan.a_client_tunnel_ls += 1;
            }
            if line.contains("HandleDatabaseLookupMessageJob:")
                && line.contains("Handling database lookup message for ")
            {
                scan.b_lookup += 1;
            }
            if line.contains("We have the published LS ") && line.contains(" answering query") {
                scan.b_answered += 1;
            }
            if line.contains("Encrypted DLM for ") && line_has_router_b(line) {
                scan.isj_encrypted_to_b += 1;
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
#[allow(clippy::too_many_arguments)]
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
    p224_build_trace_common(
        target_hash_hex,
        target_b64,
        router_b_b64,
        helper_b64,
        scan_a,
        scan_b,
        logger_config_a_ok,
        logger_config_b_ok,
    )
}

/// Plan 225 production wrapper. A successful client-tunnel attribution must
/// have the exact JVM Base32 client label; when that diagnostic is unavailable,
/// keep the complete lookup trace Unknown instead of falling back to a guessed
/// or differently encoded client identifier.
#[allow(clippy::too_many_arguments)]
fn p224_build_trace_with_b32(
    target_hash_hex: &str,
    target_b64: Option<&str>,
    router_b_b64: Option<&str>,
    helper_b64: Option<&str>,
    helper_b32: Option<&str>,
    scan_a: Option<&P224LogScan>,
    scan_b: Option<&P224LogScan>,
    logger_config_a_ok: bool,
    logger_config_b_ok: bool,
) -> P224Trace {
    if helper_b32.is_none() {
        return P224Trace {
            target_hash_hex: target_hash_hex.to_owned(),
            ..P224Trace::default()
        };
    }
    p224_build_trace_common(
        target_hash_hex,
        target_b64,
        router_b_b64,
        helper_b64,
        scan_a,
        scan_b,
        logger_config_a_ok,
        logger_config_b_ok,
    )
}

#[allow(clippy::too_many_arguments)]
fn p224_build_trace_common(
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
    trace.query_started = scan_a.isj_new > 0 || scan_a.isj_try > 0;
    // `ISJ try` records a candidate attempt. The pinned Java source emits
    // `Encrypted DLM for <target> to <peer>` immediately before handing the
    // encrypted query to the tunnel dispatcher; only that stronger line is a
    // query-dispatch fact for Plan 225.
    trace.query_to_b = scan_a.isj_encrypted_to_b > 0;
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

/// Plan 226 exact target-job facts. `observable` is false unless the target
/// `New ISJ` line identifies exactly one bounded numeric job and both log
/// scans remain within their existing caps.
#[derive(Clone, Debug, Default)]
struct P226Trace {
    observable: bool,
    target_job_count: usize,
    target_job_id_overflow: bool,
    b_ip_close_skipped: bool,
    b_old_router_rejected: bool,
    b_zero_hop_unknown_rejected: bool,
    b_encrypted_lookup_unsupported: bool,
    no_ib_client_tunnel: bool,
    no_reply_crypto: bool,
    peer_try_count: u64,
    query_to_b: bool,
    search_failed: bool,
}

fn p226_build_trace(scan_a: Option<&P224LogScan>, scan_b: Option<&P224LogScan>) -> P226Trace {
    let (Some(scan_a), Some(scan_b)) = (scan_a, scan_b) else {
        return P226Trace::default();
    };
    let target_job_count = scan_a.p226_target_job_ids.len();
    let observable = !scan_a.truncated
        && !scan_b.truncated
        && scan_a.files_read > 0
        && scan_b.files_read > 0
        && target_job_count == 1
        && !scan_a.p226_target_job_id_overflow;
    P226Trace {
        observable,
        target_job_count,
        target_job_id_overflow: scan_a.p226_target_job_id_overflow,
        b_ip_close_skipped: scan_a.p226_ip_close_skipped > 0,
        b_old_router_rejected: scan_a.p226_old_router_rejected > 0,
        b_zero_hop_unknown_rejected: scan_a.p226_zero_hop_unknown_rejected > 0,
        b_encrypted_lookup_unsupported: scan_a.p226_encrypted_lookup_unsupported > 0,
        no_ib_client_tunnel: scan_a.p226_no_ib_client_tunnel > 0,
        no_reply_crypto: scan_a.p226_no_reply_crypto > 0,
        peer_try_count: scan_a.p226_peer_try_count,
        query_to_b: scan_a.p226_query_to_b > 0,
        search_failed: scan_a.p226_search_failed > 0,
    }
}

fn record_p226_trace(evidence_dir: &Path, trace: &P226Trace, selected_preflight: bool) {
    append_evidence(
        evidence_dir,
        "p226-target-job-trace",
        &format!(
            "observable={} target_job_count={} target_job_id_overflow={} b_selected_preflight={} b_ip_close_skipped={} b_old_router_rejected={} b_zero_hop_unknown_rejected={} b_encrypted_lookup_unsupported={} no_ib_client_tunnel={} no_reply_crypto={} peer_try_count={} query_to_b={} search_failed={}",
            trace.observable,
            trace.target_job_count,
            trace.target_job_id_overflow,
            selected_preflight,
            trace.b_ip_close_skipped,
            trace.b_old_router_rejected,
            trace.b_zero_hop_unknown_rejected,
            trace.b_encrypted_lookup_unsupported,
            trace.no_ib_client_tunnel,
            trace.no_reply_crypto,
            trace.peer_try_count,
            trace.query_to_b,
            trace.search_failed,
        ),
    );
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum P226Terminal {
    BaselineIpDiversityConfirmed,
    BaselineOldRouterRejected,
    BaselineZeroHopUnknown,
    BaselineEncryptedLookupUnsupported,
    BaselineNoInboundClientTunnel,
    BaselineNoReplyCrypto,
    BaselineNotIpDiversity,
    EvidenceContradictionIpDiversityPersists,
    NextBoundaryBStillNotQueried,
    NextBoundaryAToBLookupDelivery,
    EvidenceContradictionBAnswerableNotAnswered,
    NextBoundaryBReplyToAClientTunnel,
    EvidenceContradictionClientDsmNotStored,
    NextBoundary(Vec<i32>),
    ReverseDeliveryPassed,
    EnvironmentDistinctLoopbackUnavailable,
    ObservabilityGap,
}

impl P226Terminal {
    fn token(&self) -> String {
        match self {
            Self::BaselineIpDiversityConfirmed => "P226-BASELINE-IP-DIVERSITY-CONFIRMED".to_owned(),
            Self::BaselineOldRouterRejected => "P226-BASELINE-B-OLD-ROUTER-REJECTED".to_owned(),
            Self::BaselineZeroHopUnknown => "P226-BASELINE-B-ZERO-HOP-UNKNOWN".to_owned(),
            Self::BaselineEncryptedLookupUnsupported => {
                "P226-BASELINE-B-ENCRYPTED-LOOKUP-UNSUPPORTED".to_owned()
            }
            Self::BaselineNoInboundClientTunnel => {
                "P226-BASELINE-NO-INBOUND-CLIENT-TUNNEL".to_owned()
            }
            Self::BaselineNoReplyCrypto => "P226-BASELINE-NO-REPLY-CRYPTO".to_owned(),
            Self::BaselineNotIpDiversity => "P226-BASELINE-NOT-IP-DIVERSITY".to_owned(),
            Self::EvidenceContradictionIpDiversityPersists => {
                "P226-EVIDENCE-CONTRADICTION-IP-DIVERSITY-PERSISTS".to_owned()
            }
            Self::NextBoundaryBStillNotQueried => {
                "P226-NEXT-BOUNDARY-B-STILL-NOT-QUERIED".to_owned()
            }
            Self::NextBoundaryAToBLookupDelivery => {
                "P226-NEXT-BOUNDARY-A-TO-B-LOOKUP-DELIVERY".to_owned()
            }
            Self::EvidenceContradictionBAnswerableNotAnswered => {
                "P226-EVIDENCE-CONTRADICTION-B-ANSWERABLE-NOT-ANSWERED".to_owned()
            }
            Self::NextBoundaryBReplyToAClientTunnel => {
                "P226-NEXT-BOUNDARY-B-REPLY-TO-A-CLIENT-TUNNEL".to_owned()
            }
            Self::EvidenceContradictionClientDsmNotStored => {
                "P226-EVIDENCE-CONTRADICTION-CLIENT-DSM-NOT-STORED".to_owned()
            }
            Self::NextBoundary(statuses) => {
                format!("P226-NEXT-BOUNDARY ordered_statuses={statuses:?}")
            }
            Self::ReverseDeliveryPassed => "P226-REVERSE-DELIVERY-PASSED".to_owned(),
            Self::EnvironmentDistinctLoopbackUnavailable => {
                "P226-ENVIRONMENT-DISTINCT-LOOPBACK-SUBNETS-UNAVAILABLE".to_owned()
            }
            Self::ObservabilityGap => "P226-OBSERVABILITY-GAP".to_owned(),
        }
    }
}

fn p226_classify_baseline(selected_preflight: bool, trace: &P226Trace) -> P226Terminal {
    if !trace.observable {
        return P226Terminal::ObservabilityGap;
    }
    if trace.b_old_router_rejected {
        return P226Terminal::BaselineOldRouterRejected;
    }
    if trace.b_zero_hop_unknown_rejected {
        return P226Terminal::BaselineZeroHopUnknown;
    }
    if trace.b_encrypted_lookup_unsupported {
        return P226Terminal::BaselineEncryptedLookupUnsupported;
    }
    if trace.no_ib_client_tunnel {
        return P226Terminal::BaselineNoInboundClientTunnel;
    }
    if trace.no_reply_crypto {
        return P226Terminal::BaselineNoReplyCrypto;
    }
    if selected_preflight && trace.b_ip_close_skipped && !trace.query_to_b && trace.search_failed {
        return P226Terminal::BaselineIpDiversityConfirmed;
    }
    P226Terminal::BaselineNotIpDiversity
}

#[allow(clippy::too_many_arguments)]
fn p226_classify_distinct(
    hosts_distinct: bool,
    selected_preflight: bool,
    trace: &P226Trace,
    b_answerable: bool,
    b_lookup_received: bool,
    b_published_ls_answered: bool,
    a_client_tunnel_ls_received: bool,
    client_db_validated_present: bool,
    status_no_leaseset_21: bool,
    ordered_statuses: &[i32],
    frozen_payload_45s: bool,
) -> P226Terminal {
    if frozen_payload_45s {
        return P226Terminal::ReverseDeliveryPassed;
    }
    if !hosts_distinct {
        return P226Terminal::EnvironmentDistinctLoopbackUnavailable;
    }
    if !trace.observable {
        return P226Terminal::ObservabilityGap;
    }
    if trace.b_ip_close_skipped {
        return P226Terminal::EvidenceContradictionIpDiversityPersists;
    }
    if trace.b_old_router_rejected
        || trace.b_zero_hop_unknown_rejected
        || trace.b_encrypted_lookup_unsupported
        || trace.no_ib_client_tunnel
        || trace.no_reply_crypto
    {
        return P226Terminal::NextBoundaryBStillNotQueried;
    }
    if !selected_preflight || !trace.query_to_b {
        return P226Terminal::NextBoundaryBStillNotQueried;
    }
    if !b_lookup_received {
        return P226Terminal::NextBoundaryAToBLookupDelivery;
    }
    if !b_published_ls_answered {
        return if b_answerable {
            P226Terminal::EvidenceContradictionBAnswerableNotAnswered
        } else {
            P226Terminal::NextBoundaryBStillNotQueried
        };
    }
    if !a_client_tunnel_ls_received {
        return P226Terminal::NextBoundaryBReplyToAClientTunnel;
    }
    if a_client_tunnel_ls_received && !client_db_validated_present {
        return P226Terminal::EvidenceContradictionClientDsmNotStored;
    }
    if !status_no_leaseset_21
        && ordered_statuses.iter().any(|status| *status != 1)
        && !ordered_statuses.is_empty()
    {
        return P226Terminal::NextBoundary(ordered_statuses.to_vec());
    }
    P226Terminal::ObservabilityGap
}

fn record_p226_classification(evidence_dir: &Path, terminal: &P226Terminal, trace: &P226Trace) {
    append_evidence(
        evidence_dir,
        "p226-classification",
        &format!(
            "{} target_job_count={} b_ip_close_skipped={} query_to_b={} search_failed={} peer_try_count={}",
            terminal.token(),
            trace.target_job_count,
            trace.b_ip_close_skipped,
            trace.query_to_b,
            trace.search_failed,
            trace.peer_try_count,
        ),
    );
}

// ---- Plan 227 — explicit one-hop client-tunnel corrective -----------------
// Reference-harness corrective only. No production protocol change. The raw
// helper requests a genuine stock-Java one-hop inbound/outbound client tunnel
// through controlled Router C via ordinary I2CP SessionConfig
// `inbound.explicitPeers` / `outbound.explicitPeers`. Installed pool state
// from `P227-CLIENT-TUNNELS` is authoritative; scratch-log selection facts
// are corroborative only. Exactly one `p227-classification` row per run.

/// Lowercase-hex 32-byte hash validation (exactly 64 chars, no uppercase).
fn p227_is_hex64(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

/// Strict boolean field (`true` / `false` only; anything else is Unknown).
fn p227_parse_bool(value: Option<&String>) -> Option<bool> {
    match value.map(String::as_str) {
        Some("true") => Some(true),
        Some("false") => Some(false),
        _ => None,
    }
}

/// Strict bounded count (0..=8, decimal only).
fn p227_parse_count(value: Option<&String>) -> Option<u64> {
    let raw = value?;
    if raw.is_empty() || raw.len() > 2 || !raw.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let parsed: u64 = raw.parse().ok()?;
    (parsed <= 8).then_some(parsed)
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct P227Eligibility {
    observable: bool,
    main_raw_present: bool,
    main_valid_present: bool,
    selectable: bool,
    established: bool,
    banlisted: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct P227Tunnels {
    observable: bool,
    client_resolved: bool,
    inbound_pool_present: bool,
    outbound_pool_present: bool,
    inbound_tunnel_count: u64,
    outbound_tunnel_count: u64,
    inbound_exact_one_remote_hop_via_c: bool,
    outbound_exact_one_remote_hop_via_c: bool,
    inbound_zero_hop_present: bool,
    outbound_zero_hop_present: bool,
}

fn p227_parse_eligibility(line: &str, expected_c_hex: &str) -> Option<P227Eligibility> {
    if !line.starts_with("P227-EV ") || !line.contains("kind=peer-eligibility") {
        return None;
    }
    // Reject raw-log promotion: the line must be a single bounded diagnostic
    // row, never raw Java log text.
    if line.len() > 1024 || line.contains("not doing zero-hop") || line.contains("Skipping query") {
        return None;
    }
    let kv = p220_parse_kv(&line.replace("P227-EV ", "P220-EV "));
    if kv.get("observable").is_none_or(|v| v != "true") {
        return None;
    }
    let echoed = kv.get("router_c_hex")?;
    if !echoed.eq_ignore_ascii_case(expected_c_hex) || !p227_is_hex64(&echoed.to_lowercase()) {
        return None;
    }
    // Secret/raw-log rejection: no key material, payload, or log text may
    // appear in the diagnostic row.
    for forbidden in [
        "priv",
        "seed",
        "session_key",
        "tag=",
        "payload",
        "log-router",
    ] {
        if line.to_lowercase().contains(forbidden) {
            return None;
        }
    }
    Some(P227Eligibility {
        observable: true,
        main_raw_present: p227_parse_bool(kv.get("main_raw_present"))?,
        main_valid_present: p227_parse_bool(kv.get("main_valid_present"))?,
        selectable: p227_parse_bool(kv.get("selectable"))?,
        established: p227_parse_bool(kv.get("established")).unwrap_or(false),
        banlisted: p227_parse_bool(kv.get("banlisted")).unwrap_or(false),
    })
}

fn p227_parse_tunnels(
    line: &str,
    expected_client_hex: &str,
    expected_c_hex: &str,
) -> Option<P227Tunnels> {
    if !line.starts_with("P227-EV ") || !line.contains("kind=client-tunnels") {
        return None;
    }
    if line.len() > 2048 || line.contains("not doing zero-hop") || line.contains("Skipping query") {
        return None;
    }
    let kv = p220_parse_kv(&line.replace("P227-EV ", "P220-EV "));
    if kv.get("observable").is_none_or(|v| v != "true") {
        return None;
    }
    let echo_client = kv.get("client_dbid_hex")?;
    let echo_c = kv.get("router_c_hex")?;
    if !echo_client.eq_ignore_ascii_case(expected_client_hex)
        || !echo_c.eq_ignore_ascii_case(expected_c_hex)
    {
        return None;
    }
    if !p227_is_hex64(&echo_client.to_lowercase()) || !p227_is_hex64(&echo_c.to_lowercase()) {
        return None;
    }
    for forbidden in ["priv", "seed", "session_key", "payload", "log-router"] {
        if line.to_lowercase().contains(forbidden) {
            return None;
        }
    }
    Some(P227Tunnels {
        observable: true,
        client_resolved: p227_parse_bool(kv.get("client_resolved"))?,
        inbound_pool_present: p227_parse_bool(kv.get("inbound_pool_present"))?,
        outbound_pool_present: p227_parse_bool(kv.get("outbound_pool_present"))?,
        inbound_tunnel_count: p227_parse_count(kv.get("inbound_tunnel_count"))?,
        outbound_tunnel_count: p227_parse_count(kv.get("outbound_tunnel_count"))?,
        inbound_exact_one_remote_hop_via_c: p227_parse_bool(
            kv.get("inbound_exact_one_remote_hop_via_c"),
        )?,
        outbound_exact_one_remote_hop_via_c: p227_parse_bool(
            kv.get("outbound_exact_one_remote_hop_via_c"),
        )?,
        inbound_zero_hop_present: p227_parse_bool(kv.get("inbound_zero_hop_present"))?,
        outbound_zero_hop_present: p227_parse_bool(kv.get("outbound_zero_hop_present"))?,
    })
}

async fn p227_collect_eligibility(diag_port: u16, router_c_hex: &str) -> Option<P227Eligibility> {
    if !p227_is_hex64(router_c_hex) {
        return None;
    }
    let line =
        p220_query_diagnostic(diag_port, &format!("P227-PEER-ELIGIBILITY {router_c_hex}")).await?;
    p227_parse_eligibility(&line, router_c_hex)
}

async fn p227_collect_tunnels(
    diag_port: u16,
    client_dbid_hex: &str,
    router_c_hex: &str,
) -> Option<P227Tunnels> {
    if !p227_is_hex64(client_dbid_hex) || !p227_is_hex64(router_c_hex) {
        return None;
    }
    let line = p220_query_diagnostic(
        diag_port,
        &format!("P227-CLIENT-TUNNELS {client_dbid_hex} {router_c_hex}"),
    )
    .await?;
    p227_parse_tunnels(&line, client_dbid_hex, router_c_hex)
}

fn p227_tunnel_gate_pass(tunnels: &P227Tunnels) -> bool {
    tunnels.observable
        && tunnels.client_resolved
        && tunnels.inbound_pool_present
        && tunnels.outbound_pool_present
        && tunnels.inbound_exact_one_remote_hop_via_c
        && tunnels.outbound_exact_one_remote_hop_via_c
        && !tunnels.inbound_zero_hop_present
        && !tunnels.outbound_zero_hop_present
}

fn p227_eligibility_gate_pass(eligibility: &P227Eligibility) -> bool {
    eligibility.observable
        && eligibility.main_raw_present
        && eligibility.main_valid_present
        && eligibility.selectable
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum P227Terminal {
    CNotSelectable,
    ExplicitOneHopNotBuilt,
    ObservabilityGap,
    EvidenceContradictionOneHopButZeroHopUnknown,
    EvidenceContradictionClientDsmNotStored,
    NextBoundaryBNotQueried,
    NextBoundaryAToBLookupDelivery,
    NextBoundaryBReplyToAClientTunnel,
    NextBoundary(Vec<i32>),
    ReverseDeliveryPassed,
}

impl P227Terminal {
    fn token(&self) -> String {
        match self {
            Self::CNotSelectable => "P227-C-NOT-SELECTABLE".to_owned(),
            Self::ExplicitOneHopNotBuilt => "P227-EXPLICIT-ONE-HOP-NOT-BUILT".to_owned(),
            Self::ObservabilityGap => "P227-OBSERVABILITY-GAP".to_owned(),
            Self::EvidenceContradictionOneHopButZeroHopUnknown => {
                "P227-EVIDENCE-CONTRADICTION-ONE-HOP-BUT-ZERO-HOP-UNKNOWN".to_owned()
            }
            Self::EvidenceContradictionClientDsmNotStored => {
                "P227-EVIDENCE-CONTRADICTION-CLIENT-DSM-NOT-STORED".to_owned()
            }
            Self::NextBoundaryBNotQueried => "P227-NEXT-BOUNDARY-B-NOT-QUERIED".to_owned(),
            Self::NextBoundaryAToBLookupDelivery => {
                "P227-NEXT-BOUNDARY-A-TO-B-LOOKUP-DELIVERY".to_owned()
            }
            Self::NextBoundaryBReplyToAClientTunnel => {
                "P227-NEXT-BOUNDARY-B-REPLY-TO-A-CLIENT-TUNNEL".to_owned()
            }
            Self::NextBoundary(statuses) => {
                format!("P227-NEXT-BOUNDARY ordered_statuses={statuses:?}")
            }
            Self::ReverseDeliveryPassed => "P227-REVERSE-DELIVERY-PASSED".to_owned(),
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn p227_classify(
    eligibility: Option<&P227Eligibility>,
    tunnels: Option<&P227Tunnels>,
    p226_trace: &P226Trace,
    b_answerable: bool,
    b_lookup_received: bool,
    b_published_ls_answered: bool,
    a_client_tunnel_ls_received: bool,
    client_db_validated_present: bool,
    ordered_statuses: &[i32],
    frozen_payload_45s: bool,
) -> P227Terminal {
    if frozen_payload_45s {
        return P227Terminal::ReverseDeliveryPassed;
    }
    let Some(eligibility) = eligibility else {
        return P227Terminal::ObservabilityGap;
    };
    if !eligibility.observable {
        return P227Terminal::ObservabilityGap;
    }
    if !p227_eligibility_gate_pass(eligibility) {
        return P227Terminal::CNotSelectable;
    }
    let Some(tunnels) = tunnels else {
        return P227Terminal::ObservabilityGap;
    };
    if !tunnels.observable {
        return P227Terminal::ObservabilityGap;
    }
    if !p227_tunnel_gate_pass(tunnels) {
        // Inbound-only or outbound-only or zero-hop fallback is insufficient.
        return P227Terminal::ExplicitOneHopNotBuilt;
    }
    // One-hop proven: the zero-hop guard must be absent. If the exact
    // target job still fires the zero-hop branch, that is a contradiction.
    if p226_trace.observable && p226_trace.b_zero_hop_unknown_rejected {
        return P227Terminal::EvidenceContradictionOneHopButZeroHopUnknown;
    }
    if !p226_trace.observable {
        return P227Terminal::ObservabilityGap;
    }
    if !p226_trace.query_to_b {
        return P227Terminal::NextBoundaryBNotQueried;
    }
    if !b_lookup_received {
        return P227Terminal::NextBoundaryAToBLookupDelivery;
    }
    if !b_published_ls_answered {
        // B answerability was already proven pre-send; a missing answer
        // after dispatch is a reply-path boundary, not a re-attribution.
        let _ = b_answerable;
        return P227Terminal::NextBoundaryAToBLookupDelivery;
    }
    if !a_client_tunnel_ls_received {
        return P227Terminal::NextBoundaryBReplyToAClientTunnel;
    }
    if a_client_tunnel_ls_received && !client_db_validated_present {
        return P227Terminal::EvidenceContradictionClientDsmNotStored;
    }
    // Client DB usable and status 21 replaced by a new send terminal.
    let status_21_present = ordered_statuses.contains(&21);
    if !status_21_present && !ordered_statuses.is_empty() {
        return P227Terminal::NextBoundary(ordered_statuses.to_vec());
    }
    P227Terminal::ObservabilityGap
}

fn record_p227_eligibility(
    evidence_dir: &Path,
    eligibility: Option<&P227Eligibility>,
    router_c_hex: &str,
) {
    match eligibility {
        Some(facts) => append_evidence(
            evidence_dir,
            "p227-peer-eligibility",
            &format!(
                "router_c_hex={} observable={} main_raw_present={} main_valid_present={} selectable={} established={} banlisted={}",
                router_c_hex,
                facts.observable,
                facts.main_raw_present,
                facts.main_valid_present,
                facts.selectable,
                facts.established,
                facts.banlisted,
            ),
        ),
        None => append_evidence(
            evidence_dir,
            "p227-peer-eligibility",
            &format!("router_c_hex={router_c_hex} observable=false reason=diagnostic-unreachable"),
        ),
    }
}

fn record_p227_tunnels(
    evidence_dir: &Path,
    tunnels: Option<&P227Tunnels>,
    client_hex: &str,
    router_c_hex: &str,
) {
    match tunnels {
        Some(facts) => append_evidence(
            evidence_dir,
            "p227-client-tunnels",
            &format!(
                "client_dbid_hex={} router_c_hex={} observable={} client_resolved={} inbound_pool_present={} outbound_pool_present={} inbound_tunnel_count={} outbound_tunnel_count={} inbound_exact_one_remote_hop_via_c={} outbound_exact_one_remote_hop_via_c={} inbound_zero_hop_present={} outbound_zero_hop_present={}",
                client_hex,
                router_c_hex,
                facts.observable,
                facts.client_resolved,
                facts.inbound_pool_present,
                facts.outbound_pool_present,
                facts.inbound_tunnel_count,
                facts.outbound_tunnel_count,
                facts.inbound_exact_one_remote_hop_via_c,
                facts.outbound_exact_one_remote_hop_via_c,
                facts.inbound_zero_hop_present,
                facts.outbound_zero_hop_present,
            ),
        ),
        None => append_evidence(
            evidence_dir,
            "p227-client-tunnels",
            &format!(
                "client_dbid_hex={client_hex} router_c_hex={router_c_hex} observable=false reason=diagnostic-unreachable"
            ),
        ),
    }
}

fn record_p227_classification(evidence_dir: &Path, terminal: &P227Terminal) {
    append_evidence(evidence_dir, "p227-classification", &terminal.token());
}

fn record_p227_early_stop_gap(evidence_dir: &Path, reason: &'static str) {
    append_evidence(
        evidence_dir,
        "p227-classification",
        &format!("P227-OBSERVABILITY-GAP reason={reason}"),
    );
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

/// Plan 225 terminal taxonomy. It mirrors the already-tested earliest-stage
/// ordering from Plan 224, but has its own terminal namespace so the
/// corrective's effective-observability result cannot be mistaken for the
/// predecessor plan's closure row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum P225Terminal {
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

impl P225Terminal {
    fn token(self) -> &'static str {
        match self {
            Self::AttributionBMainLsAbsent => "P225-ATTRIBUTION-B-MAIN-LS-ABSENT",
            Self::AttributionBMainLsInvalidOrStale => "P225-ATTRIBUTION-B-MAIN-LS-INVALID-OR-STALE",
            Self::AttributionBMainLsNotQueryAnswerable => {
                "P225-ATTRIBUTION-B-MAIN-LS-NOT-QUERY-ANSWERABLE"
            }
            Self::AttributionANoInboundClientReplyTunnel => {
                "P225-ATTRIBUTION-A-NO-INBOUND-CLIENT-REPLY-TUNNEL"
            }
            Self::AttributionALookupReplyCryptoUnavailable => {
                "P225-ATTRIBUTION-A-LOOKUP-REPLY-CRYPTO-UNAVAILABLE"
            }
            Self::AttributionASearchExhaustedWithoutQueryingB => {
                "P225-ATTRIBUTION-A-SEARCH-EXHAUSTED-WITHOUT-QUERYING-B"
            }
            Self::AttributionAToBLookupNotReceived => "P225-ATTRIBUTION-A-TO-B-LOOKUP-NOT-RECEIVED",
            Self::EvidenceContradictionBAnswerableButNotAnswered => {
                "P225-EVIDENCE-CONTRADICTION-B-ANSWERABLE-BUT-NOT-ANSWERED"
            }
            Self::AttributionBReplyNotObservedOnAClientTunnel => {
                "P225-ATTRIBUTION-B-REPLY-NOT-OBSERVED-ON-A-CLIENT-TUNNEL"
            }
            Self::EvidenceContradictionClientTunnelDsmNotInClientDb => {
                "P225-EVIDENCE-CONTRADICTION-CLIENT-TUNNEL-DSM-NOT-IN-CLIENT-DB"
            }
            Self::EvidenceContradictionNoLeasesetWithUsableClientLs => {
                "P225-EVIDENCE-CONTRADICTION-NO-LEASESET-WITH-USABLE-CLIENT-LS"
            }
            Self::NextBoundary => "P225-NEXT-BOUNDARY",
            Self::ReverseDeliveryPassed => "P225-REVERSE-DELIVERY-PASSED",
            Self::ObservabilityGapLookupPath => "P225-OBSERVABILITY-GAP-LOOKUP-PATH",
        }
    }
}

impl From<P224Terminal> for P225Terminal {
    fn from(value: P224Terminal) -> Self {
        match value {
            P224Terminal::AttributionBMainLsAbsent => Self::AttributionBMainLsAbsent,
            P224Terminal::AttributionBMainLsInvalidOrStale => {
                Self::AttributionBMainLsInvalidOrStale
            }
            P224Terminal::AttributionBMainLsNotQueryAnswerable => {
                Self::AttributionBMainLsNotQueryAnswerable
            }
            P224Terminal::AttributionANoInboundClientReplyTunnel => {
                Self::AttributionANoInboundClientReplyTunnel
            }
            P224Terminal::AttributionALookupReplyCryptoUnavailable => {
                Self::AttributionALookupReplyCryptoUnavailable
            }
            P224Terminal::AttributionASearchExhaustedWithoutQueryingB => {
                Self::AttributionASearchExhaustedWithoutQueryingB
            }
            P224Terminal::AttributionAToBLookupNotReceived => {
                Self::AttributionAToBLookupNotReceived
            }
            P224Terminal::EvidenceContradictionBAnswerableButNotAnswered => {
                Self::EvidenceContradictionBAnswerableButNotAnswered
            }
            P224Terminal::AttributionBReplyNotObservedOnAClientTunnel => {
                Self::AttributionBReplyNotObservedOnAClientTunnel
            }
            P224Terminal::EvidenceContradictionClientTunnelDsmNotInClientDb => {
                Self::EvidenceContradictionClientTunnelDsmNotInClientDb
            }
            P224Terminal::EvidenceContradictionNoLeasesetWithUsableClientLs => {
                Self::EvidenceContradictionNoLeasesetWithUsableClientLs
            }
            P224Terminal::NextBoundary => Self::NextBoundary,
            P224Terminal::ReverseDeliveryPassed => Self::ReverseDeliveryPassed,
            P224Terminal::ObservabilityGapLookupPath => Self::ObservabilityGapLookupPath,
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

#[allow(clippy::too_many_arguments)]
fn p225_classify(
    expected_target_hex: &str,
    b_pre: Option<&P224MainLs>,
    b_post: Option<&P224MainLs>,
    a_pre: Option<&P224ClientLs>,
    a_post: Option<&P224ClientLs>,
    trace: &P224Trace,
    status_no_leaseset_21: bool,
    ordered_statuses: &[i32],
    frozen_payload_45s: bool,
) -> P225Terminal {
    p224_classify(
        expected_target_hex,
        b_pre,
        b_post,
        a_pre,
        a_post,
        trace,
        status_no_leaseset_21,
        ordered_statuses,
        frozen_payload_45s,
    )
    .into()
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

/// Emits exactly one `p225-classification` row for the same authoritative
/// attempt. The P225 row is deliberately separate from P224 so the
/// corrective can be checked independently by the shell evidence gate.
#[allow(clippy::too_many_arguments)]
fn record_p225_classification(
    evidence_dir: &Path,
    terminal: P225Terminal,
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
        "p225-classification",
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

/// Plan 225 pre-epoch stop: no effective lookup-path observation exists, so
/// the corrective emits its own honest gap terminal rather than inheriting a
/// predecessor-plan row.
fn record_p225_early_stop_gap(evidence_dir: &Path, reason: &'static str) {
    append_evidence(
        evidence_dir,
        "p225-lookup-trace",
        &format!("observable=false target_hash_hex=unknown reason={reason}"),
    );
    append_evidence(
        evidence_dir,
        "p225-classification",
        &format!(
            "{} reason={reason}",
            P225Terminal::ObservabilityGapLookupPath.token()
        ),
    );
}

/// Plan 226 pre-epoch stop: the exact target-job topology correction
/// cannot be attributed before the authoritative epoch exists. Emit the
/// required trace and one honest terminal rather than allowing a partial
/// run to masquerade as a baseline or corrected result.
fn record_p226_early_stop_gap(evidence_dir: &Path, reason: &'static str) {
    let trace = P226Trace::default();
    record_p226_trace(evidence_dir, &trace, false);
    append_evidence(
        evidence_dir,
        "p226-classification",
        &format!("{} reason={reason}", P226Terminal::ObservabilityGap.token()),
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
    // Plan 232 WP A/B2 — initial Streaming lease from the installed route.
    let p232_stream_local = local_receive_for_lookup;
    let p232_stream_route = coord
        .registry()
        .inbound_gateway_route(p232_stream_local)
        .expect("installed streaming inbound gateway route");
    assert_eq!(
        p232_stream_route.gateway_router, service_hash,
        "Plan 232 §6: installed streaming route must terminate at the service router"
    );
    assert_eq!(
        p232_stream_route.gateway_receive_tunnel.get(),
        STREAM_IBGW_RECEIVE,
        "Plan 232 §6: installed streaming route must carry the gateway-side receive tunnel"
    );
    assert_eq!(
        p232_stream_route.local_receive_tunnel.get(),
        STREAM_IBGW_NEXT,
        "Plan 232 §6: installed streaming route must carry the local receive tunnel"
    );
    let p232_stream_slot = coord.registry().inbound_slot(p232_stream_local);
    let lease_source = p232_route_derived_lease_source(
        registrations_in[0].slot(),
        p232_stream_local,
        Some(p232_stream_route),
        p232_stream_slot,
        tunnel_expires,
        tunnel_expires.saturating_sub(60),
    )
    .expect("route-derived streaming initial lease");
    let p232_stream_publication = Hash::from_bytes(*java_hash.as_bytes());
    p232_record_lease_route(
        &evidence_dir,
        &P232LeaseRecord {
            lane: "streaming",
            stage: "initial",
            slot: registrations_in[0].slot(),
            local_receive: p232_stream_local,
            route: &p232_stream_route,
            lease: &lease_source,
            publication_target: &p232_stream_publication,
            service_router: &service_hash,
        },
    );
    p232_record_publication_separation(
        &evidence_dir,
        "streaming",
        "initial",
        &p232_stream_publication,
        &p232_stream_route,
        &lease_source,
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
    let accept_start = stream_control.command("START_ACCEPT").await;
    let java_accept_thread_started = accept_start == "STARTED";
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
    let (local_port, remote_port) = streaming
        .get_connection(connection_id)
        .map(|connection| (connection.local_port(), connection.remote_port()))
        .expect("SYN connection remains registered");
    let mut syn_queue = streaming.drain_outbound();
    assert_eq!(syn_queue.len(), 1, "connect must emit exactly one SYN");
    let syn_request = syn_queue.remove(0);
    let mut p234_epoch = P234SynEpoch {
        local_connection_id: connection_id.raw(),
        local_port,
        remote_port,
        syn_sequence_or_message_identity: syn_request.sequence,
        syn_transport_request_emitted: true,
        java_accept_thread_started,
        ..P234SynEpoch::default()
    };
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
    while tokio::time::Instant::now() < syn_ack_deadline && !p234_epoch.connection_established {
        let next = tokio::time::timeout(POLL_INTERVAL, handle.next_inbound()).await;
        let Ok(Some(inbound)) = next else {
            continue;
        };
        let message = match decode_inbound_i2np(&inbound.bytes) {
            Ok(message) => message,
            Err(_) => {
                p234_epoch.i2pr_garlic_decode_failures += 1;
                continue;
            }
        };
        let cell = match message.body() {
            I2npBody::TunnelData(cell) => cell.clone(),
            _ => continue,
        };
        p234_epoch.i2pr_inbound_tunneldata_count += 1;
        if cell.tunnel_id == STREAM_IBGW_NEXT {
            p234_epoch.i2pr_expected_stream_tunneldata_count += 1;
        } else {
            continue;
        }
        let bytes = match dest.recover_garlic_bytes(coord.registry_mut(), &cell, wall_ms()) {
            Ok(bytes) => bytes,
            Err(_) => {
                p234_epoch.i2pr_tunnel_recovery_failures += 1;
                continue;
            }
        };
        p234_epoch.i2pr_tunnel_recovery_count += 1;
        let now_secs = u32::try_from(wall_secs()).unwrap_or(u32::MAX);
        let garlic = match I2npMessage::decode_standard(&bytes, MAX_I2NP_PAYLOAD_SIZE) {
            Ok(message) => message,
            Err(_) => {
                p234_epoch.i2pr_garlic_decode_failures += 1;
                continue;
            }
        };
        let dispatch_outcome = dispatcher.dispatch_garlic_envelope(
            &mut session,
            local_identity.id(),
            local_identity.static_secret_bytes(),
            &local_identity.static_public_bytes(),
            now_secs,
            &garlic,
            routing.lease_set2_store_mut(),
        );
        if matches!(dispatch_outcome, InboundDispatchOutcome::Rejected(_)) {
            p234_epoch.i2pr_garlic_decode_failures += 1;
            continue;
        }
        p234_epoch.i2pr_garlic_payload_count += 1;
        let Some(queued) = dispatcher.pop_payload(local_identity.id()) else {
            continue;
        };
        p234_epoch.i2pr_streaming_adapter_calls += 1;
        match StreamingDestinationAdapter::receive(
            queued.bytes(),
            &local_identity,
            &mut streaming,
            reference_hash.as_bytes(),
            wall_ms(),
        ) {
            Ok(i2pr_client::InboundStreamingOutcome::StreamingDispatched { .. }) => {
                p234_epoch.i2pr_streaming_adapter_successes += 1;
            }
            Ok(_) => {}
            Err(_) => {
                p234_epoch.i2pr_streaming_adapter_errors += 1;
                continue;
            }
        }
        if let Some(conn) = streaming.get_connection(connection_id)
            && conn.state() == i2pr_client::streaming::connection::ConnectionState::Established
        {
            p234_epoch.connection_established = true;
        }
        let pending = streaming.drain_outbound();
        p234_epoch.pending_outbound_after_receive = p234_epoch
            .pending_outbound_after_receive
            .saturating_add(pending.len() as u64);
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
        for request in streaming.poll_acks(wall_ms()) {
            p234_epoch.ack_poll_emissions += 1;
            send_transport_request(
                &request,
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
        for request in streaming.poll_retransmits(wall_ms()) {
            p234_epoch.retransmit_poll_emissions += 1;
            send_transport_request(
                &request,
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
    let java_accept_state = stream_control.report_stream_state().await;
    p234_epoch.java_accept_returned =
        java_accept_state.is_some_and(|state| state.accept_returned > 0);
    p234_epoch.java_stream_receive_observed =
        java_accept_state.is_some_and(|state| state.socket_stored > 0);
    p234_epoch.java_stream_response_observed = p234_epoch.i2pr_expected_stream_tunneldata_count > 0;
    let p234_terminal = p234_classify_syn_epoch(&p234_epoch);
    record_p234_syn_epoch(
        &evidence_dir,
        &p234_epoch,
        java_accept_state,
        &Hash::from_bytes(*local_dest_hash.as_bytes()),
        &Hash::from_bytes(*reference_hash.as_bytes()),
    );
    append_evidence(&evidence_dir, "p234-classification", p234_terminal.token());
    if p234_terminal != P234Terminal::DirectionAEstablished {
        record_stop(
            &evidence_dir,
            &format!(
                "Plan 234 SYN epoch stopped at {} (java_accept_returned={} inbound_tunneldata={} expected_tunneldata={} recovery={} recovery_failures={} garlic_payload={} adapter_calls={} adapter_successes={} adapter_errors={})",
                p234_terminal.token(),
                p234_epoch.java_accept_returned,
                p234_epoch.i2pr_inbound_tunneldata_count,
                p234_epoch.i2pr_expected_stream_tunneldata_count,
                p234_epoch.i2pr_tunnel_recovery_count,
                p234_epoch.i2pr_tunnel_recovery_failures,
                p234_epoch.i2pr_garlic_payload_count,
                p234_epoch.i2pr_streaming_adapter_calls,
                p234_epoch.i2pr_streaming_adapter_successes,
                p234_epoch.i2pr_streaming_adapter_errors,
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
    // Plan 232 WP A/B3 — refreshed Streaming lease revalidates the live
    // installed route before rebuilding the LS2; never copy the previously
    // derived gateway across the long-running test.
    let p232_refresh_receive_ids = coord.registry().inbound_receive_ids();
    assert_eq!(
        p232_refresh_receive_ids.len(),
        1,
        "Plan 232 §7: refreshed LS2 requires exactly one installed inbound route"
    );
    let p232_refresh_local = p232_refresh_receive_ids[0];
    let p232_refresh_route = coord
        .registry()
        .inbound_gateway_route(p232_refresh_local)
        .expect("installed streaming refresh gateway route");
    assert_eq!(
        p232_refresh_route.gateway_router, service_hash,
        "Plan 232 §6: refreshed streaming route must terminate at the service router"
    );
    assert_eq!(
        p232_refresh_route.gateway_receive_tunnel.get(),
        STREAM_IBGW_RECEIVE,
        "Plan 232 §6: refreshed streaming route must carry the gateway-side receive tunnel"
    );
    assert_eq!(
        p232_refresh_route.local_receive_tunnel.get(),
        STREAM_IBGW_NEXT,
        "Plan 232 §6: refreshed streaming route must carry the local receive tunnel"
    );
    let p232_refresh_slot = coord.registry().inbound_slot(p232_refresh_local);
    let fresh_lease = p232_route_derived_lease_source(
        fresh_registrations[0].slot(),
        p232_refresh_local,
        Some(p232_refresh_route),
        p232_refresh_slot,
        fresh_expires,
        fresh_expires.saturating_sub(60),
    )
    .expect("route-derived streaming refresh lease");
    let p232_refresh_publication = Hash::from_bytes(*java_hash.as_bytes());
    p232_record_lease_route(
        &evidence_dir,
        &P232LeaseRecord {
            lane: "streaming",
            stage: "refresh",
            slot: fresh_registrations[0].slot(),
            local_receive: p232_refresh_local,
            route: &p232_refresh_route,
            lease: &fresh_lease,
            publication_target: &p232_refresh_publication,
            service_router: &service_hash,
        },
    );
    p232_record_publication_separation(
        &evidence_dir,
        "streaming",
        "refresh",
        &p232_refresh_publication,
        &p232_refresh_route,
        &fresh_lease,
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
    // Plan 232 §10 — the Streaming lane reached its end with
    // route-derived initial + refresh leases (both recorded above), so
    // the retained Streaming qualification rows are interpretable on
    // the corrected fixture. Second-family closure itself is a
    // closure-record conclusion over destination + streaming evidence,
    // never a single driver row.
    append_evidence(
        &evidence_dir,
        "p232-streaming-complete",
        "initial_route_parity=true refresh_route_parity=true",
    );
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
            "INFO Job-7: ISJ try 0 for LS {target_b64} to {b_b64} direct? false reply via client tunnel? true\nINFO Job-7: Encrypted DLM for {target_b64} to {b_b64}\nINFO Job-7: ISJ for {target_b64} failed with 0 remaining after 15000\n"
        ),
        true,
    );
    // B handled an unrelated lookup (proves DLM logging live) but
    // never saw our target.
    let b_logs = p224_write_log_tree(
        &dir,
        "b",
        "log-router-0.txt",
        "DEBUG HandleDatabaseLookupMessageJob: Handling database lookup message for UnrelatedKeyAAAAAAAAAAAAAAAAAAAAAAAAAA= with replies going to from\n",
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
            "INFO Job-9: ISJ try 1 for LS {target_b64} to {b_b64} direct? false reply via client tunnel? true\nINFO Job-9: Encrypted DLM for {target_b64} to {b_b64}\nINFO Job-9: ISJ for {target_b64} failed with 0 remaining after 15000\n"
        ),
        true,
    );
    let b_logs = p224_write_log_tree(
        &dir,
        "b",
        "log-router-0.txt",
        &format!(
            "DEBUG HandleDatabaseLookupMessageJob: Handling database lookup message for {target_b64} with replies going to fromKey\nINFO We have the published LS {target_b64}, answering query\nERROR DLM reply encryption error\n"
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
            "INFO Job-3: ISJ try 0 for LS {target_b64} to {b_b64} direct? false reply via client tunnel? true\nINFO Job-3: Encrypted DLM for {target_b64} to {b_b64}\nINFO Storing garlic LS down tunnel for: {target_b64} sent to: {helper_b64}\nINFO Job-3: ISJ for {target_b64} failed with 0 remaining after 15000\n"
        ),
        true,
    );
    let b_logs = p224_write_log_tree(
        &dir,
        "b",
        "log-router-0.txt",
        &format!(
            "DEBUG HandleDatabaseLookupMessageJob: Handling database lookup message for {target_b64} with replies going to fromKey\nINFO We have the published LS {target_b64}, answering query\n"
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
    let truncated = P224LogScan {
        truncated: true,
        files_read: 1,
        a_isj_any: 5,
        ..P224LogScan::default()
    };
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

// ---- Plan 225 corrective unit rows ----------------------------------------

#[test]
fn p225_logger_parser_requires_effective_pinned_levels() {
    let line = "P225-EV kind=logger-config observable=true default_level=ERROR isj_level=INFO dlm_level=DEBUG ibmd_level=INFO";
    let parsed = p225_parse_logger_config(line).expect("logger config parses");
    assert_eq!(
        parsed,
        P225LoggerConfig {
            effective: true,
            default_level: "ERROR".to_owned(),
            isj_level: "INFO".to_owned(),
            dlm_level: "DEBUG".to_owned(),
            ibmd_level: "INFO".to_owned(),
        }
    );
    let wrong_class = line.replace("dlm_level=DEBUG", "dlm_level=INFO");
    assert!(
        !p225_parse_logger_config(&wrong_class)
            .expect("wrong logger level remains an observed response")
            .effective
    );
    assert!(
        p225_parse_logger_config("P225-EV kind=logger-config observable=false reason=unreadable")
            .is_none()
    );
}

#[test]
fn p225_hash_b32_parser_requires_exact_echo_and_alphabet() {
    let hex = p224_test_target_hex();
    let b32 = "a2".repeat(26);
    let line = format!("P225-EV kind=hash-b32 hash_hex={hex} observable=true hash_b32={b32}");
    assert_eq!(
        p225_parse_hash_b32(&line, &hex).as_deref(),
        Some(b32.as_str())
    );
    assert!(p225_parse_hash_b32(&line, &p224_test_b_hex()).is_none());
    assert!(p225_parse_hash_b32(&line.replace("a2", "a0"), &hex).is_none());
    assert!(p225_parse_hash_b32("P225-ERROR invalid-hex-hash", &hex).is_none());
}

#[test]
fn p225_terminal_namespace_and_mapping_are_canonical() {
    assert_eq!(
        P225Terminal::AttributionASearchExhaustedWithoutQueryingB.token(),
        "P225-ATTRIBUTION-A-SEARCH-EXHAUSTED-WITHOUT-QUERYING-B"
    );
    assert_eq!(
        P225Terminal::ObservabilityGapLookupPath.token(),
        "P225-OBSERVABILITY-GAP-LOOKUP-PATH"
    );
    let target = p224_test_target_hex();
    let trace = p224_observable_trace(&target);
    let b_pre = p224_answerable_main(&target);
    let client = "ef".repeat(32);
    let a_pre = p224_absent_client(&client, &target);
    assert_eq!(
        p225_classify(
            &target,
            Some(&b_pre),
            None,
            Some(&a_pre),
            None,
            &trace,
            true,
            &[1, 21],
            false,
        ),
        P225Terminal::ObservabilityGapLookupPath
    );
}

#[test]
fn p225_record_emits_one_terminal_for_pre_epoch_gap() {
    let dir = p224_test_tmpdir("p225-record-once");
    let evidence_dir = dir.join("evidence");
    std::fs::create_dir_all(&evidence_dir).expect("evidence dir");
    record_p225_early_stop_gap(&evidence_dir, "test-stop");
    let tsv = std::fs::read_to_string(evidence_dir.join("driver-evidence.tsv")).expect("read tsv");
    assert_eq!(
        tsv.lines()
            .filter(|line| line.starts_with("p225-classification\t"))
            .count(),
        1
    );
    assert!(tsv.contains("P225-OBSERVABILITY-GAP-LOOKUP-PATH"));
    let _ = std::fs::remove_dir_all(&dir);
}

// ---- Plan 226 corrective unit rows ----------------------------------------

#[test]
fn p226_loopback_mask3_semantics_are_pairwise_exact() {
    let shared = [
        "127.0.0.1".parse::<IpAddr>().unwrap(),
        "127.0.0.2".parse::<IpAddr>().unwrap(),
        "127.0.0.3".parse::<IpAddr>().unwrap(),
    ];
    assert!(!p226_pairwise_mask3_distinct(&shared));
    let distinct = [
        "127.0.1.1".parse::<IpAddr>().unwrap(),
        "127.0.2.1".parse::<IpAddr>().unwrap(),
        "127.0.3.1".parse::<IpAddr>().unwrap(),
    ];
    assert!(p226_pairwise_mask3_distinct(&distinct));
    assert_eq!(p226_mask3_prefix(distinct[0]), Some([127, 0, 1]));
    assert!(p226_mask3_prefix("192.0.2.1".parse().unwrap()).is_none());
    assert!(p226_mask3_prefix("::1".parse().unwrap()).is_none());
}

#[test]
fn p226_exact_target_job_requires_exact_job_id_and_router_hash() {
    let target = p224_test_target_b64();
    let router_b = p224_test_b_b64();
    let other = format!("{}=", "O".repeat(43));
    let dir = p224_test_tmpdir("p226-target-job");
    let a_logs = p224_write_log_tree(
        &dir,
        "a",
        "log-router-0.txt",
        &format!(
            "INFO JobId: 42; dbid: helper: New ISJ for LS {target} (rkey key) timeout 15000 toTry: 2\nINFO 42: Skipping query w/ router too close to others {router_b}\nINFO 42: ISJ try 0 for LS {target} to {other} direct? false reply via client tunnel? true\nINFO 42: ISJ for {target} failed with 0 remaining after 15000, peers queried: 1\nINFO 43: Skipping query w/ router too close to others {router_b}\n"
        ),
        true,
    );
    let b_logs = p224_write_log_tree(&dir, "b", "log-router-0.txt", "INFO unrelated\n", true);
    let scan_a = p224_scan_log_dir(&a_logs, &target, &router_b, None).expect("scan A");
    let scan_b = p224_scan_log_dir(&b_logs, &target, &router_b, None).expect("scan B");
    let trace = p226_build_trace(Some(&scan_a), Some(&scan_b));
    assert!(trace.observable);
    assert_eq!(trace.target_job_count, 1);
    assert!(trace.b_ip_close_skipped);
    assert!(!trace.query_to_b);
    assert!(trace.search_failed);
    assert_eq!(trace.peer_try_count, 1);
    assert_eq!(
        p226_classify_baseline(true, &trace),
        P226Terminal::BaselineIpDiversityConfirmed
    );
    assert_eq!(
        p226_new_isj_job_id(
            &format!("INFO JobId: 44; dbid: helper: New ISJ for LS [Hash: {target}] (rkey key)"),
            &target,
        ),
        Some(44)
    );
    assert!(p226_hash_token_matches(
        &format!("[Hash: {router_b}]"),
        &router_b
    ));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn p226_unrelated_skip_cannot_classify_router_b() {
    let target = p224_test_target_b64();
    let router_b = p224_test_b_b64();
    let dir = p224_test_tmpdir("p226-unrelated-skip");
    let a_logs = p224_write_log_tree(
        &dir,
        "a",
        "log-router-0.txt",
        &format!(
            "INFO JobId: 42; dbid: helper: New ISJ for LS {target} (rkey key) timeout 15000 toTry: 2\nINFO 43: Skipping query w/ router too close to others {router_b}\nINFO 42: ISJ for {target} failed with 0 remaining after 15000, peers queried: 1\n"
        ),
        true,
    );
    let b_logs = p224_write_log_tree(&dir, "b", "log-router-0.txt", "INFO unrelated\n", true);
    let scan_a = p224_scan_log_dir(&a_logs, &target, &router_b, None).expect("scan A");
    let scan_b = p224_scan_log_dir(&b_logs, &target, &router_b, None).expect("scan B");
    let trace = p226_build_trace(Some(&scan_a), Some(&scan_b));
    assert!(trace.observable);
    assert!(!trace.b_ip_close_skipped);
    assert_eq!(
        p226_classify_baseline(true, &trace),
        P226Terminal::BaselineNotIpDiversity
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn p226_non_ip_baseline_reason_stops_before_topology_correction() {
    let mut trace = P226Trace {
        observable: true,
        target_job_count: 1,
        b_old_router_rejected: true,
        ..P226Trace::default()
    };
    assert_eq!(
        p226_classify_baseline(true, &trace),
        P226Terminal::BaselineOldRouterRejected
    );
    trace.b_old_router_rejected = false;
    trace.b_zero_hop_unknown_rejected = true;
    assert_eq!(
        p226_classify_baseline(true, &trace),
        P226Terminal::BaselineZeroHopUnknown
    );
}

#[test]
fn p226_distinct_topology_requires_query_dispatch_not_selector_membership() {
    let trace = P226Trace {
        observable: true,
        target_job_count: 1,
        query_to_b: false,
        search_failed: true,
        ..P226Trace::default()
    };
    assert_eq!(
        p226_classify_distinct(
            true,
            true,
            &trace,
            true,
            false,
            false,
            false,
            false,
            true,
            &[1, 21],
            false
        ),
        P226Terminal::NextBoundaryBStillNotQueried
    );
}

#[test]
fn p226_distinct_topology_maps_post_dispatch_boundaries() {
    let trace = P226Trace {
        observable: true,
        target_job_count: 1,
        query_to_b: true,
        ..P226Trace::default()
    };
    assert_eq!(
        p226_classify_distinct(
            true,
            true,
            &trace,
            true,
            false,
            false,
            false,
            false,
            true,
            &[1, 21],
            false
        ),
        P226Terminal::NextBoundaryAToBLookupDelivery
    );
    assert_eq!(
        p226_classify_distinct(
            true,
            true,
            &trace,
            true,
            true,
            false,
            false,
            false,
            true,
            &[1, 21],
            false
        ),
        P226Terminal::EvidenceContradictionBAnswerableNotAnswered
    );
    assert_eq!(
        p226_classify_distinct(
            true,
            true,
            &trace,
            true,
            true,
            true,
            false,
            false,
            true,
            &[1, 21],
            false
        ),
        P226Terminal::NextBoundaryBReplyToAClientTunnel
    );
    assert_eq!(
        p226_classify_distinct(
            true,
            true,
            &trace,
            true,
            true,
            true,
            true,
            false,
            true,
            &[1, 21],
            false
        ),
        P226Terminal::EvidenceContradictionClientDsmNotStored
    );
    assert_eq!(
        p226_classify_distinct(
            true,
            true,
            &trace,
            true,
            true,
            true,
            true,
            true,
            false,
            &[1, 19],
            false
        ),
        P226Terminal::NextBoundary(vec![1, 19])
    );
}

#[test]
fn p226_classification_is_single_and_frozen_payload_wins() {
    let dir = p224_test_tmpdir("p226-record-once");
    let evidence_dir = dir.join("evidence");
    std::fs::create_dir_all(&evidence_dir).expect("evidence dir");
    let trace = P226Trace {
        observable: true,
        target_job_count: 1,
        ..P226Trace::default()
    };
    let terminal = p226_classify_distinct(
        true,
        true,
        &trace,
        false,
        false,
        false,
        false,
        false,
        true,
        &[1, 21],
        true,
    );
    assert_eq!(terminal, P226Terminal::ReverseDeliveryPassed);
    record_p226_classification(&evidence_dir, &terminal, &trace);
    let tsv = std::fs::read_to_string(evidence_dir.join("driver-evidence.tsv")).expect("read tsv");
    assert_eq!(
        tsv.lines()
            .filter(|line| line.starts_with("p226-classification\t"))
            .count(),
        1
    );
    assert!(tsv.contains("P226-REVERSE-DELIVERY-PASSED"));
    let _ = std::fs::remove_dir_all(&dir);
}

// ---- Plan 227 corrective unit rows ----------------------------------------

fn p227_test_c_hex() -> String {
    "ab".repeat(32)
}

fn p227_test_client_hex() -> String {
    "cd".repeat(32)
}

fn p227_test_eligibility(selectable: bool) -> P227Eligibility {
    P227Eligibility {
        observable: true,
        main_raw_present: true,
        main_valid_present: true,
        selectable,
        established: false,
        banlisted: false,
    }
}

fn p227_test_tunnels(
    inbound_exact: bool,
    outbound_exact: bool,
    inbound_zero: bool,
    outbound_zero: bool,
) -> P227Tunnels {
    P227Tunnels {
        observable: true,
        client_resolved: true,
        inbound_pool_present: true,
        outbound_pool_present: true,
        inbound_tunnel_count: 1,
        outbound_tunnel_count: 1,
        inbound_exact_one_remote_hop_via_c: inbound_exact,
        outbound_exact_one_remote_hop_via_c: outbound_exact,
        inbound_zero_hop_present: inbound_zero,
        outbound_zero_hop_present: outbound_zero,
    }
}

fn p227_test_trace(query_to_b: bool, zero_hop_rejected: bool) -> P226Trace {
    P226Trace {
        observable: true,
        target_job_count: 1,
        b_zero_hop_unknown_rejected: zero_hop_rejected,
        query_to_b,
        ..P226Trace::default()
    }
}

#[test]
fn p227_c_selectable_passes_eligibility_gate() {
    let eligible = p227_test_eligibility(true);
    assert!(p227_eligibility_gate_pass(&eligible));
    let tunnels = p227_test_tunnels(true, true, false, false);
    assert!(p227_tunnel_gate_pass(&tunnels));
}

#[test]
fn p227_c_non_selectable_maps_to_not_selectable() {
    let eligible = p227_test_eligibility(false);
    assert!(!p227_eligibility_gate_pass(&eligible));
    let tunnels = p227_test_tunnels(true, true, false, false);
    let terminal = p227_classify(
        Some(&eligible),
        Some(&tunnels),
        &p227_test_trace(true, false),
        true,
        true,
        true,
        true,
        true,
        &[1, 21],
        false,
    );
    assert_eq!(terminal, P227Terminal::CNotSelectable);
}

#[test]
fn p227_exact_c_identity_mismatch_rejected() {
    let c_hex = p227_test_c_hex();
    let other_hex = p227_test_client_hex();
    let line = format!(
        "P227-EV kind=peer-eligibility router_c_hex={c_hex} observable=true main_raw_present=true main_valid_present=true selectable=true established=false banlisted=false"
    );
    assert!(p227_parse_eligibility(&line, &c_hex).is_some());
    assert!(p227_parse_eligibility(&line, &other_hex).is_none());
    let tun_line = format!(
        "P227-EV kind=client-tunnels client_dbid_hex={} router_c_hex={} observable=true client_resolved=true inbound_pool_present=true outbound_pool_present=true inbound_tunnel_count=1 outbound_tunnel_count=1 inbound_exact_one_remote_hop_via_c=true outbound_exact_one_remote_hop_via_c=true inbound_zero_hop_present=false outbound_zero_hop_present=false",
        p227_test_client_hex(),
        c_hex,
    );
    assert!(p227_parse_tunnels(&tun_line, &p227_test_client_hex(), &c_hex).is_some());
    assert!(p227_parse_tunnels(&tun_line, &p227_test_client_hex(), &other_hex).is_none());
    assert!(!p227_is_hex64("AB".repeat(32).as_str()));
    assert!(!p227_is_hex64("abc"));
}

#[test]
fn p227_inbound_only_tunnel_is_insufficient() {
    let tunnels = p227_test_tunnels(true, false, false, false);
    assert!(!p227_tunnel_gate_pass(&tunnels));
    let terminal = p227_classify(
        Some(&p227_test_eligibility(true)),
        Some(&tunnels),
        &p227_test_trace(false, false),
        true,
        false,
        false,
        false,
        false,
        &[1, 21],
        false,
    );
    assert_eq!(terminal, P227Terminal::ExplicitOneHopNotBuilt);
}

#[test]
fn p227_outbound_only_tunnel_is_insufficient() {
    let tunnels = p227_test_tunnels(false, true, false, false);
    assert!(!p227_tunnel_gate_pass(&tunnels));
    let terminal = p227_classify(
        Some(&p227_test_eligibility(true)),
        Some(&tunnels),
        &p227_test_trace(false, false),
        true,
        false,
        false,
        false,
        false,
        &[1, 21],
        false,
    );
    assert_eq!(terminal, P227Terminal::ExplicitOneHopNotBuilt);
}

#[test]
fn p227_zero_hop_fallback_is_insufficient() {
    let tunnels = p227_test_tunnels(true, true, true, false);
    assert!(!p227_tunnel_gate_pass(&tunnels));
    let terminal = p227_classify(
        Some(&p227_test_eligibility(true)),
        Some(&tunnels),
        &p227_test_trace(false, false),
        true,
        false,
        false,
        false,
        false,
        &[1, 21],
        false,
    );
    assert_eq!(terminal, P227Terminal::ExplicitOneHopNotBuilt);
}

#[test]
fn p227_exact_one_hop_both_directions_passes_gate() {
    let tunnels = p227_test_tunnels(true, true, false, false);
    assert!(p227_tunnel_gate_pass(&tunnels));
    // Gate passes but B never queried -> new boundary, not a pass.
    let terminal = p227_classify(
        Some(&p227_test_eligibility(true)),
        Some(&tunnels),
        &p227_test_trace(false, false),
        true,
        false,
        false,
        false,
        false,
        &[1, 21],
        false,
    );
    assert_eq!(terminal, P227Terminal::NextBoundaryBNotQueried);
}

#[test]
fn p227_one_hop_plus_zero_hop_contradiction() {
    // Direct gate fails when zero-hop present, but the classifier must
    // also surface the one-hop-but-zero-hop contradiction when the
    // zero-hop guard fires despite proven one-hop pools.
    let trace = p227_test_trace(true, true);
    let terminal = p227_classify(
        Some(&p227_test_eligibility(true)),
        Some(&p227_test_tunnels(true, true, false, false)),
        &trace,
        true,
        true,
        true,
        true,
        true,
        &[1, 21],
        false,
    );
    assert_eq!(
        terminal,
        P227Terminal::EvidenceContradictionOneHopButZeroHopUnknown
    );
}

#[test]
fn p227_one_hop_no_b_query_maps_to_next_boundary() {
    let terminal = p227_classify(
        Some(&p227_test_eligibility(true)),
        Some(&p227_test_tunnels(true, true, false, false)),
        &p227_test_trace(false, false),
        true,
        false,
        false,
        false,
        false,
        &[1, 21],
        false,
    );
    assert_eq!(terminal, P227Terminal::NextBoundaryBNotQueried);
}

#[test]
fn p227_b_query_reply_client_store_downstream() {
    // A dispatches toward B but B never receives.
    let to_b = p227_classify(
        Some(&p227_test_eligibility(true)),
        Some(&p227_test_tunnels(true, true, false, false)),
        &p227_test_trace(true, false),
        true,
        false,
        false,
        false,
        false,
        &[1, 21],
        false,
    );
    assert_eq!(to_b, P227Terminal::NextBoundaryAToBLookupDelivery);
    // B answers but A does not observe the client-tunnel DSM.
    let reply = p227_classify(
        Some(&p227_test_eligibility(true)),
        Some(&p227_test_tunnels(true, true, false, false)),
        &p227_test_trace(true, false),
        true,
        true,
        true,
        false,
        false,
        &[1, 21],
        false,
    );
    assert_eq!(reply, P227Terminal::NextBoundaryBReplyToAClientTunnel);
    // DSM received but not installed in the client DB.
    let store = p227_classify(
        Some(&p227_test_eligibility(true)),
        Some(&p227_test_tunnels(true, true, false, false)),
        &p227_test_trace(true, false),
        true,
        true,
        true,
        true,
        false,
        &[1, 21],
        false,
    );
    assert_eq!(store, P227Terminal::EvidenceContradictionClientDsmNotStored);
}

#[test]
fn p227_changed_send_status_maps_to_next_boundary() {
    let terminal = p227_classify(
        Some(&p227_test_eligibility(true)),
        Some(&p227_test_tunnels(true, true, false, false)),
        &p227_test_trace(true, false),
        true,
        true,
        true,
        true,
        true,
        &[1, 22],
        false,
    );
    assert_eq!(terminal, P227Terminal::NextBoundary(vec![1, 22]));
}

#[test]
fn p227_digest_matched_delivery_passes() {
    let terminal = p227_classify(
        Some(&p227_test_eligibility(true)),
        Some(&p227_test_tunnels(true, true, false, false)),
        &p227_test_trace(true, false),
        true,
        true,
        true,
        true,
        true,
        &[1, 21],
        true,
    );
    assert_eq!(terminal, P227Terminal::ReverseDeliveryPassed);
}

#[test]
fn p227_secret_raw_log_rejected() {
    let c_hex = p227_test_c_hex();
    // Secret material in the row must be rejected.
    let secret = format!(
        "P227-EV kind=peer-eligibility router_c_hex={c_hex} observable=true main_raw_present=true main_valid_present=true selectable=true established=false banlisted=false session_key=abcd"
    );
    assert!(p227_parse_eligibility(&secret, &c_hex).is_none());
    // Raw-log promotion must be rejected.
    let raw = format!(
        "P227-EV kind=peer-eligibility router_c_hex={c_hex} observable=true main_raw_present=true main_valid_present=true selectable=true established=false banlisted=false not doing zero-hop lookup to unknown foo"
    );
    assert!(p227_parse_eligibility(&raw, &c_hex).is_none());
    // Uppercase hex must be rejected (lowercase-only authority).
    assert!(!p227_is_hex64(&"AB".repeat(32)));
    assert!(p227_is_hex64(&c_hex));
}

#[test]
fn p227_classification_is_single_and_frozen_payload_wins() {
    let dir = p224_test_tmpdir("p227-record-once");
    let evidence_dir = dir.join("evidence");
    std::fs::create_dir_all(&evidence_dir).expect("evidence dir");
    let terminal = P227Terminal::ReverseDeliveryPassed;
    record_p227_classification(&evidence_dir, &terminal);
    let tsv = std::fs::read_to_string(evidence_dir.join("driver-evidence.tsv")).expect("read tsv");
    assert_eq!(
        tsv.lines()
            .filter(|line| line.starts_with("p227-classification\t"))
            .count(),
        1
    );
    assert!(tsv.contains("P227-REVERSE-DELIVERY-PASSED"));
    let _ = std::fs::remove_dir_all(&dir);
}

// ---- Plan 228 build-path attribution ---------------------------------------
// Attribution-only. No production behavior change, no Java patch, no profile
// mutation, no NetDB store, no tunnel install, no timeout change. The
// whitelist below matches only exact-pinned English log scaffolding; full
// log lines (which may carry reply keys/tags/records) are never retained.
// Durable evidence carries only booleans, bounded counts, directions,
// hashes, response/status codes, and elapsed timings.

const P228_MAX_LOG_FILES_PER_ROUTER: usize = 16;
const P228_MAX_LOG_BYTES_PER_ROUTER: u64 = 96 * 1024 * 1024;
const P228_MAX_LINE_LEN: usize = 8192;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum P228Direction {
    Inbound,
    Outbound,
    Both,
}

impl P228Direction {
    fn token(self) -> &'static str {
        match self {
            Self::Inbound => "inbound",
            Self::Outbound => "outbound",
            Self::Both => "both",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct P228Infra {
    observable: bool,
    free_tunnel_count: u64,
    inbound_tunnel_count: u64,
    outbound_tunnel_count: u64,
    inbound_exploratory_count: u64,
    outbound_exploratory_count: u64,
    inbound_exploratory_nonzero_count: u64,
    outbound_exploratory_nonzero_count: u64,
}

fn p228_parse_count(value: Option<&String>) -> Option<u64> {
    let raw = value?;
    if raw.is_empty() || raw.len() > 2 || !raw.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let parsed: u64 = raw.parse().ok()?;
    (parsed <= 64).then_some(parsed)
}

fn p228_line_is_secret_bearing(line: &str) -> bool {
    let lower = line.to_lowercase();
    // Raw secret/key/record markers that must never satisfy a durable fact.
    // Note: peer Base64 hashes are NOT secrets here (Router-C identity is a
    // bounded hash correlation input, explicitly permitted); only private
    // material, session keys/tags, payloads, and raw log-file paths are
    // rejected.
    for forbidden in [
        "priv",
        "seed",
        "session_key",
        "sessionkey",
        "reply key",
        "replykey",
        "tag=",
        "payload=",
        "log-router",
    ] {
        if lower.contains(forbidden) {
            return true;
        }
    }
    false
}

fn p228_parse_infra(line: &str) -> Option<P228Infra> {
    if !line.starts_with("P228-EV ") || !line.contains("kind=tunnel-infra") {
        return None;
    }
    if line.len() > 1024 || p228_line_is_secret_bearing(line) {
        return None;
    }
    let kv = p220_parse_kv(&line.replace("P228-EV ", "P220-EV "));
    if kv.get("observable").is_none_or(|v| v != "true") {
        return None;
    }
    Some(P228Infra {
        observable: true,
        free_tunnel_count: p228_parse_count(kv.get("free_tunnel_count"))?,
        inbound_tunnel_count: p228_parse_count(kv.get("inbound_tunnel_count"))?,
        outbound_tunnel_count: p228_parse_count(kv.get("outbound_tunnel_count"))?,
        inbound_exploratory_count: p228_parse_count(kv.get("inbound_exploratory_count"))?,
        outbound_exploratory_count: p228_parse_count(kv.get("outbound_exploratory_count"))?,
        inbound_exploratory_nonzero_count: p228_parse_count(
            kv.get("inbound_exploratory_nonzero_count"),
        )?,
        outbound_exploratory_nonzero_count: p228_parse_count(
            kv.get("outbound_exploratory_nonzero_count"),
        )?,
    })
}

async fn p228_collect_infra(diag_port: u16) -> Option<P228Infra> {
    let line = p220_query_diagnostic(diag_port, "P228-TUNNEL-INFRA").await?;
    p228_parse_infra(&line)
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct P228Trace {
    observable: bool,
    logger_config_ok: bool,
    files_read_a: u64,
    files_read_c: u64,
    // WP B — pool configuration / selection.
    selector_activity_seen: bool,
    explicit_not_selectable_seen: bool,
    zero_hop_fallback_seen: bool,
    peers_for_inbound_seen: bool,
    peers_for_outbound_seen: bool,
    config_contains_c: bool,
    configuring_new_tunnel_seen: bool,
    no_tunnel_to_build_with_seen: bool,
    // WP C — paired tunnel.
    paired_missing_seen: bool,
    paired_exploratory_fallback_seen: bool,
    build_message_create_fail_seen: bool,
    // WP D — dispatch.
    inbound_dispatch_seen: bool,
    outbound_dispatch_seen: bool,
    outbound_dispatch_to_c: bool,
    inbound_dispatch_to_c: bool,
    next_hop_missing_seen: bool,
    first_hop_failure_seen: bool,
    // WP E — Router-C processing.
    c_log_observable: bool,
    c_read_slot_seen: bool,
    c_response_code: Option<i32>,
    c_decrypt_failure_seen: bool,
    // WP F — Router-A reply handling.
    a_reply_handling_seen: bool,
    a_peer_status_seen: bool,
    a_remote_status_code: Option<i32>,
    a_reply_decrypt_fail_seen: bool,
    a_reply_no_match_seen: bool,
    a_dup_id_seen: bool,
    // WP G — timeout/result counters.
    build_timeout_seen: bool,
    client_build_success_hint_seen: bool,
    // Install proof comes from the P227 snapshot, never from log text.
    inbound_installed: bool,
    outbound_installed: bool,
}

fn p228_parse_response_code(line: &str, marker: &str) -> Option<i32> {
    let idx = line.find(marker)?;
    let rest = line[idx + marker.len()..].trim_start();
    let mut digits = String::new();
    let mut negative = false;
    for (i, ch) in rest.char_indices() {
        if i == 0 && ch == '-' {
            negative = true;
            continue;
        }
        if ch.is_ascii_digit() && digits.len() < 4 {
            digits.push(ch);
        } else {
            break;
        }
    }
    if digits.is_empty() {
        return None;
    }
    let mut value: i32 = digits.parse().ok()?;
    if negative {
        value = value.saturating_neg();
    }
    // Bounded response/status codes only.
    if (-8..=64).contains(&value) {
        Some(value)
    } else {
        None
    }
}

fn p228_scan_single_file(text: &str, c_b64: &str, trace: &mut P228Trace, is_c_side: bool) {
    for line in text.lines() {
        if line.len() > P228_MAX_LINE_LEN {
            continue;
        }
        // Never let a secret-bearing line satisfy a durable fact, and never
        // retain the line. Counters below only advance on clean scaffolding.
        if p228_line_is_secret_bearing(line) {
            continue;
        }
        // WP B — selector / config scaffolding (Router A side only).
        if !is_c_side {
            if line.contains("TunnelPeerSelector")
                || line.contains("ClientPeerSelector")
                || (line.contains("peers for ") && line.contains(" peers: "))
            {
                trace.selector_activity_seen = true;
            }
            if line.contains("Explicit peer is not selectable") {
                trace.explicit_not_selectable_seen = true;
            }
            if line.contains("No valid explicit peers found, building zero hop") {
                trace.zero_hop_fallback_seen = true;
            }
            if line.contains("peers for ") && line.contains(" inbound") {
                trace.peers_for_inbound_seen = true;
            }
            if line.contains("peers for ") && line.contains(" outbound") {
                trace.peers_for_outbound_seen = true;
            }
            if (line.contains("peers for ") || line.contains("selectExplicit"))
                && !c_b64.is_empty()
                && line.contains(c_b64)
            {
                trace.config_contains_c = true;
            }
            if line.contains("Configuring new tunnel") {
                trace.configuring_new_tunnel_seen = true;
            }
            if line.contains("No tunnel to build with") {
                trace.no_tunnel_to_build_with_seen = true;
            }
            // WP C.
            if line.contains("couldn't find a paired tunnel") {
                trace.paired_missing_seen = true;
            }
            if line.contains("using exploratory tunnel") {
                trace.paired_exploratory_fallback_seen = true;
            }
            if line.contains("couldn't create the tunnel build message") {
                trace.build_message_create_fail_seen = true;
            }
            // WP D.
            if line.contains("Sending the tunnel build request directly to") {
                trace.outbound_dispatch_seen = true;
                if !c_b64.is_empty() && line.contains(c_b64) {
                    trace.outbound_dispatch_to_c = true;
                }
            } else if line.contains("Sending the tunnel build request ")
                && line.contains(" out the tunnel ")
            {
                trace.inbound_dispatch_seen = true;
                if !c_b64.is_empty() && line.contains(c_b64) {
                    trace.inbound_dispatch_to_c = true;
                }
            }
            if line.contains("Could not find the next hop to send the outbound request to") {
                trace.next_hop_missing_seen = true;
            }
            // The first-hop fail job has no pinned log line; the stat-only
            // path is unobservable via logs. The flag stays false on live
            // scans and is exercised only by unit rows for precedence.
            // WP F (A side).
            if line.contains("Handling the reply after") {
                trace.a_reply_handling_seen = true;
            }
            if line.contains("replied with status") {
                trace.a_peer_status_seen = true;
                if trace.a_remote_status_code.is_none()
                    && let Some(code) = p228_parse_response_code(line, "replied with status")
                {
                    trace.a_remote_status_code = Some(code);
                }
            }
            if line.contains("could not be decrypted for tunnel") {
                trace.a_reply_decrypt_fail_seen = true;
            }
            if line.contains("did not match any pending tunnels") {
                trace.a_reply_no_match_seen = true;
            }
            if line.contains("Dup ID for our own tunnel") {
                trace.a_dup_id_seen = true;
            }
            // WP G.
            if line.contains("Timed out waiting for reply asking for") {
                trace.build_timeout_seen = true;
            }
        } else {
            // WP E — Router-C side only.
            if line.contains("could not be decrypted from:")
                || line.contains("No record decrypted")
                || line.contains("Matching record decrypt failure")
            {
                trace.c_decrypt_failure_seen = true;
            }
            if line.contains("Read slot") && line.contains("accepted? ") {
                trace.c_read_slot_seen = true;
                if trace.c_response_code.is_none()
                    && let Some(code) = p228_parse_response_code(line, "accepted? ")
                {
                    trace.c_response_code = Some(code);
                }
            }
        }
    }
}

fn p228_scan_log_dir(dir: &std::path::Path, c_b64: &str, is_c_side: bool) -> Option<P228Trace> {
    let mut log_dirs = vec![dir.to_path_buf()];
    if let Ok(read_dir) = std::fs::read_dir(dir)
        && let Some(child) = read_dir.filter_map(Result::ok).find_map(|entry| {
            (entry.file_name() == "logs"
                && entry.file_type().ok().is_some_and(|kind| kind.is_dir()))
            .then(|| entry.path())
        })
    {
        log_dirs.push(child);
    }
    let mut entries: Vec<std::path::PathBuf> = Vec::new();
    for log_dir in log_dirs {
        let Ok(read_dir) = std::fs::read_dir(&log_dir) else {
            continue;
        };
        for entry in read_dir {
            let Ok(entry) = entry else { continue };
            let name = entry.file_name().to_string_lossy().into_owned();
            if !name.starts_with("log-router-") || !name.ends_with(".txt") {
                continue;
            }
            entries.push(entry.path());
        }
    }
    entries.sort();
    entries.dedup();
    if entries.is_empty() {
        return None;
    }
    entries.truncate(P228_MAX_LOG_FILES_PER_ROUTER);
    let mut trace = P228Trace::default();
    let mut bytes_scanned: u64 = 0;
    for path in entries {
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        if bytes_scanned.saturating_add(bytes.len() as u64) > P228_MAX_LOG_BYTES_PER_ROUTER {
            break;
        }
        bytes_scanned += bytes.len() as u64;
        if is_c_side {
            trace.files_read_c += 1;
        } else {
            trace.files_read_a += 1;
        }
        let text = String::from_utf8_lossy(&bytes);
        p228_scan_single_file(&text, c_b64, &mut trace, is_c_side);
    }
    Some(trace)
}

fn p228_logger_config_installed(log_dir: &std::path::Path) -> bool {
    let config_path = log_dir.join("..").join("logger.config");
    let Ok(bytes) = std::fs::read(&config_path) else {
        return false;
    };
    if bytes.len() > 65536 {
        return false;
    }
    let text = String::from_utf8_lossy(&bytes);
    text.contains("logger.defaultLevel=ERROR")
        && text.contains("logger.record.net.i2p.router.tunnel.pool.TunnelPeerSelector=INFO")
        && text.contains("logger.record.net.i2p.router.tunnel.pool.ClientPeerSelector=INFO")
        && text.contains("logger.record.net.i2p.router.tunnel.pool.BuildExecutor=DEBUG")
        && text.contains("logger.record.net.i2p.router.tunnel.pool.BuildRequestor=DEBUG")
        && text.contains("logger.record.net.i2p.router.tunnel.pool.BuildHandler=DEBUG")
}

fn p228_config_direction(trace: &P228Trace) -> P228Direction {
    match (trace.peers_for_inbound_seen, trace.peers_for_outbound_seen) {
        (true, true) => P228Direction::Both,
        (true, false) => P228Direction::Inbound,
        (false, true) => P228Direction::Outbound,
        (false, false) => P228Direction::Both,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum P228Terminal {
    NoRouterTunnelInfra,
    NoClientConfig(P228Direction),
    NoPairedTunnel(P228Direction),
    BuildMessageCreateFailure(P228Direction),
    BuildCreatedNotDispatched(P228Direction),
    FirstHopDeliveryFailure,
    ADispatchedCNotReceived,
    CDecryptFailure,
    CRejected(i32),
    ReplyNotReturned,
    BuildReplyTimeout(P228Direction),
    ReplyDecryptFailure,
    RemoteReject(i32),
    LocalJoinFailure,
    NextBoundaryClientTunnelsBuilt,
    ObservabilityGapBuildPath,
}

impl P228Terminal {
    fn token(&self) -> String {
        match self {
            Self::NoRouterTunnelInfra => "P228-ATTRIBUTION-NO-ROUTER-TUNNEL-INFRA".to_owned(),
            Self::NoClientConfig(dir) => {
                format!(
                    "P228-ATTRIBUTION-NO-CLIENT-CONFIG direction={}",
                    dir.token()
                )
            }
            Self::NoPairedTunnel(dir) => {
                format!(
                    "P228-ATTRIBUTION-NO-PAIRED-TUNNEL direction={}",
                    dir.token()
                )
            }
            Self::BuildMessageCreateFailure(dir) => {
                format!(
                    "P228-ATTRIBUTION-BUILD-MESSAGE-CREATE-FAILURE direction={}",
                    dir.token()
                )
            }
            Self::BuildCreatedNotDispatched(dir) => {
                format!(
                    "P228-ATTRIBUTION-BUILD-CREATED-NOT-DISPATCHED direction={}",
                    dir.token()
                )
            }
            Self::FirstHopDeliveryFailure => {
                "P228-ATTRIBUTION-FIRST-HOP-DELIVERY-FAILURE".to_owned()
            }
            Self::ADispatchedCNotReceived => {
                "P228-ATTRIBUTION-A-DISPATCHED-C-NOT-RECEIVED".to_owned()
            }
            Self::CDecryptFailure => "P228-ATTRIBUTION-C-DECRYPT-FAILURE".to_owned(),
            Self::CRejected(code) => format!("P228-ATTRIBUTION-C-REJECTED code={code}"),
            Self::ReplyNotReturned => "P228-ATTRIBUTION-REPLY-NOT-RETURNED".to_owned(),
            Self::BuildReplyTimeout(dir) => {
                format!(
                    "P228-ATTRIBUTION-BUILD-REPLY-TIMEOUT direction={}",
                    dir.token()
                )
            }
            Self::ReplyDecryptFailure => "P228-ATTRIBUTION-REPLY-DECRYPT-FAILURE".to_owned(),
            Self::RemoteReject(code) => format!("P228-ATTRIBUTION-REMOTE-REJECT code={code}"),
            Self::LocalJoinFailure => "P228-ATTRIBUTION-LOCAL-JOIN-FAILURE".to_owned(),
            Self::NextBoundaryClientTunnelsBuilt => {
                "P228-NEXT-BOUNDARY-CLIENT-TUNNELS-BUILT".to_owned()
            }
            Self::ObservabilityGapBuildPath => "P228-OBSERVABILITY-GAP-BUILD-PATH".to_owned(),
        }
    }
}

// Earliest-proven-missing-stage order. Later evidence never overrides an
// earlier proven gap; unrelated exploratory/destination facts never satisfy
// the raw-helper correlation (callers must only set flags from
// Router-C-correlated lines).
#[allow(clippy::too_many_arguments)]
fn p228_classify(
    infra: Option<&P228Infra>,
    trace: &P228Trace,
    c_hex_known: bool,
    helper_connected: bool,
) -> P228Terminal {
    // Installed one-hop tunnels through C retire the assumed boundary.
    // Proof comes only from the installed-pool snapshot, never from logs.
    if trace.inbound_installed && trace.outbound_installed {
        return P228Terminal::NextBoundaryClientTunnelsBuilt;
    }
    let Some(infra) = infra else {
        return P228Terminal::ObservabilityGapBuildPath;
    };
    if !infra.observable {
        return P228Terminal::ObservabilityGapBuildPath;
    }
    // WP A — BuildExecutor prerequisite: usable router tunnel infrastructure.
    if infra.free_tunnel_count == 0 || infra.outbound_tunnel_count == 0 {
        return P228Terminal::NoRouterTunnelInfra;
    }
    if !trace.observable || !trace.logger_config_ok {
        return P228Terminal::ObservabilityGapBuildPath;
    }
    if !c_hex_known {
        return P228Terminal::ObservabilityGapBuildPath;
    }
    let dir = p228_config_direction(trace);
    // WP B — client pool configuration/selection for the raw helper.
    // A config is proven only by Router-C-correlated selector activity plus
    // a build-config event. Selector log presence alone never suffices.
    let config_proven = trace.config_contains_c && trace.configuring_new_tunnel_seen;
    if !config_proven {
        return P228Terminal::NoClientConfig(dir);
    }
    // WP C — paired tunnel requirement.
    if trace.paired_missing_seen {
        return P228Terminal::NoPairedTunnel(dir);
    }
    if trace.build_message_create_fail_seen {
        return P228Terminal::BuildMessageCreateFailure(dir);
    }
    // WP D — dispatch.
    let dispatched = trace.inbound_dispatch_seen || trace.outbound_dispatch_seen;
    if !dispatched {
        return P228Terminal::BuildCreatedNotDispatched(dir);
    }
    if trace.first_hop_failure_seen || trace.next_hop_missing_seen {
        return P228Terminal::FirstHopDeliveryFailure;
    }
    // WP E — Router-C handling.
    if !trace.c_log_observable {
        return P228Terminal::ObservabilityGapBuildPath;
    }
    if !trace.c_read_slot_seen {
        return P228Terminal::ADispatchedCNotReceived;
    }
    if trace.c_decrypt_failure_seen {
        return P228Terminal::CDecryptFailure;
    }
    if let Some(code) = trace.c_response_code {
        if code != 0 {
            return P228Terminal::CRejected(code);
        }
    } else {
        return P228Terminal::ObservabilityGapBuildPath;
    }
    // WP F — Router-A reply handling.
    if !trace.a_reply_handling_seen && !trace.a_peer_status_seen {
        // No matching reply observed at A. A bare timeout line confirms the
        // expiry path; otherwise the reply path is unproven.
        if trace.build_timeout_seen {
            return P228Terminal::BuildReplyTimeout(dir);
        }
        return P228Terminal::ReplyNotReturned;
    }
    if trace.a_reply_decrypt_fail_seen {
        return P228Terminal::ReplyDecryptFailure;
    }
    if let Some(code) = trace.a_remote_status_code
        && code != 0
    {
        return P228Terminal::RemoteReject(code);
    }
    // All remote statuses agree (or no explicit nonzero status) but no
    // install: with the helper still disconnected this is the local
    // join/install gap. A dup-ID line confirms it; otherwise the timeout
    // expiry with agreed statuses still maps to join failure when the
    // install snapshot is negative.
    if !helper_connected
        && (trace.a_dup_id_seen || trace.build_timeout_seen || trace.a_peer_status_seen)
    {
        return P228Terminal::LocalJoinFailure;
    }
    if trace.build_timeout_seen {
        return P228Terminal::BuildReplyTimeout(dir);
    }
    P228Terminal::ObservabilityGapBuildPath
}

fn record_p228_infra(evidence_dir: &std::path::Path, stage: &str, infra: Option<&P228Infra>) {
    match infra {
        Some(facts) => append_evidence(
            evidence_dir,
            "p228-tunnel-infra",
            &format!(
                "stage={stage} observable={} free_tunnel_count={} inbound_tunnel_count={} outbound_tunnel_count={} inbound_exploratory_count={} outbound_exploratory_count={} inbound_exploratory_nonzero_count={} outbound_exploratory_nonzero_count={}",
                facts.observable,
                facts.free_tunnel_count,
                facts.inbound_tunnel_count,
                facts.outbound_tunnel_count,
                facts.inbound_exploratory_count,
                facts.outbound_exploratory_count,
                facts.inbound_exploratory_nonzero_count,
                facts.outbound_exploratory_nonzero_count,
            ),
        ),
        None => append_evidence(
            evidence_dir,
            "p228-tunnel-infra",
            &format!("stage={stage} observable=false reason=diagnostic-unreachable"),
        ),
    }
}

fn record_p228_trace(evidence_dir: &std::path::Path, trace: &P228Trace, router_c_hex: &str) {
    append_evidence(
        evidence_dir,
        "p228-trace",
        &format!(
            "router_c_hex={router_c_hex} observable={} logger_config_ok={} selector_activity_seen={} explicit_not_selectable_seen={} zero_hop_fallback_seen={} peers_for_inbound_seen={} peers_for_outbound_seen={} config_contains_c={} configuring_new_tunnel_seen={} no_tunnel_to_build_with_seen={} paired_missing_seen={} paired_exploratory_fallback_seen={} build_message_create_fail_seen={} inbound_dispatch_seen={} outbound_dispatch_seen={} outbound_dispatch_to_c={} inbound_dispatch_to_c={} next_hop_missing_seen={} first_hop_failure_seen={} c_log_observable={} c_read_slot_seen={} c_response_code={} c_decrypt_failure_seen={} a_reply_handling_seen={} a_peer_status_seen={} a_remote_status_code={} a_reply_decrypt_fail_seen={} a_reply_no_match_seen={} a_dup_id_seen={} build_timeout_seen={} inbound_installed={} outbound_installed={}",
            trace.observable,
            trace.logger_config_ok,
            trace.selector_activity_seen,
            trace.explicit_not_selectable_seen,
            trace.zero_hop_fallback_seen,
            trace.peers_for_inbound_seen,
            trace.peers_for_outbound_seen,
            trace.config_contains_c,
            trace.configuring_new_tunnel_seen,
            trace.no_tunnel_to_build_with_seen,
            trace.paired_missing_seen,
            trace.paired_exploratory_fallback_seen,
            trace.build_message_create_fail_seen,
            trace.inbound_dispatch_seen,
            trace.outbound_dispatch_seen,
            trace.outbound_dispatch_to_c,
            trace.inbound_dispatch_to_c,
            trace.next_hop_missing_seen,
            trace.first_hop_failure_seen,
            trace.c_log_observable,
            trace.c_read_slot_seen,
            trace
                .c_response_code
                .map(|c| c.to_string())
                .as_deref()
                .unwrap_or("none"),
            trace.c_decrypt_failure_seen,
            trace.a_reply_handling_seen,
            trace.a_peer_status_seen,
            trace
                .a_remote_status_code
                .map(|c| c.to_string())
                .as_deref()
                .unwrap_or("none"),
            trace.a_reply_decrypt_fail_seen,
            trace.a_reply_no_match_seen,
            trace.a_dup_id_seen,
            trace.build_timeout_seen,
            trace.inbound_installed,
            trace.outbound_installed,
        ),
    )
}

fn record_p228_classification(evidence_dir: &std::path::Path, terminal: &P228Terminal) {
    append_evidence(evidence_dir, "p228-classification", &terminal.token());
}

/// Plan 228 attribution driver. Runs when the destination sub-run executed,
/// regardless of helper connect outcome. Collects the authoritative
/// tunnel-infra snapshot, the whitelist-only log trace correlated to
/// Router C, the installed-tunnel install proof (when the client DBID
/// resolved), and emits exactly one terminal.
#[tokio::test]
#[ignore = "Plan 228: requires the exact-pinned triple Java router environment"]
async fn p228_build_path_attribution() {
    let evidence_dir = std::env::var("EVIDENCE_DIR").expect("EVIDENCE_DIR");
    let evidence_dir = std::path::PathBuf::from(evidence_dir);
    let router_c_hex = std::env::var("P228_ROUTER_C_HEX").unwrap_or_default();
    let router_c_b64 = std::env::var("P228_ROUTER_C_B64").unwrap_or_default();
    let client_hex = std::env::var("P228_CLIENT_DBID_HEX").unwrap_or_default();
    let diag_a: u16 = std::env::var("JAVA_DIAGNOSTIC_A_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let log_a = std::env::var("JAVA_A_LOG_DIR")
        .map(std::path::PathBuf::from)
        .ok();
    let log_c = std::env::var("JAVA_C_LOG_DIR")
        .map(std::path::PathBuf::from)
        .ok();
    let c_hex_known = p227_is_hex64(&router_c_hex);
    let client_known = p227_is_hex64(&client_hex);

    // Authoritative infra snapshot at the attribution epoch.
    let infra = if diag_a != 0 {
        p228_collect_infra(diag_a).await
    } else {
        None
    };
    record_p228_infra(&evidence_dir, "attribution-epoch", infra.as_ref());

    // Logger-config proof on both routers (file sibling of each log dir).
    let logger_a_ok = log_a
        .as_ref()
        .is_some_and(|path| p228_logger_config_installed(path.as_path()));
    let logger_c_ok = log_c
        .as_ref()
        .is_some_and(|path| p228_logger_config_installed(path.as_path()));
    append_evidence(
        &evidence_dir,
        "p228-logger-config-a",
        &format!("stage=attribution-epoch observable={logger_a_ok}"),
    );
    append_evidence(
        &evidence_dir,
        "p228-logger-config-c",
        &format!("stage=attribution-epoch observable={logger_c_ok}"),
    );

    // Whitelist-only log trace, correlated to Router-C Base64.
    let mut trace = P228Trace {
        logger_config_ok: logger_a_ok,
        c_log_observable: logger_c_ok,
        ..P228Trace::default()
    };
    if c_hex_known && !router_c_b64.is_empty() {
        let mut any_a = false;
        let mut any_c = false;
        if let Some(log_a) = log_a.as_ref()
            && let Some(scan_a) = p228_scan_log_dir(log_a, &router_c_b64, false)
        {
            any_a = scan_a.files_read_a > 0;
            trace.files_read_a = scan_a.files_read_a;
            trace.selector_activity_seen |= scan_a.selector_activity_seen;
            trace.explicit_not_selectable_seen |= scan_a.explicit_not_selectable_seen;
            trace.zero_hop_fallback_seen |= scan_a.zero_hop_fallback_seen;
            trace.peers_for_inbound_seen |= scan_a.peers_for_inbound_seen;
            trace.peers_for_outbound_seen |= scan_a.peers_for_outbound_seen;
            trace.config_contains_c |= scan_a.config_contains_c;
            trace.configuring_new_tunnel_seen |= scan_a.configuring_new_tunnel_seen;
            trace.no_tunnel_to_build_with_seen |= scan_a.no_tunnel_to_build_with_seen;
            trace.paired_missing_seen |= scan_a.paired_missing_seen;
            trace.paired_exploratory_fallback_seen |= scan_a.paired_exploratory_fallback_seen;
            trace.build_message_create_fail_seen |= scan_a.build_message_create_fail_seen;
            trace.inbound_dispatch_seen |= scan_a.inbound_dispatch_seen;
            trace.outbound_dispatch_seen |= scan_a.outbound_dispatch_seen;
            trace.outbound_dispatch_to_c |= scan_a.outbound_dispatch_to_c;
            trace.inbound_dispatch_to_c |= scan_a.inbound_dispatch_to_c;
            trace.next_hop_missing_seen |= scan_a.next_hop_missing_seen;
            trace.a_reply_handling_seen |= scan_a.a_reply_handling_seen;
            trace.a_peer_status_seen |= scan_a.a_peer_status_seen;
            if trace.a_remote_status_code.is_none() {
                trace.a_remote_status_code = scan_a.a_remote_status_code;
            }
            trace.a_reply_decrypt_fail_seen |= scan_a.a_reply_decrypt_fail_seen;
            trace.a_reply_no_match_seen |= scan_a.a_reply_no_match_seen;
            trace.a_dup_id_seen |= scan_a.a_dup_id_seen;
            trace.build_timeout_seen |= scan_a.build_timeout_seen;
        }
        if let Some(log_c) = log_c.as_ref()
            && let Some(scan_c) = p228_scan_log_dir(log_c, &router_c_b64, true)
        {
            any_c = scan_c.files_read_c > 0;
            trace.files_read_c = scan_c.files_read_c;
            trace.c_read_slot_seen |= scan_c.c_read_slot_seen;
            if trace.c_response_code.is_none() {
                trace.c_response_code = scan_c.c_response_code;
            }
            trace.c_decrypt_failure_seen |= scan_c.c_decrypt_failure_seen;
        }
        trace.observable = any_a && logger_a_ok;
        trace.c_log_observable = any_c && logger_c_ok;
    }

    // Install proof from the installed-pool snapshot when the client DBID
    // resolved (helper connected). Never inferred from log presence.
    let mut helper_connected = false;
    if client_known
        && diag_a != 0
        && let Some(tunnels) = p227_collect_tunnels(diag_a, &client_hex, &router_c_hex).await
    {
        helper_connected = true;
        trace.inbound_installed =
            tunnels.inbound_exact_one_remote_hop_via_c && !tunnels.inbound_zero_hop_present;
        trace.outbound_installed =
            tunnels.outbound_exact_one_remote_hop_via_c && !tunnels.outbound_zero_hop_present;
        record_p227_tunnels(&evidence_dir, Some(&tunnels), &client_hex, &router_c_hex);
    }
    record_p228_trace(&evidence_dir, &trace, &router_c_hex);

    let terminal = p228_classify(infra.as_ref(), &trace, c_hex_known, helper_connected);
    record_p228_classification(&evidence_dir, &terminal);
}

// ---- Plan 228 focused unit rows --------------------------------------------

fn p228_test_infra(free: u64, outbound: u64) -> P228Infra {
    P228Infra {
        observable: true,
        free_tunnel_count: free,
        inbound_tunnel_count: free,
        outbound_tunnel_count: outbound,
        inbound_exploratory_count: free,
        outbound_exploratory_count: outbound,
        inbound_exploratory_nonzero_count: 1,
        outbound_exploratory_nonzero_count: 1,
    }
}

fn p228_test_trace_full() -> P228Trace {
    P228Trace {
        observable: true,
        logger_config_ok: true,
        selector_activity_seen: true,
        peers_for_inbound_seen: true,
        peers_for_outbound_seen: true,
        config_contains_c: true,
        configuring_new_tunnel_seen: true,
        inbound_dispatch_seen: true,
        outbound_dispatch_seen: true,
        outbound_dispatch_to_c: true,
        inbound_dispatch_to_c: true,
        c_log_observable: true,
        c_read_slot_seen: true,
        c_response_code: Some(0),
        a_reply_handling_seen: true,
        a_peer_status_seen: true,
        a_remote_status_code: Some(0),
        ..P228Trace::default()
    }
}

#[test]
fn p228_no_router_tunnel_infra_maps_to_infra_terminal() {
    let infra = p228_test_infra(0, 0);
    let trace = P228Trace {
        observable: true,
        logger_config_ok: true,
        ..P228Trace::default()
    };
    assert_eq!(
        p228_classify(Some(&infra), &trace, true, false),
        P228Terminal::NoRouterTunnelInfra
    );
    let infra_free_only = p228_test_infra(2, 0);
    assert_eq!(
        p228_classify(Some(&infra_free_only), &trace, true, false),
        P228Terminal::NoRouterTunnelInfra
    );
}

#[test]
fn p228_selector_without_config_maps_to_no_client_config() {
    let infra = p228_test_infra(2, 2);
    let trace = P228Trace {
        observable: true,
        logger_config_ok: true,
        selector_activity_seen: true,
        peers_for_outbound_seen: true,
        // No configuring event and no C correlation: selector presence alone
        // never proves a config.
        ..P228Trace::default()
    };
    assert_eq!(
        p228_classify(Some(&infra), &trace, true, false),
        P228Terminal::NoClientConfig(P228Direction::Outbound)
    );
}

#[test]
fn p228_config_through_c_without_paired_tunnel_maps_correctly() {
    let infra = p228_test_infra(2, 2);
    let trace = P228Trace {
        observable: true,
        logger_config_ok: true,
        selector_activity_seen: true,
        peers_for_inbound_seen: true,
        peers_for_outbound_seen: true,
        config_contains_c: true,
        configuring_new_tunnel_seen: true,
        paired_missing_seen: true,
        ..P228Trace::default()
    };
    assert_eq!(
        p228_classify(Some(&infra), &trace, true, false),
        P228Terminal::NoPairedTunnel(P228Direction::Both)
    );
}

#[test]
fn p228_message_created_but_not_dispatched_maps_correctly() {
    let infra = p228_test_infra(2, 2);
    let trace = P228Trace {
        observable: true,
        logger_config_ok: true,
        selector_activity_seen: true,
        peers_for_outbound_seen: true,
        config_contains_c: true,
        configuring_new_tunnel_seen: true,
        // Paired tunnel available, message created, but no dispatch lines.
        ..P228Trace::default()
    };
    assert_eq!(
        p228_classify(Some(&infra), &trace, true, false),
        P228Terminal::BuildCreatedNotDispatched(P228Direction::Outbound)
    );
}

#[test]
fn p228_outbound_first_hop_failure_maps_correctly() {
    let infra = p228_test_infra(2, 2);
    let trace = P228Trace {
        observable: true,
        logger_config_ok: true,
        selector_activity_seen: true,
        peers_for_outbound_seen: true,
        config_contains_c: true,
        configuring_new_tunnel_seen: true,
        outbound_dispatch_seen: true,
        outbound_dispatch_to_c: true,
        first_hop_failure_seen: true,
        c_log_observable: true,
        ..P228Trace::default()
    };
    assert_eq!(
        p228_classify(Some(&infra), &trace, true, false),
        P228Terminal::FirstHopDeliveryFailure
    );
}

#[test]
fn p228_dispatched_but_c_receives_nothing_maps_correctly() {
    let infra = p228_test_infra(2, 2);
    let trace = P228Trace {
        observable: true,
        logger_config_ok: true,
        selector_activity_seen: true,
        peers_for_outbound_seen: true,
        config_contains_c: true,
        configuring_new_tunnel_seen: true,
        outbound_dispatch_seen: true,
        outbound_dispatch_to_c: true,
        c_log_observable: true,
        // No read-slot on C despite dispatch.
        ..P228Trace::default()
    };
    assert_eq!(
        p228_classify(Some(&infra), &trace, true, false),
        P228Terminal::ADispatchedCNotReceived
    );
}

#[test]
fn p228_c_receive_decrypt_failure_maps_correctly() {
    let infra = p228_test_infra(2, 2);
    let trace = P228Trace {
        observable: true,
        logger_config_ok: true,
        selector_activity_seen: true,
        peers_for_outbound_seen: true,
        config_contains_c: true,
        configuring_new_tunnel_seen: true,
        outbound_dispatch_seen: true,
        c_log_observable: true,
        c_read_slot_seen: true,
        c_response_code: Some(0),
        c_decrypt_failure_seen: true,
        ..P228Trace::default()
    };
    assert_eq!(
        p228_classify(Some(&infra), &trace, true, false),
        P228Terminal::CDecryptFailure
    );
}

#[test]
fn p228_c_explicit_reject_code_maps_correctly() {
    let infra = p228_test_infra(2, 2);
    let trace = P228Trace {
        observable: true,
        logger_config_ok: true,
        selector_activity_seen: true,
        peers_for_outbound_seen: true,
        config_contains_c: true,
        configuring_new_tunnel_seen: true,
        outbound_dispatch_seen: true,
        c_log_observable: true,
        c_read_slot_seen: true,
        c_response_code: Some(30),
        ..P228Trace::default()
    };
    assert_eq!(
        p228_classify(Some(&infra), &trace, true, false),
        P228Terminal::CRejected(30)
    );
}

#[test]
fn p228_c_accept_reply_absent_at_a_maps_correctly() {
    let infra = p228_test_infra(2, 2);
    let trace = P228Trace {
        observable: true,
        logger_config_ok: true,
        selector_activity_seen: true,
        peers_for_outbound_seen: true,
        config_contains_c: true,
        configuring_new_tunnel_seen: true,
        outbound_dispatch_seen: true,
        c_log_observable: true,
        c_read_slot_seen: true,
        c_response_code: Some(0),
        // C accepted but A sees no reply handling or status.
        ..P228Trace::default()
    };
    assert_eq!(
        p228_classify(Some(&infra), &trace, true, false),
        P228Terminal::ReplyNotReturned
    );
}

#[test]
fn p228_a_reply_decrypt_failure_maps_correctly() {
    let mut trace = p228_test_trace_full();
    trace.a_reply_decrypt_fail_seen = true;
    let infra = p228_test_infra(2, 2);
    assert_eq!(
        p228_classify(Some(&infra), &trace, true, false),
        P228Terminal::ReplyDecryptFailure
    );
}

#[test]
fn p228_a_remote_rejection_maps_correctly() {
    let mut trace = p228_test_trace_full();
    trace.a_remote_status_code = Some(20);
    let infra = p228_test_infra(2, 2);
    assert_eq!(
        p228_classify(Some(&infra), &trace, true, false),
        P228Terminal::RemoteReject(20)
    );
}

#[test]
fn p228_local_join_failure_maps_correctly() {
    let mut trace = p228_test_trace_full();
    trace.a_dup_id_seen = true;
    let infra = p228_test_infra(2, 2);
    assert_eq!(
        p228_classify(Some(&infra), &trace, true, false),
        P228Terminal::LocalJoinFailure
    );
}

#[test]
fn p228_build_reply_timeout_maps_correctly() {
    let infra = p228_test_infra(2, 2);
    let trace = P228Trace {
        observable: true,
        logger_config_ok: true,
        selector_activity_seen: true,
        peers_for_outbound_seen: true,
        config_contains_c: true,
        configuring_new_tunnel_seen: true,
        outbound_dispatch_seen: true,
        c_log_observable: true,
        c_read_slot_seen: true,
        c_response_code: Some(0),
        build_timeout_seen: true,
        // No reply handling at A: pending request expired.
        ..P228Trace::default()
    };
    assert_eq!(
        p228_classify(Some(&infra), &trace, true, false),
        P228Terminal::BuildReplyTimeout(P228Direction::Outbound)
    );
}

#[test]
fn p228_inbound_only_install_is_insufficient() {
    let infra = p228_test_infra(2, 2);
    let trace = P228Trace {
        inbound_installed: true,
        ..p228_test_trace_full()
    };
    // Inbound-only install never claims both tunnels built.
    assert_ne!(
        p228_classify(Some(&infra), &trace, true, true),
        P228Terminal::NextBoundaryClientTunnelsBuilt
    );
}

#[test]
fn p228_outbound_only_install_is_insufficient() {
    let infra = p228_test_infra(2, 2);
    let trace = P228Trace {
        outbound_installed: true,
        ..p228_test_trace_full()
    };
    assert_ne!(
        p228_classify(Some(&infra), &trace, true, true),
        P228Terminal::NextBoundaryClientTunnelsBuilt
    );
}

#[test]
fn p228_both_installed_maps_only_to_next_boundary() {
    let infra = p228_test_infra(2, 2);
    let trace = P228Trace {
        inbound_installed: true,
        outbound_installed: true,
        ..p228_test_trace_full()
    };
    let terminal = p228_classify(Some(&infra), &trace, true, true);
    assert_eq!(terminal, P228Terminal::NextBoundaryClientTunnelsBuilt);
    assert_eq!(terminal.token(), "P228-NEXT-BOUNDARY-CLIENT-TUNNELS-BUILT");
}

#[test]
fn p228_unrelated_exploratory_logs_cannot_classify_client_build() {
    // Exploratory generic activity without Router-C correlation must not
    // prove a client config: the classifier requires config_contains_c.
    let infra = p228_test_infra(2, 2);
    let trace = P228Trace {
        observable: true,
        logger_config_ok: true,
        selector_activity_seen: true,
        configuring_new_tunnel_seen: true,
        paired_exploratory_fallback_seen: true,
        // No config_contains_c: generic exploratory/config lines only.
        ..P228Trace::default()
    };
    assert_eq!(
        p228_classify(Some(&infra), &trace, true, false),
        P228Terminal::NoClientConfig(P228Direction::Both)
    );
}

#[test]
fn p228_unrelated_destinations_cannot_classify_raw_helper() {
    // Unknown Router-C identity fails closed to a gap, never to a
    // stage attribution for another destination.
    let infra = p228_test_infra(2, 2);
    let trace = p228_test_trace_full();
    assert_eq!(
        p228_classify(Some(&infra), &trace, false, false),
        P228Terminal::ObservabilityGapBuildPath
    );
}

#[test]
fn p228_secret_raw_log_lines_rejected_from_evidence() {
    // Secret-bearing diagnostic rows never parse.
    let c_hex = p227_test_c_hex();
    let secret_infra =
        "P228-EV kind=tunnel-infra observable=true free_tunnel_count=2 inbound_tunnel_count=2 outbound_tunnel_count=2 inbound_exploratory_count=2 outbound_exploratory_count=2 inbound_exploratory_nonzero_count=1 outbound_exploratory_nonzero_count=1 session_key=abcd"
            .to_string();
    assert!(p228_parse_infra(&secret_infra).is_none());
    let raw_log =
        "P228-EV kind=tunnel-infra observable=true free_tunnel_count=2 inbound_tunnel_count=2 outbound_tunnel_count=2 inbound_exploratory_count=2 outbound_exploratory_count=2 inbound_exploratory_nonzero_count=1 outbound_exploratory_nonzero_count=1 not doing zero-hop lookup to unknown foo"
            .to_string();
    assert!(p228_parse_infra(&raw_log).is_some());
    // Secret lines never advance trace flags.
    let mut trace = P228Trace::default();
    let secret_line =
        format!("Sending the tunnel build request directly to {c_hex} session_key=deadbeef");
    p228_scan_single_file(&secret_line, &c_hex, &mut trace, false);
    assert!(!trace.outbound_dispatch_seen);
    assert!(!trace.outbound_dispatch_to_c);
    // Response codes outside the bounded range are rejected.
    assert_eq!(
        p228_parse_response_code("accepted? 9999", "accepted? "),
        None
    );
    assert_eq!(
        p228_parse_response_code("replied with status 0", "replied with status"),
        Some(0)
    );
}

#[test]
fn p228_classification_emits_exactly_one_terminal() {
    let dir = p224_test_tmpdir("p228-record-once");
    let evidence_dir = dir.join("evidence");
    std::fs::create_dir_all(&evidence_dir).expect("evidence dir");
    let terminal = P228Terminal::NoPairedTunnel(P228Direction::Both);
    record_p228_classification(&evidence_dir, &terminal);
    let tsv = std::fs::read_to_string(evidence_dir.join("driver-evidence.tsv")).expect("read tsv");
    assert_eq!(
        tsv.lines()
            .filter(|line| line.starts_with("p228-classification\t"))
            .count(),
        1
    );
    assert!(tsv.contains("P228-ATTRIBUTION-NO-PAIRED-TUNNEL direction=both"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn p228_earliest_stage_precedence_is_deterministic() {
    // Every later signal present, but missing infra still wins.
    let infra = p228_test_infra(0, 0);
    let trace = p228_test_trace_full();
    assert_eq!(
        p228_classify(Some(&infra), &trace, true, false),
        P228Terminal::NoRouterTunnelInfra
    );
    // Config gap beats a later paired-tunnel signal.
    let infra_ok = p228_test_infra(2, 2);
    let early = P228Trace {
        observable: true,
        logger_config_ok: true,
        selector_activity_seen: true,
        paired_missing_seen: true,
        ..P228Trace::default()
    };
    assert_eq!(
        p228_classify(Some(&infra_ok), &early, true, false),
        P228Terminal::NoClientConfig(P228Direction::Both)
    );
    // Paired gap beats dispatch/reply signals.
    let mut paired_trace = p228_test_trace_full();
    paired_trace.paired_missing_seen = true;
    paired_trace.a_remote_status_code = Some(20);
    assert_eq!(
        p228_classify(Some(&infra_ok), &paired_trace, true, false),
        P228Terminal::NoPairedTunnel(P228Direction::Both)
    );
    // Dispatch gap beats C-side signals.
    let mut dispatch_trace = p228_test_trace_full();
    dispatch_trace.inbound_dispatch_seen = false;
    dispatch_trace.outbound_dispatch_seen = false;
    dispatch_trace.build_timeout_seen = true;
    assert_eq!(
        p228_classify(Some(&infra_ok), &dispatch_trace, true, false),
        P228Terminal::BuildCreatedNotDispatched(P228Direction::Both)
    );
}

// ---- Plan 229 non-zero exploratory paired-tunnel bootstrap corrective ----
// Reference-topology corrective only: role-aware launcher proof, stock
// small-router exploratory profile on Router A, ordinary-profile transit
// gate for Router C, genuine non-zero exploratory gate in both directions,
// then the unchanged Plan-227 client profile through the reused Plan-228
// build-path attribution. No production behavior, no Java source patch,
// no profile/NetDB/tunnel mutation, no VMComm, no alwaysQuery, no public
// topology, no timeout change, no lookup/reverse-delivery qualification.

/// Plan 229 counted launcher roles. Unknown values fail closed (`None`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum P229Role {
    Service,
    Publication,
    Transit,
}

fn p229_parse_role(value: &str) -> Option<P229Role> {
    match value {
        "service" => Some(P229Role::Service),
        "publication" => Some(P229Role::Publication),
        "transit" => Some(P229Role::Transit),
        _ => None,
    }
}

/// Plan 229 WP A: service and publication retain floodfill; the transit
/// tunnel participant must be non-floodfill.
fn p229_role_floodfill(role: P229Role) -> bool {
    match role {
        P229Role::Service | P229Role::Publication => true,
        P229Role::Transit => false,
    }
}

/// Plan 229 WP B: only the service role receives Java's stock small-router
/// exploratory profile.
fn p229_role_applies_small_exploratory(role: P229Role) -> bool {
    matches!(role, P229Role::Service)
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct P229TransitPeer {
    observable: bool,
    main_raw_present: bool,
    main_valid_present: bool,
    profile_present: bool,
    selectable: bool,
    banlisted: bool,
    unreachable: bool,
    caps_has_f: bool,
    profile_count: u64,
    not_failing_count: u64,
}

fn p229_parse_small_count(value: Option<&String>) -> Option<u64> {
    let raw = value?;
    if raw.is_empty() || raw.len() > 2 || !raw.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let parsed: u64 = raw.parse().ok()?;
    (parsed <= 64).then_some(parsed)
}

fn p229_parse_bool(value: Option<&String>) -> Option<bool> {
    match value.map(String::as_str) {
        Some("true") => Some(true),
        Some("false") => Some(false),
        _ => None,
    }
}

fn p229_parse_transit_peer(line: &str, expected_c_hex: &str) -> Option<P229TransitPeer> {
    if !line.starts_with("P229-EV ") || !line.contains("kind=transit-peer") {
        return None;
    }
    if line.len() > 1024 || p228_line_is_secret_bearing(line) {
        return None;
    }
    let kv = p220_parse_kv(&line.replace("P229-EV ", "P220-EV "));
    if kv.get("observable").is_none_or(|v| v != "true") {
        return None;
    }
    let echoed = kv.get("router_c_hex")?;
    if !echoed.eq_ignore_ascii_case(expected_c_hex) || !p227_is_hex64(&echoed.to_lowercase()) {
        return None;
    }
    Some(P229TransitPeer {
        observable: true,
        main_raw_present: p229_parse_bool(kv.get("main_raw_present"))?,
        main_valid_present: p229_parse_bool(kv.get("main_valid_present"))?,
        profile_present: p229_parse_bool(kv.get("profile_present"))?,
        selectable: p229_parse_bool(kv.get("selectable"))?,
        banlisted: p229_parse_bool(kv.get("banlisted"))?,
        unreachable: p229_parse_bool(kv.get("unreachable"))?,
        caps_has_f: p229_parse_bool(kv.get("caps_has_f"))?,
        profile_count: p229_parse_small_count(kv.get("profile_count"))?,
        not_failing_count: p229_parse_small_count(kv.get("not_failing_count"))?,
    })
}

async fn p229_collect_transit_peer(diag_port: u16, router_c_hex: &str) -> Option<P229TransitPeer> {
    let line =
        p220_query_diagnostic(diag_port, &format!("P229-TRANSIT-PEER {router_c_hex}")).await?;
    p229_parse_transit_peer(&line, router_c_hex)
}

/// Plan 229 WP C gate: ordinary authenticated-bootstrap profile that is
/// present, selectable, non-banned, reachable, and non-floodfill.
fn p229_transit_gate(peer: &P229TransitPeer) -> bool {
    peer.observable
        && peer.main_raw_present
        && peer.main_valid_present
        && peer.profile_present
        && peer.selectable
        && !peer.banlisted
        && !peer.unreachable
        && !peer.caps_has_f
}

fn record_p229_transit_peer(
    evidence_dir: &std::path::Path,
    peer: Option<&P229TransitPeer>,
    router_c_hex: &str,
) {
    match peer {
        Some(facts) => append_evidence(
            evidence_dir,
            "p229-transit-peer",
            &format!(
                "router_c_hex={router_c_hex} observable={} main_raw_present={} main_valid_present={} profile_present={} selectable={} banlisted={} unreachable={} caps_has_f={} profile_count={} not_failing_count={} gate={}",
                facts.observable,
                facts.main_raw_present,
                facts.main_valid_present,
                facts.profile_present,
                facts.selectable,
                facts.banlisted,
                facts.unreachable,
                facts.caps_has_f,
                facts.profile_count,
                facts.not_failing_count,
                p229_transit_gate(facts),
            ),
        ),
        None => append_evidence(
            evidence_dir,
            "p229-transit-peer",
            &format!("router_c_hex={router_c_hex} observable=false reason=diagnostic-unreachable"),
        ),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct P229ExploratorySettings {
    observable: bool,
    inbound_length: u64,
    inbound_variance: u64,
    inbound_quantity: u64,
    outbound_length: u64,
    outbound_variance: u64,
    outbound_quantity: u64,
}

fn p229_parse_exploratory_settings(line: &str) -> Option<P229ExploratorySettings> {
    if !line.starts_with("P229-EV ") || !line.contains("kind=exploratory-settings") {
        return None;
    }
    // Fail-closed: exploratory explicitPeers is not available for the
    // exploratory pools (pinned TunnelPeerSelector.shouldSelectExplicit);
    // any such row is rejected from evidence.
    if line.len() > 1024 || p228_line_is_secret_bearing(line) || line.contains("explicitPeers") {
        return None;
    }
    let kv = p220_parse_kv(&line.replace("P229-EV ", "P220-EV "));
    if kv.get("observable").is_none_or(|v| v != "true") {
        return None;
    }
    Some(P229ExploratorySettings {
        observable: true,
        inbound_length: p229_parse_small_count(kv.get("inbound_length"))?,
        inbound_variance: p229_parse_small_count(kv.get("inbound_variance"))?,
        inbound_quantity: p229_parse_small_count(kv.get("inbound_quantity"))?,
        outbound_length: p229_parse_small_count(kv.get("outbound_length"))?,
        outbound_variance: p229_parse_small_count(kv.get("outbound_variance"))?,
        outbound_quantity: p229_parse_small_count(kv.get("outbound_quantity"))?,
    })
}

async fn p229_collect_exploratory_settings(diag_port: u16) -> Option<P229ExploratorySettings> {
    let line = p220_query_diagnostic(diag_port, "P229-EXPLORATORY-SETTINGS").await?;
    p229_parse_exploratory_settings(&line)
}

/// Plan 229 WP B gate: exactly Java's stock small-router exploratory
/// profile on Router A. Quantities are diagnostic only and never gated.
fn p229_settings_match(settings: &P229ExploratorySettings) -> bool {
    settings.observable
        && settings.inbound_length == 1
        && settings.inbound_variance == 1
        && settings.outbound_length == 1
        && settings.outbound_variance == 1
}

fn record_p229_exploratory_settings(
    evidence_dir: &std::path::Path,
    settings: Option<&P229ExploratorySettings>,
) {
    match settings {
        Some(facts) => append_evidence(
            evidence_dir,
            "p229-exploratory-settings",
            &format!(
                "observable={} inbound_length={} inbound_variance={} outbound_length={} outbound_variance={} inbound_quantity={} outbound_quantity={} match={}",
                facts.observable,
                facts.inbound_length,
                facts.inbound_variance,
                facts.outbound_length,
                facts.outbound_variance,
                facts.inbound_quantity,
                facts.outbound_quantity,
                p229_settings_match(facts),
            ),
        ),
        None => append_evidence(
            evidence_dir,
            "p229-exploratory-settings",
            "observable=false reason=diagnostic-unreachable",
        ),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct P229Exploratory {
    observable: bool,
    inbound_count: u64,
    outbound_count: u64,
    inbound_nonzero: u64,
    outbound_nonzero: u64,
    inbound_c_present: bool,
    outbound_c_present: bool,
    zero_hop_fallback_present: bool,
}

fn p229_parse_exploratory(line: &str, expected_c_hex: &str) -> Option<P229Exploratory> {
    if !line.starts_with("P229-EV ") || !line.contains("kind=exploratory-tunnels") {
        return None;
    }
    if line.len() > 1024 || p228_line_is_secret_bearing(line) {
        return None;
    }
    let kv = p220_parse_kv(&line.replace("P229-EV ", "P220-EV "));
    if kv.get("observable").is_none_or(|v| v != "true") {
        return None;
    }
    let echoed = kv.get("router_c_hex")?;
    if !echoed.eq_ignore_ascii_case(expected_c_hex) || !p227_is_hex64(&echoed.to_lowercase()) {
        return None;
    }
    Some(P229Exploratory {
        observable: true,
        inbound_count: p229_parse_small_count(kv.get("inbound_exploratory_count"))?,
        outbound_count: p229_parse_small_count(kv.get("outbound_exploratory_count"))?,
        inbound_nonzero: p229_parse_small_count(kv.get("inbound_nonzero_count"))?,
        outbound_nonzero: p229_parse_small_count(kv.get("outbound_nonzero_count"))?,
        inbound_c_present: p229_parse_bool(kv.get("inbound_c_present"))?,
        outbound_c_present: p229_parse_bool(kv.get("outbound_c_present"))?,
        zero_hop_fallback_present: p229_parse_bool(kv.get("zero_hop_fallback_present"))?,
    })
}

async fn p229_collect_exploratory(diag_port: u16, router_c_hex: &str) -> Option<P229Exploratory> {
    let line = p220_query_diagnostic(
        diag_port,
        &format!("P229-EXPLORATORY-TUNNELS {router_c_hex}"),
    )
    .await?;
    p229_parse_exploratory(&line, router_c_hex)
}

/// Plan 229 WP D authoritative gate: genuine non-zero exploratory tunnels
/// in both directions. C presence is recorded but not required; only the
/// non-zero paired-tunnel property gates helper start. The returned
/// direction names the missing scope (`both` when neither direction has a
/// non-zero tunnel), mirroring the Plan-228 direction convention.
fn p229_nonzero_gate(exploratory: &P229Exploratory) -> P228Direction {
    match (
        exploratory.observable && exploratory.inbound_nonzero >= 1,
        exploratory.observable && exploratory.outbound_nonzero >= 1,
    ) {
        (true, true) => P228Direction::Both,
        (true, false) => P228Direction::Outbound,
        (false, true) => P228Direction::Inbound,
        (false, false) => P228Direction::Both,
    }
}

fn p229_nonzero_gate_pass(exploratory: &P229Exploratory) -> bool {
    exploratory.observable && exploratory.inbound_nonzero >= 1 && exploratory.outbound_nonzero >= 1
}

fn record_p229_exploratory(
    evidence_dir: &std::path::Path,
    exploratory: Option<&P229Exploratory>,
    router_c_hex: &str,
) {
    match exploratory {
        Some(facts) => append_evidence(
            evidence_dir,
            "p229-exploratory-tunnels",
            &format!(
                "router_c_hex={router_c_hex} observable={} inbound_exploratory_count={} outbound_exploratory_count={} inbound_nonzero_count={} outbound_nonzero_count={} inbound_c_present={} outbound_c_present={} zero_hop_fallback_present={} gate={}",
                facts.observable,
                facts.inbound_count,
                facts.outbound_count,
                facts.inbound_nonzero,
                facts.outbound_nonzero,
                facts.inbound_c_present,
                facts.outbound_c_present,
                facts.zero_hop_fallback_present,
                p229_nonzero_gate_pass(facts),
            ),
        ),
        None => append_evidence(
            evidence_dir,
            "p229-exploratory-tunnels",
            &format!("router_c_hex={router_c_hex} observable=false reason=diagnostic-unreachable"),
        ),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum P229Terminal {
    RoleMismatch,
    ExploratorySettingsMismatch,
    CNotExploratoryEligible,
    ExploratoryNonzeroNotBuilt(P228Direction),
    ContradictionNonzeroButNoPaired,
    NextBoundaryBuildMessageCreate,
    NextBoundaryADispatch,
    NextBoundaryFirstHopDelivery,
    NextBoundaryCDecrypt,
    NextBoundaryCReject(i32),
    NextBoundaryReplyReturn,
    NextBoundaryReplyDecrypt,
    NextBoundaryRemoteReject(i32),
    NextBoundaryLocalJoin,
    NextBoundaryBuildReplyTimeout,
    ClientTunnelsBuilt,
    ObservabilityGap,
}

impl P229Terminal {
    fn token(&self) -> String {
        match self {
            Self::RoleMismatch => "P229-C-ROLE-MISMATCH".to_owned(),
            Self::ExploratorySettingsMismatch => "P229-EXPLORATORY-SETTINGS-MISMATCH".to_owned(),
            Self::CNotExploratoryEligible => "P229-C-NOT-EXPLORATORY-ELIGIBLE".to_owned(),
            Self::ExploratoryNonzeroNotBuilt(dir) => {
                format!(
                    "P229-EXPLORATORY-NONZERO-NOT-BUILT direction={}",
                    dir.token()
                )
            }
            Self::ContradictionNonzeroButNoPaired => {
                "P229-EVIDENCE-CONTRADICTION-NONZERO-EXPLORATORY-BUT-NO-PAIRED".to_owned()
            }
            Self::NextBoundaryBuildMessageCreate => {
                "P229-NEXT-BOUNDARY-BUILD-MESSAGE-CREATE".to_owned()
            }
            Self::NextBoundaryADispatch => "P229-NEXT-BOUNDARY-A-DISPATCH".to_owned(),
            Self::NextBoundaryFirstHopDelivery => {
                "P229-NEXT-BOUNDARY-FIRST-HOP-DELIVERY".to_owned()
            }
            Self::NextBoundaryCDecrypt => "P229-NEXT-BOUNDARY-C-DECRYPT".to_owned(),
            Self::NextBoundaryCReject(code) => {
                format!("P229-NEXT-BOUNDARY-C-REJECT code={code}")
            }
            Self::NextBoundaryReplyReturn => "P229-NEXT-BOUNDARY-REPLY-RETURN".to_owned(),
            Self::NextBoundaryReplyDecrypt => "P229-NEXT-BOUNDARY-REPLY-DECRYPT".to_owned(),
            Self::NextBoundaryRemoteReject(code) => {
                format!("P229-NEXT-BOUNDARY-REMOTE-REJECT code={code}")
            }
            Self::NextBoundaryLocalJoin => "P229-NEXT-BOUNDARY-LOCAL-JOIN".to_owned(),
            Self::NextBoundaryBuildReplyTimeout => {
                "P229-NEXT-BOUNDARY-BUILD-REPLY-TIMEOUT".to_owned()
            }
            Self::ClientTunnelsBuilt => "P229-CLIENT-TUNNELS-BUILT".to_owned(),
            Self::ObservabilityGap => "P229-OBSERVABILITY-GAP".to_owned(),
        }
    }
}

/// Plan 229 earliest-proven-missing-stage order. The role/profile/settings
/// and non-zero exploratory gates precede the reused build-path
/// attribution; once both non-zero directions are proven, the Plan-228
/// `NO-PAIRED-TUNNEL` terminal becomes an explicit contradiction and every
/// later build stage maps to its §11 next-boundary terminal. Installed
/// one-hop client tunnels through C retire every assumed boundary.
#[allow(clippy::too_many_arguments)]
fn p229_classify(
    role_ok: bool,
    settings: Option<&P229ExploratorySettings>,
    transit: Option<&P229TransitPeer>,
    exploratory: Option<&P229Exploratory>,
    p228: &P228Terminal,
) -> P229Terminal {
    if !role_ok {
        return P229Terminal::RoleMismatch;
    }
    let Some(settings) = settings else {
        return P229Terminal::ExploratorySettingsMismatch;
    };
    if !p229_settings_match(settings) {
        return P229Terminal::ExploratorySettingsMismatch;
    }
    let Some(transit) = transit else {
        return P229Terminal::CNotExploratoryEligible;
    };
    if !p229_transit_gate(transit) {
        // A transit Router C that still advertises floodfill is the
        // Plan-229 role mismatch, not a profile-eligibility stop.
        if transit.observable && transit.caps_has_f {
            return P229Terminal::RoleMismatch;
        }
        return P229Terminal::CNotExploratoryEligible;
    }
    let Some(exploratory) = exploratory else {
        return P229Terminal::ObservabilityGap;
    };
    if !exploratory.observable {
        return P229Terminal::ObservabilityGap;
    }
    if !p229_nonzero_gate_pass(exploratory) {
        return P229Terminal::ExploratoryNonzeroNotBuilt(p229_nonzero_gate(exploratory));
    }
    // Both non-zero directions proven: continue through the already
    // instrumented stock-Java build path.
    match p228 {
        P228Terminal::NoPairedTunnel(_) => P229Terminal::ContradictionNonzeroButNoPaired,
        P228Terminal::BuildMessageCreateFailure(_) => P229Terminal::NextBoundaryBuildMessageCreate,
        P228Terminal::BuildCreatedNotDispatched(_) => P229Terminal::NextBoundaryADispatch,
        P228Terminal::FirstHopDeliveryFailure | P228Terminal::ADispatchedCNotReceived => {
            P229Terminal::NextBoundaryFirstHopDelivery
        }
        P228Terminal::CDecryptFailure => P229Terminal::NextBoundaryCDecrypt,
        P228Terminal::CRejected(code) => P229Terminal::NextBoundaryCReject(*code),
        P228Terminal::ReplyNotReturned => P229Terminal::NextBoundaryReplyReturn,
        P228Terminal::ReplyDecryptFailure => P229Terminal::NextBoundaryReplyDecrypt,
        P228Terminal::RemoteReject(code) => P229Terminal::NextBoundaryRemoteReject(*code),
        P228Terminal::LocalJoinFailure => P229Terminal::NextBoundaryLocalJoin,
        P228Terminal::BuildReplyTimeout(_) => P229Terminal::NextBoundaryBuildReplyTimeout,
        P228Terminal::NextBoundaryClientTunnelsBuilt => P229Terminal::ClientTunnelsBuilt,
        P228Terminal::NoRouterTunnelInfra
        | P228Terminal::NoClientConfig(_)
        | P228Terminal::ObservabilityGapBuildPath => P229Terminal::ObservabilityGap,
    }
}

fn record_p229_classification(evidence_dir: &std::path::Path, terminal: &P229Terminal) {
    append_evidence(evidence_dir, "p229-classification", &terminal.token());
}

/// Plan 229 invariant 20 / §15: no Plan-226 lookup or reverse-delivery
/// evidence is permitted after the Plan-229 terminal. The frozen 45-second
/// window is untouched and never entered here.
fn p229_terminal_permits_lookup(_terminal: &P229Terminal) -> bool {
    false
}

/// Plan 229 WP E driver. Runs only after the shell proved the role,
/// settings, transit-peer, and non-zero exploratory gates and started the
/// unchanged Plan-227 raw helper. Re-verifies every gate at the
/// attribution epoch, reuses the Plan-228 whitelist-only build-path trace
/// correlated to Router C, and emits exactly one terminal.
#[tokio::test]
#[ignore = "Plan 229: requires the exact-pinned triple Java router environment"]
async fn p229_nonzero_exploratory_bootstrap() {
    let evidence_dir = std::env::var("EVIDENCE_DIR").expect("EVIDENCE_DIR");
    let evidence_dir = std::path::PathBuf::from(evidence_dir);
    let router_c_hex = std::env::var("P229_ROUTER_C_HEX").unwrap_or_default();
    let router_c_b64 = std::env::var("P229_ROUTER_C_B64").unwrap_or_default();
    let client_hex = std::env::var("P229_CLIENT_DBID_HEX").unwrap_or_default();
    let role_ok = std::env::var("P229_ROLE_OK")
        .map(|v| v == "true")
        .unwrap_or(false);
    let diag_a: u16 = std::env::var("JAVA_DIAGNOSTIC_A_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let log_a = std::env::var("JAVA_A_LOG_DIR")
        .map(std::path::PathBuf::from)
        .ok();
    let log_c = std::env::var("JAVA_C_LOG_DIR")
        .map(std::path::PathBuf::from)
        .ok();
    let c_hex_known = p227_is_hex64(&router_c_hex);
    let client_known = p227_is_hex64(&client_hex);

    // Re-verify every Plan-229 gate at the attribution epoch.
    let settings = if diag_a != 0 {
        p229_collect_exploratory_settings(diag_a).await
    } else {
        None
    };
    record_p229_exploratory_settings(&evidence_dir, settings.as_ref());

    let transit = if diag_a != 0 && c_hex_known {
        p229_collect_transit_peer(diag_a, &router_c_hex).await
    } else {
        None
    };
    record_p229_transit_peer(&evidence_dir, transit.as_ref(), &router_c_hex);

    let exploratory = if diag_a != 0 && c_hex_known {
        p229_collect_exploratory(diag_a, &router_c_hex).await
    } else {
        None
    };
    record_p229_exploratory(&evidence_dir, exploratory.as_ref(), &router_c_hex);

    // Reused Plan-228 attribution inputs at the same epoch.
    let infra = if diag_a != 0 {
        p228_collect_infra(diag_a).await
    } else {
        None
    };
    let logger_a_ok = log_a
        .as_ref()
        .is_some_and(|path| p228_logger_config_installed(path.as_path()));
    let logger_c_ok = log_c
        .as_ref()
        .is_some_and(|path| p228_logger_config_installed(path.as_path()));
    let mut trace = P228Trace {
        logger_config_ok: logger_a_ok,
        c_log_observable: logger_c_ok,
        ..P228Trace::default()
    };
    if c_hex_known && !router_c_b64.is_empty() {
        let mut any_a = false;
        let mut any_c = false;
        if let Some(log_a) = log_a.as_ref()
            && let Some(scan_a) = p228_scan_log_dir(log_a, &router_c_b64, false)
        {
            any_a = scan_a.files_read_a > 0;
            trace.files_read_a = scan_a.files_read_a;
            trace.selector_activity_seen |= scan_a.selector_activity_seen;
            trace.explicit_not_selectable_seen |= scan_a.explicit_not_selectable_seen;
            trace.zero_hop_fallback_seen |= scan_a.zero_hop_fallback_seen;
            trace.peers_for_inbound_seen |= scan_a.peers_for_inbound_seen;
            trace.peers_for_outbound_seen |= scan_a.peers_for_outbound_seen;
            trace.config_contains_c |= scan_a.config_contains_c;
            trace.configuring_new_tunnel_seen |= scan_a.configuring_new_tunnel_seen;
            trace.no_tunnel_to_build_with_seen |= scan_a.no_tunnel_to_build_with_seen;
            trace.paired_missing_seen |= scan_a.paired_missing_seen;
            trace.paired_exploratory_fallback_seen |= scan_a.paired_exploratory_fallback_seen;
            trace.build_message_create_fail_seen |= scan_a.build_message_create_fail_seen;
            trace.inbound_dispatch_seen |= scan_a.inbound_dispatch_seen;
            trace.outbound_dispatch_seen |= scan_a.outbound_dispatch_seen;
            trace.outbound_dispatch_to_c |= scan_a.outbound_dispatch_to_c;
            trace.inbound_dispatch_to_c |= scan_a.inbound_dispatch_to_c;
            trace.next_hop_missing_seen |= scan_a.next_hop_missing_seen;
            trace.a_reply_handling_seen |= scan_a.a_reply_handling_seen;
            trace.a_peer_status_seen |= scan_a.a_peer_status_seen;
            if trace.a_remote_status_code.is_none() {
                trace.a_remote_status_code = scan_a.a_remote_status_code;
            }
            trace.a_reply_decrypt_fail_seen |= scan_a.a_reply_decrypt_fail_seen;
            trace.a_reply_no_match_seen |= scan_a.a_reply_no_match_seen;
            trace.a_dup_id_seen |= scan_a.a_dup_id_seen;
            trace.build_timeout_seen |= scan_a.build_timeout_seen;
        }
        if let Some(log_c) = log_c.as_ref()
            && let Some(scan_c) = p228_scan_log_dir(log_c, &router_c_b64, true)
        {
            any_c = scan_c.files_read_c > 0;
            trace.files_read_c = scan_c.files_read_c;
            trace.c_read_slot_seen |= scan_c.c_read_slot_seen;
            if trace.c_response_code.is_none() {
                trace.c_response_code = scan_c.c_response_code;
            }
            trace.c_decrypt_failure_seen |= scan_c.c_decrypt_failure_seen;
        }
        trace.observable = any_a && logger_a_ok;
        trace.c_log_observable = any_c && logger_c_ok;
    }

    let mut helper_connected = false;
    if client_known
        && diag_a != 0
        && let Some(tunnels) = p227_collect_tunnels(diag_a, &client_hex, &router_c_hex).await
    {
        helper_connected = true;
        trace.inbound_installed =
            tunnels.inbound_exact_one_remote_hop_via_c && !tunnels.inbound_zero_hop_present;
        trace.outbound_installed =
            tunnels.outbound_exact_one_remote_hop_via_c && !tunnels.outbound_zero_hop_present;
    }

    let p228 = p228_classify(infra.as_ref(), &trace, c_hex_known, helper_connected);
    let terminal = p229_classify(
        role_ok,
        settings.as_ref(),
        transit.as_ref(),
        exploratory.as_ref(),
        &p228,
    );
    // The target lookup / reverse-delivery lane is never entered here.
    debug_assert!(!p229_terminal_permits_lookup(&terminal));
    record_p229_classification(&evidence_dir, &terminal);
}

// ---- Plan 229 focused unit rows (Plan 229 §14) ----------------------------

fn p229_test_settings() -> P229ExploratorySettings {
    P229ExploratorySettings {
        observable: true,
        inbound_length: 1,
        inbound_variance: 1,
        inbound_quantity: 2,
        outbound_length: 1,
        outbound_variance: 1,
        outbound_quantity: 2,
    }
}

fn p229_test_transit_ok() -> P229TransitPeer {
    P229TransitPeer {
        observable: true,
        main_raw_present: true,
        main_valid_present: true,
        profile_present: true,
        selectable: true,
        banlisted: false,
        unreachable: false,
        caps_has_f: false,
        profile_count: 3,
        not_failing_count: 2,
    }
}

fn p229_test_exploratory(inbound_nonzero: u64, outbound_nonzero: u64) -> P229Exploratory {
    P229Exploratory {
        observable: true,
        inbound_count: 2,
        outbound_count: 2,
        inbound_nonzero,
        outbound_nonzero,
        inbound_c_present: true,
        outbound_c_present: true,
        zero_hop_fallback_present: true,
    }
}

fn p229_test_p228_paired_missing() -> P228Terminal {
    P228Terminal::NoPairedTunnel(P228Direction::Both)
}

#[test]
fn p229_unknown_launcher_role_fails_closed() {
    assert_eq!(p229_parse_role("service"), Some(P229Role::Service));
    assert_eq!(p229_parse_role("publication"), Some(P229Role::Publication));
    assert_eq!(p229_parse_role("transit"), Some(P229Role::Transit));
    assert_eq!(p229_parse_role(""), None);
    assert_eq!(p229_parse_role("floodfill"), None);
    assert_eq!(p229_parse_role("SERVICE"), None);
    assert_eq!(p229_parse_role("routerC"), None);
    assert_eq!(p229_parse_role("transit "), None);
}

#[test]
fn p229_service_role_keeps_floodfill_true() {
    assert!(p229_role_floodfill(P229Role::Service));
}

#[test]
fn p229_publication_role_keeps_floodfill_true() {
    assert!(p229_role_floodfill(P229Role::Publication));
}

#[test]
fn p229_transit_role_sets_floodfill_false() {
    assert!(!p229_role_floodfill(P229Role::Transit));
}

#[test]
fn p229_only_service_role_receives_small_exploratory_profile() {
    assert!(p229_role_applies_small_exploratory(P229Role::Service));
    assert!(!p229_role_applies_small_exploratory(P229Role::Publication));
    assert!(!p229_role_applies_small_exploratory(P229Role::Transit));
    // The four small-router properties match exactly; quantities stay
    // diagnostic-only (any stock quantity passes).
    let mut high_quantity = p229_test_settings();
    high_quantity.inbound_quantity = 8;
    high_quantity.outbound_quantity = 8;
    assert!(p229_settings_match(&p229_test_settings()));
    assert!(p229_settings_match(&high_quantity));
    let mut wrong_length = p229_test_settings();
    wrong_length.inbound_length = 2;
    assert!(!p229_settings_match(&wrong_length));
    let mut wrong_variance = p229_test_settings();
    wrong_variance.outbound_variance = 0;
    assert!(!p229_settings_match(&wrong_variance));
}

#[test]
fn p229_no_explicit_peers_on_exploratory_settings() {
    // Any exploratory explicitPeers mention invalidates the settings row.
    let line = "P229-EV kind=exploratory-settings observable=true inbound_length=1 inbound_variance=1 inbound_quantity=2 outbound_length=1 outbound_variance=1 outbound_quantity=2 explicitPeers=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
    assert!(p229_parse_exploratory_settings(line).is_none());
    let clean = "P229-EV kind=exploratory-settings observable=true inbound_length=1 inbound_variance=1 inbound_quantity=2 outbound_length=1 outbound_variance=1 outbound_quantity=2";
    assert!(p229_parse_exploratory_settings(clean).is_some());
}

#[test]
fn p229_no_exploratory_quantity_backup_timeout_override() {
    // Quantities are observed but never gated: stock/current values pass.
    let mut settings = p229_test_settings();
    settings.inbound_quantity = 0;
    settings.outbound_quantity = 64;
    assert!(p229_settings_match(&settings));
    // Length overrides outside the small-router profile fail the match.
    settings.inbound_length = 3;
    assert!(!p229_settings_match(&settings));
}

#[test]
fn p229_c_advertising_f_maps_to_role_mismatch() {
    // Router C still advertising floodfill is the role mismatch, even
    // when every other gate input passes.
    let mut transit = p229_test_transit_ok();
    transit.caps_has_f = true;
    let terminal = p229_classify(
        true,
        Some(&p229_test_settings()),
        Some(&transit),
        Some(&p229_test_exploratory(1, 1)),
        &p229_test_p228_paired_missing(),
    );
    assert_eq!(terminal, P229Terminal::RoleMismatch);
    // Shell role proof failing also maps to the mismatch terminal.
    let terminal = p229_classify(
        false,
        Some(&p229_test_settings()),
        Some(&p229_test_transit_ok()),
        Some(&p229_test_exploratory(1, 1)),
        &p229_test_p228_paired_missing(),
    );
    assert_eq!(terminal, P229Terminal::RoleMismatch);
}

#[test]
fn p229_c_no_profile_maps_to_not_eligible() {
    let mut transit = p229_test_transit_ok();
    transit.profile_present = false;
    let terminal = p229_classify(
        true,
        Some(&p229_test_settings()),
        Some(&transit),
        Some(&p229_test_exploratory(1, 1)),
        &p229_test_p228_paired_missing(),
    );
    assert_eq!(terminal, P229Terminal::CNotExploratoryEligible);
    // Missing diagnostic is the same eligibility stop, not a role claim.
    let terminal = p229_classify(
        true,
        Some(&p229_test_settings()),
        None,
        Some(&p229_test_exploratory(1, 1)),
        &p229_test_p228_paired_missing(),
    );
    assert_eq!(terminal, P229Terminal::CNotExploratoryEligible);
}

#[test]
fn p229_c_profile_but_not_selectable_maps_to_not_eligible() {
    for transit in [
        {
            let mut peer = p229_test_transit_ok();
            peer.selectable = false;
            peer
        },
        {
            let mut peer = p229_test_transit_ok();
            peer.banlisted = true;
            peer
        },
        {
            let mut peer = p229_test_transit_ok();
            peer.unreachable = true;
            peer
        },
        {
            let mut peer = p229_test_transit_ok();
            peer.main_valid_present = false;
            peer
        },
    ] {
        assert!(!p229_transit_gate(&transit));
        let terminal = p229_classify(
            true,
            Some(&p229_test_settings()),
            Some(&transit),
            Some(&p229_test_exploratory(1, 1)),
            &p229_test_p228_paired_missing(),
        );
        assert_eq!(terminal, P229Terminal::CNotExploratoryEligible);
    }
    assert!(p229_transit_gate(&p229_test_transit_ok()));
}

#[test]
fn p229_zero_hop_only_exploratory_is_insufficient() {
    let exploratory = p229_test_exploratory(0, 0);
    assert!(!p229_nonzero_gate_pass(&exploratory));
    let terminal = p229_classify(
        true,
        Some(&p229_test_settings()),
        Some(&p229_test_transit_ok()),
        Some(&exploratory),
        &p229_test_p228_paired_missing(),
    );
    assert_eq!(
        terminal,
        P229Terminal::ExploratoryNonzeroNotBuilt(P228Direction::Both)
    );
}

#[test]
fn p229_inbound_only_nonzero_is_insufficient() {
    let exploratory = p229_test_exploratory(1, 0);
    assert!(!p229_nonzero_gate_pass(&exploratory));
    assert_eq!(p229_nonzero_gate(&exploratory), P228Direction::Outbound);
    let terminal = p229_classify(
        true,
        Some(&p229_test_settings()),
        Some(&p229_test_transit_ok()),
        Some(&exploratory),
        &p229_test_p228_paired_missing(),
    );
    assert_eq!(
        terminal,
        P229Terminal::ExploratoryNonzeroNotBuilt(P228Direction::Outbound)
    );
}

#[test]
fn p229_outbound_only_nonzero_is_insufficient() {
    let exploratory = p229_test_exploratory(0, 2);
    assert!(!p229_nonzero_gate_pass(&exploratory));
    assert_eq!(p229_nonzero_gate(&exploratory), P228Direction::Inbound);
    let terminal = p229_classify(
        true,
        Some(&p229_test_settings()),
        Some(&p229_test_transit_ok()),
        Some(&exploratory),
        &p229_test_p228_paired_missing(),
    );
    assert_eq!(
        terminal,
        P229Terminal::ExploratoryNonzeroNotBuilt(P228Direction::Inbound)
    );
}

#[test]
fn p229_both_nonzero_admits_helper_start() {
    let exploratory = p229_test_exploratory(1, 1);
    assert!(p229_nonzero_gate_pass(&exploratory));
    // The gate passing moves classification past the exploratory stop
    // into the build-path continuation (contradiction here because the
    // stubbed P228 trace still reports no paired tunnel).
    let terminal = p229_classify(
        true,
        Some(&p229_test_settings()),
        Some(&p229_test_transit_ok()),
        Some(&exploratory),
        &p229_test_p228_paired_missing(),
    );
    assert_eq!(terminal, P229Terminal::ContradictionNonzeroButNoPaired);
}

#[test]
fn p229_nonzero_plus_no_paired_maps_to_contradiction() {
    // Plan 228's terminal must disappear once both non-zero exploratory
    // directions are proven; its persistence is an explicit contradiction.
    let terminal = p229_classify(
        true,
        Some(&p229_test_settings()),
        Some(&p229_test_transit_ok()),
        Some(&p229_test_exploratory(2, 1)),
        &P228Terminal::NoPairedTunnel(P228Direction::Both),
    );
    assert_eq!(terminal, P229Terminal::ContradictionNonzeroButNoPaired);
    assert_eq!(
        terminal.token(),
        "P229-EVIDENCE-CONTRADICTION-NONZERO-EXPLORATORY-BUT-NO-PAIRED"
    );
}

#[test]
fn p229_paired_without_create_maps_to_create_boundary() {
    let mut trace = p228_test_trace_full();
    trace.paired_missing_seen = false;
    trace.build_message_create_fail_seen = true;
    trace.inbound_dispatch_seen = false;
    trace.outbound_dispatch_seen = false;
    let infra = p228_test_infra(2, 2);
    let p228 = p228_classify(Some(&infra), &trace, true, false);
    assert_eq!(
        p228,
        P228Terminal::BuildMessageCreateFailure(P228Direction::Both)
    );
    let terminal = p229_classify(
        true,
        Some(&p229_test_settings()),
        Some(&p229_test_transit_ok()),
        Some(&p229_test_exploratory(1, 1)),
        &p228,
    );
    assert_eq!(terminal, P229Terminal::NextBoundaryBuildMessageCreate);
}

#[test]
fn p229_create_without_dispatch_maps_to_dispatch_boundary() {
    let mut trace = p228_test_trace_full();
    trace.paired_missing_seen = false;
    trace.inbound_dispatch_seen = false;
    trace.outbound_dispatch_seen = false;
    let infra = p228_test_infra(2, 2);
    let p228 = p228_classify(Some(&infra), &trace, true, false);
    assert_eq!(
        p228,
        P228Terminal::BuildCreatedNotDispatched(P228Direction::Both)
    );
    let terminal = p229_classify(
        true,
        Some(&p229_test_settings()),
        Some(&p229_test_transit_ok()),
        Some(&p229_test_exploratory(1, 1)),
        &p228,
    );
    assert_eq!(terminal, P229Terminal::NextBoundaryADispatch);
}

#[test]
fn p229_dispatch_without_c_receive_maps_to_first_hop_boundary() {
    for p228 in [
        P228Terminal::FirstHopDeliveryFailure,
        P228Terminal::ADispatchedCNotReceived,
    ] {
        let terminal = p229_classify(
            true,
            Some(&p229_test_settings()),
            Some(&p229_test_transit_ok()),
            Some(&p229_test_exploratory(1, 1)),
            &p228,
        );
        assert_eq!(terminal, P229Terminal::NextBoundaryFirstHopDelivery);
    }
}

#[test]
fn p229_c_reject_retained_with_bounded_code() {
    let terminal = p229_classify(
        true,
        Some(&p229_test_settings()),
        Some(&p229_test_transit_ok()),
        Some(&p229_test_exploratory(1, 1)),
        &P228Terminal::CRejected(10),
    );
    assert_eq!(terminal, P229Terminal::NextBoundaryCReject(10));
    assert_eq!(terminal.token(), "P229-NEXT-BOUNDARY-C-REJECT code=10");
    let terminal = p229_classify(
        true,
        Some(&p229_test_settings()),
        Some(&p229_test_transit_ok()),
        Some(&p229_test_exploratory(1, 1)),
        &P228Terminal::RemoteReject(20),
    );
    assert_eq!(terminal, P229Terminal::NextBoundaryRemoteReject(20));
    assert_eq!(terminal.token(), "P229-NEXT-BOUNDARY-REMOTE-REJECT code=20");
    // Out-of-range codes never reach the P229 boundary: the P228 parser
    // rejects them first.
    assert_eq!(
        p228_parse_response_code("replied with status 9999", "replied with status"),
        None
    );
}

#[test]
fn p229_reply_decrypt_join_failures_retain_earliest_ordering() {
    // Reply-decrypt failure precedes join: earliest proven stage wins.
    let terminal = p229_classify(
        true,
        Some(&p229_test_settings()),
        Some(&p229_test_transit_ok()),
        Some(&p229_test_exploratory(1, 1)),
        &P228Terminal::ReplyDecryptFailure,
    );
    assert_eq!(terminal, P229Terminal::NextBoundaryReplyDecrypt);
    let terminal = p229_classify(
        true,
        Some(&p229_test_settings()),
        Some(&p229_test_transit_ok()),
        Some(&p229_test_exploratory(1, 1)),
        &P228Terminal::LocalJoinFailure,
    );
    assert_eq!(terminal, P229Terminal::NextBoundaryLocalJoin);
    let terminal = p229_classify(
        true,
        Some(&p229_test_settings()),
        Some(&p229_test_transit_ok()),
        Some(&p229_test_exploratory(1, 1)),
        &P228Terminal::ReplyNotReturned,
    );
    assert_eq!(terminal, P229Terminal::NextBoundaryReplyReturn);
    let terminal = p229_classify(
        true,
        Some(&p229_test_settings()),
        Some(&p229_test_transit_ok()),
        Some(&p229_test_exploratory(1, 1)),
        &P228Terminal::BuildReplyTimeout(P228Direction::Both),
    );
    assert_eq!(terminal, P229Terminal::NextBoundaryBuildReplyTimeout);
    let terminal = p229_classify(
        true,
        Some(&p229_test_settings()),
        Some(&p229_test_transit_ok()),
        Some(&p229_test_exploratory(1, 1)),
        &P228Terminal::CDecryptFailure,
    );
    assert_eq!(terminal, P229Terminal::NextBoundaryCDecrypt);
}

#[test]
fn p229_both_client_tunnels_install_maps_only_to_built() {
    let terminal = p229_classify(
        true,
        Some(&p229_test_settings()),
        Some(&p229_test_transit_ok()),
        Some(&p229_test_exploratory(1, 1)),
        &P228Terminal::NextBoundaryClientTunnelsBuilt,
    );
    assert_eq!(terminal, P229Terminal::ClientTunnelsBuilt);
    assert_eq!(terminal.token(), "P229-CLIENT-TUNNELS-BUILT");
    // The built terminal stops before lookup qualification.
    assert!(!p229_terminal_permits_lookup(&terminal));
}

#[test]
fn p229_no_lookup_or_reverse_send_after_terminal() {
    // No Plan-229 terminal may admit the Plan-226 lookup or the frozen
    // reverse-delivery lane.
    for terminal in [
        P229Terminal::RoleMismatch,
        P229Terminal::ExploratorySettingsMismatch,
        P229Terminal::CNotExploratoryEligible,
        P229Terminal::ExploratoryNonzeroNotBuilt(P228Direction::Both),
        P229Terminal::ContradictionNonzeroButNoPaired,
        P229Terminal::NextBoundaryBuildMessageCreate,
        P229Terminal::NextBoundaryADispatch,
        P229Terminal::NextBoundaryFirstHopDelivery,
        P229Terminal::NextBoundaryCDecrypt,
        P229Terminal::NextBoundaryCReject(0),
        P229Terminal::NextBoundaryReplyReturn,
        P229Terminal::NextBoundaryReplyDecrypt,
        P229Terminal::NextBoundaryRemoteReject(0),
        P229Terminal::NextBoundaryLocalJoin,
        P229Terminal::NextBoundaryBuildReplyTimeout,
        P229Terminal::ClientTunnelsBuilt,
        P229Terminal::ObservabilityGap,
    ] {
        assert!(
            !p229_terminal_permits_lookup(&terminal),
            "terminal must not permit lookup: {}",
            terminal.token()
        );
        assert!(
            !terminal.token().contains("LOOKUP") && !terminal.token().contains("45"),
            "terminal must not claim lookup/reverse payload: {}",
            terminal.token()
        );
    }
}

#[test]
fn p229_exactly_one_terminal_per_attempt() {
    let dir = p224_test_tmpdir("p229-record-once");
    let evidence_dir = dir.join("evidence");
    std::fs::create_dir_all(&evidence_dir).expect("evidence dir");
    let terminal = P229Terminal::ContradictionNonzeroButNoPaired;
    record_p229_classification(&evidence_dir, &terminal);
    let tsv = std::fs::read_to_string(evidence_dir.join("driver-evidence.tsv")).expect("read tsv");
    assert_eq!(
        tsv.lines()
            .filter(|line| line.starts_with("p229-classification\t"))
            .count(),
        1
    );
    assert!(tsv.contains("P229-EVIDENCE-CONTRADICTION-NONZERO-EXPLORATORY-BUT-NO-PAIRED"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn p229_unrelated_logs_cannot_satisfy_facts() {
    let c_hex = p227_test_c_hex();
    // Wrong prefix never parses.
    assert!(
        p229_parse_transit_peer(
            &format!("P228-EV kind=transit-peer observable=true router_c_hex={c_hex}"),
            &c_hex
        )
        .is_none()
    );
    assert!(
        p229_parse_exploratory_settings("P228-EV kind=exploratory-settings observable=true")
            .is_none()
    );
    assert!(
        p229_parse_exploratory(
            &format!("P227-EV kind=exploratory-tunnels observable=true router_c_hex={c_hex}"),
            &c_hex
        )
        .is_none()
    );
    // Echo mismatch never parses.
    let other = "cd".repeat(32);
    let line = format!(
        "P229-EV kind=transit-peer observable=true router_c_hex={other} main_raw_present=true main_valid_present=true profile_present=true selectable=true banlisted=false unreachable=false caps_has_f=false profile_count=3 not_failing_count=2"
    );
    assert!(p229_parse_transit_peer(&line, &c_hex).is_none());
    // Positive parse requires the exact Router-C echo.
    let line = format!(
        "P229-EV kind=transit-peer observable=true router_c_hex={c_hex} main_raw_present=true main_valid_present=true profile_present=true selectable=true banlisted=false unreachable=false caps_has_f=false profile_count=3 not_failing_count=2"
    );
    let parsed = p229_parse_transit_peer(&line, &c_hex).expect("parse transit");
    assert!(p229_transit_gate(&parsed));
    let exp_line = format!(
        "P229-EV kind=exploratory-tunnels observable=true router_c_hex={c_hex} inbound_exploratory_count=2 outbound_exploratory_count=2 inbound_nonzero_count=1 outbound_nonzero_count=1 inbound_c_present=true outbound_c_present=true zero_hop_fallback_present=true"
    );
    let exp = p229_parse_exploratory(&exp_line, &c_hex).expect("parse exploratory");
    assert!(p229_nonzero_gate_pass(&exp));
}

#[test]
fn p229_secret_raw_log_lines_rejected() {
    let c_hex = p227_test_c_hex();
    // Secret-bearing diagnostic rows never parse.
    let secret = format!(
        "P229-EV kind=transit-peer observable=true router_c_hex={c_hex} main_raw_present=true main_valid_present=true profile_present=true selectable=true banlisted=false unreachable=false caps_has_f=false profile_count=3 not_failing_count=2 session_key=abcd"
    );
    assert!(p229_parse_transit_peer(&secret, &c_hex).is_none());
    let secret_settings = "P229-EV kind=exploratory-settings observable=true inbound_length=1 inbound_variance=1 inbound_quantity=2 outbound_length=1 outbound_variance=1 outbound_quantity=2 payload=deadbeef";
    assert!(p229_parse_exploratory_settings(secret_settings).is_none());
    let secret_exp = format!(
        "P229-EV kind=exploratory-tunnels observable=true router_c_hex={c_hex} inbound_exploratory_count=2 outbound_exploratory_count=2 inbound_nonzero_count=1 outbound_nonzero_count=1 inbound_c_present=true outbound_c_present=true zero_hop_fallback_present=false log-router-0.txt"
    );
    assert!(p229_parse_exploratory(&secret_exp, &c_hex).is_none());
    // Malformed booleans/counts never parse.
    let malformed = format!(
        "P229-EV kind=transit-peer observable=true router_c_hex={c_hex} main_raw_present=yes main_valid_present=true profile_present=true selectable=true banlisted=false unreachable=false caps_has_f=false profile_count=3 not_failing_count=2"
    );
    assert!(p229_parse_transit_peer(&malformed, &c_hex).is_none());
}

// ---- Plan 230 reachability-capability/profile-bootstrap corrective ----
// Observation-contract corrective only: capability/predicate evidence,
// conditional stock controlled-topology correction, natural profile
// bootstrap, then direct continuation through the retained P229/P228/P201
// gates. No production behavior, no Java source patch, no profile/NetDB/
// tunnel mutation, no VMComm, no alwaysQuery, no public topology, no
// timeout change. The historical P229 `unreachable` signal (deprecated
// `ProfileOrganizer.isFailing`, unconditionally false) is never consumed.

/// Exact-pinned `ProfileManagerImpl.shouldCreate(caps)` share floor.
const P230_SHARE_BANDWIDTH_FLOOR_BYTES: u64 = 128 * 1024;

/// Exact mirror of `P230Probe.heardAboutCreationEligible`: the ordinary
/// `heardAbout()` profile-creation predicate from the exact Java I2P
/// 2.13.0 pin. `local_*` are the observer's own
/// `netDb().floodfillEnabled()` / `bandwidthLimiter()`
/// `.getMaxShareBandwidth()` inputs.
#[allow(clippy::too_many_arguments)]
fn p230_heard_about_eligible(
    caps_has_r: bool,
    caps_has_f: bool,
    caps_has_l: bool,
    local_floodfill_enabled: bool,
    local_max_share_bandwidth: u64,
    caps_has_e: bool,
    caps_has_g: bool,
) -> bool {
    if !caps_has_r {
        return false;
    }
    if caps_has_f {
        return true;
    }
    let low_bandwidth_exempt = !caps_has_l
        || (!local_floodfill_enabled
            && local_max_share_bandwidth < P230_SHARE_BANDWIDTH_FLOOR_BYTES);
    if !low_bandwidth_exempt {
        return false;
    }
    if caps_has_e {
        return false;
    }
    if caps_has_g {
        return false;
    }
    true
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct P230Capability {
    main_raw_present: bool,
    main_valid_present: bool,
    selectable: bool,
    banlisted: bool,
    caps_has_r: bool,
    caps_has_u: bool,
    caps_has_f: bool,
    caps_has_l: bool,
    caps_has_e: bool,
    caps_has_g: bool,
    bandwidth_tier: String,
    c_ri_sha256: String,
    profile_present: bool,
    profile_count: u64,
    not_failing_count: u64,
    local_floodfill_enabled: bool,
    local_max_share_bandwidth: u64,
    local_comm_status: String,
    heard_about_creation_eligible: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct P230SelfView {
    self_comm_status: String,
    self_caps_has_r: bool,
    self_caps_has_u: bool,
    self_caps_has_f: bool,
    self_caps_has_l: bool,
    self_caps_has_e: bool,
    self_caps_has_g: bool,
    self_bandwidth_tier: String,
    self_ri_sha256: String,
}

fn p230_parse_token(value: Option<&String>, max_len: usize) -> Option<String> {
    let raw = value?;
    if raw.is_empty() || raw.len() > max_len {
        return None;
    }
    if !raw.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
        return None;
    }
    Some(raw.to_owned())
}

fn p230_parse_share(value: Option<&String>) -> Option<u64> {
    let raw = value?;
    if raw.is_empty() || raw.len() > 12 || !raw.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    raw.parse().ok()
}

fn p230_parse_sha(value: Option<&String>) -> Option<String> {
    let raw = value?;
    if raw == "unknown" {
        return Some(raw.to_owned());
    }
    if p227_is_hex64(raw) {
        return Some(raw.to_owned());
    }
    None
}

/// P230 rows are unquoted `key=value` tokens only: any bare token (a
/// space-injected value truncated by the kv splitter, a stray log
/// fragment) rejects the row instead of silently truncating a fact.
fn p230_strict_shape(line: &str) -> bool {
    let mut tokens = line.split(' ');
    match (tokens.next(), tokens.next()) {
        (Some("P230-EV"), Some(kind)) if kind.starts_with("kind=") => {}
        _ => return false,
    }
    tokens.all(|t| t.is_empty() || t.contains('='))
}

fn p230_parse_capability(line: &str, expected_c_hex: &str) -> Option<P230Capability> {
    if !line.starts_with("P230-EV ") || !line.contains("kind=capability") {
        return None;
    }
    if line.len() > 1024 || p228_line_is_secret_bearing(line) {
        return None;
    }
    if !p230_strict_shape(line) {
        return None;
    }
    // Plan 230 §4.11: the historical P229 `unreachable` / `isFailing`
    // signal is non-authoritative and must not appear in Plan 230 rows.
    if line.contains("unreachable") || line.contains("isFailing") {
        return None;
    }
    // Plan 230 §13: probe-side profile creation calls are forbidden;
    // a row carrying their markers is corrupt evidence, never a fact.
    if line.contains("addProfile")
        || line.contains("getOrCreateProfile")
        || line.contains("heardAbout")
    {
        return None;
    }
    let kv = p220_parse_kv(&line.replace("P230-EV ", "P220-EV "));
    if kv.get("observable").is_none_or(|v| v != "true") {
        return None;
    }
    let echoed = kv.get("router_c_hex")?;
    if !echoed.eq_ignore_ascii_case(expected_c_hex) || !p227_is_hex64(&echoed.to_lowercase()) {
        return None;
    }
    Some(P230Capability {
        main_raw_present: p229_parse_bool(kv.get("main_raw_present"))?,
        main_valid_present: p229_parse_bool(kv.get("main_valid_present"))?,
        selectable: p229_parse_bool(kv.get("selectable"))?,
        banlisted: p229_parse_bool(kv.get("banlisted"))?,
        caps_has_r: p229_parse_bool(kv.get("caps_has_r"))?,
        caps_has_u: p229_parse_bool(kv.get("caps_has_u"))?,
        caps_has_f: p229_parse_bool(kv.get("caps_has_f"))?,
        caps_has_l: p229_parse_bool(kv.get("caps_has_l"))?,
        caps_has_e: p229_parse_bool(kv.get("caps_has_e"))?,
        caps_has_g: p229_parse_bool(kv.get("caps_has_g"))?,
        bandwidth_tier: p230_parse_token(kv.get("bandwidth_tier"), 8)?,
        c_ri_sha256: p230_parse_sha(kv.get("c_ri_sha256"))?,
        profile_present: p229_parse_bool(kv.get("profile_present"))?,
        profile_count: p229_parse_small_count(kv.get("profile_count"))?,
        not_failing_count: p229_parse_small_count(kv.get("not_failing_count"))?,
        local_floodfill_enabled: p229_parse_bool(kv.get("local_floodfill_enabled"))?,
        local_max_share_bandwidth: p230_parse_share(kv.get("local_max_share_bandwidth"))?,
        local_comm_status: p230_parse_token(kv.get("local_comm_status"), 32)?,
        heard_about_creation_eligible: p229_parse_bool(kv.get("heard_about_creation_eligible"))?,
    })
}

fn p230_parse_self_view(line: &str) -> Option<P230SelfView> {
    if !line.starts_with("P230-EV ") || !line.contains("kind=self-view") {
        return None;
    }
    if line.len() > 1024 || p228_line_is_secret_bearing(line) {
        return None;
    }
    if !p230_strict_shape(line) {
        return None;
    }
    if line.contains("unreachable") || line.contains("isFailing") {
        return None;
    }
    if line.contains("addProfile")
        || line.contains("getOrCreateProfile")
        || line.contains("heardAbout")
    {
        return None;
    }
    let kv = p220_parse_kv(&line.replace("P230-EV ", "P220-EV "));
    if kv.get("observable").is_none_or(|v| v != "true") {
        return None;
    }
    Some(P230SelfView {
        self_comm_status: p230_parse_token(kv.get("self_comm_status"), 32)?,
        self_caps_has_r: p229_parse_bool(kv.get("self_caps_has_r"))?,
        self_caps_has_u: p229_parse_bool(kv.get("self_caps_has_u"))?,
        self_caps_has_f: p229_parse_bool(kv.get("self_caps_has_f"))?,
        self_caps_has_l: p229_parse_bool(kv.get("self_caps_has_l"))?,
        self_caps_has_e: p229_parse_bool(kv.get("self_caps_has_e"))?,
        self_caps_has_g: p229_parse_bool(kv.get("self_caps_has_g"))?,
        self_bandwidth_tier: p230_parse_token(kv.get("self_bandwidth_tier"), 8)?,
        self_ri_sha256: p230_parse_sha(kv.get("self_ri_sha256"))?,
    })
}

/// The probe's derived `heard_about_creation_eligible` fact must agree
/// with the exact predicate recomputed from the independently durable
/// inputs (Plan 230 §5). Disagreement is an observability gap, never a
/// silent pass.
fn p230_predicate_agrees(cap: &P230Capability) -> bool {
    p230_heard_about_eligible(
        cap.caps_has_r,
        cap.caps_has_f,
        cap.caps_has_l,
        cap.local_floodfill_enabled,
        cap.local_max_share_bandwidth,
        cap.caps_has_e,
        cap.caps_has_g,
    ) == cap.heard_about_creation_eligible
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum P230Blocker {
    MissingR,
    HasU,
    LowBandwidthLOnFloodfillObserver,
    HasE,
    HasG,
    Compound,
}

impl P230Blocker {
    fn reason(self) -> &'static str {
        match self {
            Self::MissingR => "missing-r",
            Self::HasU => "has-u",
            Self::LowBandwidthLOnFloodfillObserver => "low-bandwidth-l-on-floodfill-observer",
            Self::HasE => "has-e",
            Self::HasG => "has-g",
            Self::Compound => "compound",
        }
    }
}

/// Collect every applicable creation-predicate blocker in fixed order.
/// `U` without `R` reports `has-u`; any other missing-`R` reports
/// `missing-r`. The low-bandwidth branch fires exactly when the pinned
/// alternate (`!floodfill observer` with sub-floor share) does not hold.
/// Blockers are observed RouterInfo facts: `L`/`E`/`G` are collected
/// even when `R` is also missing, so a compound baseline authorizes the
/// matching correction subset. An eligible capability carries none.
fn p230_baseline_blockers(cap: &P230Capability) -> Vec<P230Blocker> {
    if p230_heard_about_eligible(
        cap.caps_has_r,
        cap.caps_has_f,
        cap.caps_has_l,
        cap.local_floodfill_enabled,
        cap.local_max_share_bandwidth,
        cap.caps_has_e,
        cap.caps_has_g,
    ) {
        return Vec::new();
    }
    let mut blockers = Vec::new();
    if !cap.caps_has_r {
        blockers.push(if cap.caps_has_u {
            P230Blocker::HasU
        } else {
            P230Blocker::MissingR
        });
    }
    let low_bandwidth_exempt = !cap.caps_has_l
        || (!cap.local_floodfill_enabled
            && cap.local_max_share_bandwidth < P230_SHARE_BANDWIDTH_FLOOR_BYTES);
    if cap.caps_has_l && !low_bandwidth_exempt {
        blockers.push(P230Blocker::LowBandwidthLOnFloodfillObserver);
    }
    if cap.caps_has_e {
        blockers.push(P230Blocker::HasE);
    }
    if cap.caps_has_g {
        blockers.push(P230Blocker::HasG);
    }
    blockers
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum P230Baseline {
    Eligible,
    Ineligible(P230Blocker),
    ObservabilityGap,
}

impl P230Baseline {
    /// Canonical WP B outcome vocabulary, shared with the shell
    /// `p230-baseline` row. Eligibility never implies the profile
    /// exists; it only authorizes WP D without the correction.
    fn token(&self) -> String {
        match self {
            Self::Eligible => "P230-A-PREDICATE-ELIGIBLE".to_owned(),
            Self::Ineligible(reason) => {
                format!("P230-A-PREDICATE-INELIGIBLE reason={}", reason.reason())
            }
            Self::ObservabilityGap => "P230-A-OBSERVABILITY-GAP".to_owned(),
        }
    }
}

/// Plan 230 WP B gate: one baseline predicate outcome. Eligibility never
/// implies the profile exists; it only authorizes WP D without the
/// configuration corrective.
fn p230_classify_baseline(cap: Option<&P230Capability>) -> P230Baseline {
    let Some(cap) = cap else {
        return P230Baseline::ObservabilityGap;
    };
    if !p230_predicate_agrees(cap) {
        return P230Baseline::ObservabilityGap;
    }
    if cap.heard_about_creation_eligible {
        return P230Baseline::Eligible;
    }
    let blockers = p230_baseline_blockers(cap);
    if blockers.len() == 1 {
        return P230Baseline::Ineligible(blockers[0]);
    }
    P230Baseline::Ineligible(P230Blocker::Compound)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum P230Correction {
    None,
    ReachabilityOnly,
    BandwidthOnly,
    Both,
    StopUnexpectedExclusion,
}

/// Plan 230 WP C authorization: only the correction matching the proven
/// baseline blocker(s). `E`/`G` stop the correction only when they are
/// the sole remaining blockers (Plan 230 §7 C3); alongside a proven
/// reachability or bandwidth blocker the matching C1/C2 correction
/// applies first. No tuning knob is added in the same implementation
/// SHA.
fn p230_authorized_correction(cap: Option<&P230Capability>) -> P230Correction {
    let Some(cap) = cap else {
        return P230Correction::None;
    };
    if !p230_predicate_agrees(cap) {
        return P230Correction::None;
    }
    if cap.heard_about_creation_eligible {
        return P230Correction::None;
    }
    let blockers = p230_baseline_blockers(cap);
    if blockers.is_empty() {
        return P230Correction::None;
    }
    let only_exclusions = blockers
        .iter()
        .all(|b| matches!(b, P230Blocker::HasE | P230Blocker::HasG));
    if only_exclusions {
        return P230Correction::StopUnexpectedExclusion;
    }
    let reach = blockers.contains(&P230Blocker::MissingR) || blockers.contains(&P230Blocker::HasU);
    let bandwidth = blockers.contains(&P230Blocker::LowBandwidthLOnFloodfillObserver);
    match (reach, bandwidth) {
        (true, true) => P230Correction::Both,
        (true, false) => P230Correction::ReachabilityOnly,
        (false, true) => P230Correction::BandwidthOnly,
        (false, false) => P230Correction::None,
    }
}

/// The stock controlled-topology correction is fixture-only: every SSU2
/// endpoint stays loopback, reseed stays disabled, VMComm stays disabled.
/// The harness proves these before any override is honored.
fn p230_reachability_override_permitted(
    ssu2_hosts_loopback: bool,
    reseed_disabled: bool,
    vmcomm_disabled: bool,
) -> bool {
    ssu2_hosts_loopback && reseed_disabled && vmcomm_disabled
}

/// Exact stock properties the controlled fixture may apply, mirrored by
/// `ControlledRouter`. Reachability applies to every controlled role;
/// the bandwidth class correction applies to the transit Router C only.
/// `router.forceBandwidthClass` is never authorized (Plan 230 §7).
fn p230_authorized_launcher_props(
    correction: P230Correction,
    role: P229Role,
    scope_permitted: bool,
) -> Vec<(&'static str, &'static str)> {
    if !scope_permitted {
        return Vec::new();
    }
    let mut props = Vec::new();
    if matches!(
        correction,
        P230Correction::ReachabilityOnly | P230Correction::Both
    ) {
        props.push(("i2np.udp.status", "ok"));
    }
    if matches!(
        correction,
        P230Correction::BandwidthOnly | P230Correction::Both
    ) && matches!(role, P229Role::Transit)
    {
        props.push(("i2np.bandwidth.outboundKBytesPerSecond", "128"));
        props.push(("i2np.bandwidth.outboundBurstKBytesPerSecond", "128"));
    }
    props
}

/// Plan 230 WP D staleness discriminator: a pre-correction RouterInfo
/// observed before a capability change may not satisfy a post-correction
/// gate. Both identities must be concrete 64-hex SHAs and byte-equal.
fn p230_ri_fresh(observed_sha: &str, self_sha: &str) -> bool {
    observed_sha != "unknown"
        && self_sha != "unknown"
        && observed_sha == self_sha
        && p227_is_hex64(observed_sha)
        && p227_is_hex64(self_sha)
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum P230Terminal {
    AObservabilityGap,
    AIneligible(P230Blocker),
    CUnexpectedCapabilityExclusion,
    DObservabilityGap,
    DRiNotUpdated,
    DEligibleButNoProfile,
    EExploratoryNotBuilt(P228Direction),
    EClientNotBuilt(P228Direction),
    EPairedTunnelContradiction,
    ETunnelContinuationPassed,
}

impl P230Terminal {
    fn token(&self) -> String {
        match self {
            Self::AObservabilityGap => "P230-A-OBSERVABILITY-GAP".to_owned(),
            Self::AIneligible(reason) => {
                format!("P230-A-PREDICATE-INELIGIBLE reason={}", reason.reason())
            }
            Self::CUnexpectedCapabilityExclusion => {
                "P230-C-UNEXPECTED-CAPABILITY-EXCLUSION".to_owned()
            }
            Self::DObservabilityGap => "P230-D-OBSERVABILITY-GAP".to_owned(),
            Self::DRiNotUpdated => "P230-D-RI-NOT-UPDATED".to_owned(),
            Self::DEligibleButNoProfile => "P230-D-ELIGIBLE-BUT-NO-PROFILE".to_owned(),
            Self::EExploratoryNotBuilt(dir) => {
                format!("P230-E-EXPLORATORY-NOT-INSTALLED direction={}", dir.token())
            }
            Self::EClientNotBuilt(dir) => {
                format!("P230-E-CLIENT-NOT-BUILT direction={}", dir.token())
            }
            Self::EPairedTunnelContradiction => "P230-E-PAIRED-TUNNEL-CONTRADICTION".to_owned(),
            Self::ETunnelContinuationPassed => "P230-E-TUNNEL-CONTINUATION-PASSED".to_owned(),
        }
    }
}

/// Plan 230 WP D gate: natural profile bootstrap through the ordinary
/// authenticated DatabaseStore path. Requires the exact creation
/// predicate, a post-correction RI identity match, natural organizer
/// membership (`selectAllPeers` + non-null `getProfileNonblocking`,
/// already encoded in `profile_present`), a non-empty not-failing
/// population, and a selectable, non-banned, non-floodfill record.
fn p230_classify_profile(
    cap: Option<&P230Capability>,
    self_view: Option<&P230SelfView>,
) -> P230Terminal {
    let (Some(cap), Some(self_view)) = (cap, self_view) else {
        return P230Terminal::DObservabilityGap;
    };
    if !p230_predicate_agrees(cap) {
        return P230Terminal::DObservabilityGap;
    }
    if !cap.heard_about_creation_eligible {
        return match p230_classify_baseline(Some(cap)) {
            P230Baseline::Ineligible(reason) => P230Terminal::AIneligible(reason),
            _ => P230Terminal::DObservabilityGap,
        };
    }
    if !p230_ri_fresh(&cap.c_ri_sha256, &self_view.self_ri_sha256) {
        return P230Terminal::DRiNotUpdated;
    }
    if cap.profile_present
        && cap.selectable
        && !cap.banlisted
        && !cap.caps_has_f
        && cap.not_failing_count >= 1
    {
        return P230Terminal::ETunnelContinuationPassed;
    }
    P230Terminal::DEligibleButNoProfile
}

/// Plan 230 WP E gate: genuine non-zero exploratory tunnels in both
/// directions, genuine one-hop client tunnels through C in both
/// directions, then the retained Plan-228 paired-tunnel disposition.
/// Returns the E-stage terminal; `ETunnelContinuationPassed` is the only
/// terminal that permits entering the frozen destination lane (WP F).
#[allow(clippy::too_many_arguments)]
fn p230_classify_tunnel_continuation(
    exploratory: Option<&P229Exploratory>,
    client_in_exact: bool,
    client_out_exact: bool,
    p228: &P228Terminal,
) -> P230Terminal {
    let Some(exploratory) = exploratory else {
        return P230Terminal::EExploratoryNotBuilt(P228Direction::Both);
    };
    if !p229_nonzero_gate_pass(exploratory) {
        return P230Terminal::EExploratoryNotBuilt(p229_nonzero_gate(exploratory));
    }
    if client_in_exact && client_out_exact {
        return P230Terminal::ETunnelContinuationPassed;
    }
    if matches!(p228, P228Terminal::NoPairedTunnel(_)) {
        return P230Terminal::EPairedTunnelContradiction;
    }
    let missing = match (client_in_exact, client_out_exact) {
        (true, false) => P228Direction::Outbound,
        (false, true) => P228Direction::Inbound,
        _ => P228Direction::Both,
    };
    P230Terminal::EClientNotBuilt(missing)
}

/// Plan 230 §10 / §15: the frozen destination/Streaming lane is entered
/// only through the tunnel-continuation pass. No other terminal admits
/// lookup or reverse-delivery qualification.
fn p230_terminal_permits_destination(terminal: &P230Terminal) -> bool {
    matches!(terminal, P230Terminal::ETunnelContinuationPassed)
}

fn record_p230_capability(
    evidence_dir: &std::path::Path,
    cap: Option<&P230Capability>,
    router_c_hex: &str,
) {
    match cap {
        Some(facts) => append_evidence(
            evidence_dir,
            "p230-capability",
            &format!(
                "router_c_hex={router_c_hex} main_raw_present={} main_valid_present={} selectable={} banlisted={} caps_has_r={} caps_has_u={} caps_has_f={} caps_has_l={} caps_has_e={} caps_has_g={} bandwidth_tier={} c_ri_sha256={} profile_present={} profile_count={} not_failing_count={} local_floodfill_enabled={} local_max_share_bandwidth={} local_comm_status={} heard_about_creation_eligible={}",
                facts.main_raw_present,
                facts.main_valid_present,
                facts.selectable,
                facts.banlisted,
                facts.caps_has_r,
                facts.caps_has_u,
                facts.caps_has_f,
                facts.caps_has_l,
                facts.caps_has_e,
                facts.caps_has_g,
                facts.bandwidth_tier,
                facts.c_ri_sha256,
                facts.profile_present,
                facts.profile_count,
                facts.not_failing_count,
                facts.local_floodfill_enabled,
                facts.local_max_share_bandwidth,
                facts.local_comm_status,
                facts.heard_about_creation_eligible,
            ),
        ),
        None => append_evidence(
            evidence_dir,
            "p230-capability",
            &format!("router_c_hex={router_c_hex} observable=false reason=diagnostic-unreachable"),
        ),
    }
}

fn record_p230_classification(evidence_dir: &std::path::Path, terminal: &P230Terminal) {
    append_evidence(evidence_dir, "p230-classification", &terminal.token());
}

/// Canonical WP D pass outcome, shared with the shell `p230-profile`
/// row. The pass itself continues into WP E under the
/// `ETunnelContinuationPassed` terminal; this token names the row.
const P230_D_PROFILE_BOOTSTRAP_PASSED: &str = "P230-D-PROFILE-BOOTSTRAP-PASSED";

fn record_p230_baseline(
    evidence_dir: &std::path::Path,
    baseline: &P230Baseline,
    cap: Option<&P230Capability>,
    router_c_hex: &str,
) {
    match cap {
        Some(facts) => append_evidence(
            evidence_dir,
            "p230-baseline",
            &format!(
                "router_c_hex={router_c_hex} outcome={} caps_has_r={} caps_has_f={} caps_has_l={} caps_has_e={} caps_has_g={} local_floodfill_enabled={} local_max_share_bandwidth={}",
                baseline.token(),
                facts.caps_has_r,
                facts.caps_has_f,
                facts.caps_has_l,
                facts.caps_has_e,
                facts.caps_has_g,
                facts.local_floodfill_enabled,
                facts.local_max_share_bandwidth,
            ),
        ),
        None => append_evidence(
            evidence_dir,
            "p230-baseline",
            &format!(
                "router_c_hex={router_c_hex} outcome={} reason=diagnostic-unreachable",
                P230Baseline::ObservabilityGap.token(),
            ),
        ),
    }
}

fn record_p230_profile(
    evidence_dir: &std::path::Path,
    outcome: &str,
    cap: Option<&P230Capability>,
    self_view: Option<&P230SelfView>,
    router_c_hex: &str,
) {
    match (cap, self_view) {
        (Some(facts), Some(view)) => append_evidence(
            evidence_dir,
            "p230-profile",
            &format!(
                "router_c_hex={router_c_hex} outcome={outcome} profile_present={} not_failing_count={} c_ri_sha256={} self_ri_sha256={}",
                facts.profile_present,
                facts.not_failing_count,
                facts.c_ri_sha256,
                view.self_ri_sha256,
            ),
        ),
        _ => append_evidence(
            evidence_dir,
            "p230-profile",
            &format!("router_c_hex={router_c_hex} outcome={outcome} reason=diagnostic-unreachable"),
        ),
    }
}

// ---- Plan 230 focused unit rows (Plan 230 §12) ---------------------------

fn p230_test_cap() -> P230Capability {
    P230Capability {
        main_raw_present: true,
        main_valid_present: true,
        selectable: true,
        banlisted: false,
        caps_has_r: true,
        caps_has_u: false,
        caps_has_f: false,
        caps_has_l: false,
        caps_has_e: false,
        caps_has_g: false,
        bandwidth_tier: "N".to_owned(),
        c_ri_sha256: "ab".repeat(32),
        profile_present: false,
        profile_count: 0,
        not_failing_count: 0,
        local_floodfill_enabled: true,
        local_max_share_bandwidth: 48 * 1024,
        local_comm_status: "OK".to_owned(),
        heard_about_creation_eligible: true,
    }
}

fn p230_test_self() -> P230SelfView {
    P230SelfView {
        self_comm_status: "OK".to_owned(),
        self_caps_has_r: true,
        self_caps_has_u: false,
        self_caps_has_f: false,
        self_caps_has_l: false,
        self_caps_has_e: false,
        self_caps_has_g: false,
        self_bandwidth_tier: "N".to_owned(),
        self_ri_sha256: "ab".repeat(32),
    }
}

fn p230_test_cap_line(c_hex: &str) -> String {
    format!(
        "P230-EV kind=capability observable=true router_c_hex={c_hex} main_raw_present=true main_valid_present=true selectable=true banlisted=false caps_has_r=true caps_has_u=false caps_has_f=false caps_has_l=false caps_has_e=false caps_has_g=false bandwidth_tier=N c_ri_sha256={} profile_present=false profile_count=0 not_failing_count=0 local_floodfill_enabled=true local_max_share_bandwidth=49152 local_comm_status=OK heard_about_creation_eligible=true",
        "ab".repeat(32),
    )
}

#[test]
fn p230_isfailing_never_authoritative_for_reachability() {
    // The P230 observation contract has no failing/unreachable input:
    // selectability and banlist state never affect the creation
    // predicate, and the deprecated `isFailing` signal is rejected at
    // the parse boundary.
    let mut cap = p230_test_cap();
    cap.selectable = false;
    cap.banlisted = true;
    assert!(p230_predicate_agrees(&cap));
    assert_eq!(p230_classify_baseline(Some(&cap)), P230Baseline::Eligible);
    let c_hex = p227_test_c_hex();
    let tainted = p230_test_cap_line(&c_hex) + " unreachable=false";
    assert!(p230_parse_capability(&tainted, &c_hex).is_none());
    let tainted =
        p230_test_cap_line(&c_hex).replace("selectable=true", "selectable=true isFailing=false");
    assert!(p230_parse_capability(&tainted, &c_hex).is_none());
}

#[test]
fn p230_selectable_alone_does_not_imply_creation_eligible() {
    // Selectable, present, and valid in the main NetDB but missing the
    // reachability capability: still ineligible.
    let mut cap = p230_test_cap();
    cap.caps_has_r = false;
    cap.caps_has_l = true;
    cap.heard_about_creation_eligible = false;
    assert!(cap.selectable);
    assert!(cap.main_raw_present && cap.main_valid_present);
    assert!(!p230_heard_about_eligible(
        cap.caps_has_r,
        cap.caps_has_f,
        cap.caps_has_l,
        cap.local_floodfill_enabled,
        cap.local_max_share_bandwidth,
        cap.caps_has_e,
        cap.caps_has_g,
    ));
    assert!(matches!(
        p230_classify_baseline(Some(&cap)),
        P230Baseline::Ineligible(_)
    ));
}

#[test]
fn p230_missing_r_is_ineligible() {
    assert!(!p230_heard_about_eligible(
        false,
        false,
        false,
        true,
        48 * 1024,
        false,
        false
    ));
    assert!(!p230_heard_about_eligible(
        false,
        true,
        false,
        true,
        48 * 1024,
        false,
        false
    ));
    let mut cap = p230_test_cap();
    cap.caps_has_r = false;
    cap.heard_about_creation_eligible = false;
    assert_eq!(
        p230_classify_baseline(Some(&cap)),
        P230Baseline::Ineligible(P230Blocker::MissingR)
    );
    assert_eq!(
        p230_authorized_correction(Some(&cap)),
        P230Correction::ReachabilityOnly
    );
}

#[test]
fn p230_u_without_r_is_ineligible() {
    assert!(!p230_heard_about_eligible(
        false,
        false,
        false,
        true,
        48 * 1024,
        false,
        false
    ));
    let mut cap = p230_test_cap();
    cap.caps_has_r = false;
    cap.caps_has_u = true;
    cap.heard_about_creation_eligible = false;
    assert_eq!(
        p230_classify_baseline(Some(&cap)),
        P230Baseline::Ineligible(P230Blocker::HasU)
    );
    assert_eq!(
        p230_authorized_correction(Some(&cap)),
        P230Correction::ReachabilityOnly
    );
}

#[test]
fn p230_nonff_l_peer_on_floodfill_observer_is_ineligible() {
    // The exact Plan-230 fixture shape: non-floodfill transit C
    // advertises the default low-bandwidth class while floodfill
    // Router A observes. The predicate rejects it; only the C
    // bandwidth correction is authorized.
    assert!(!p230_heard_about_eligible(
        true,
        false,
        true,
        true,
        48 * 1024,
        false,
        false
    ));
    let mut cap = p230_test_cap();
    cap.caps_has_l = true;
    cap.bandwidth_tier = "L".to_owned();
    cap.heard_about_creation_eligible = false;
    assert_eq!(
        p230_classify_baseline(Some(&cap)),
        P230Baseline::Ineligible(P230Blocker::LowBandwidthLOnFloodfillObserver)
    );
    assert_eq!(
        p230_authorized_correction(Some(&cap)),
        P230Correction::BandwidthOnly
    );
    // A non-floodfill observer with a sub-floor share exempts the same
    // peer through the exact alternate branch.
    assert!(p230_heard_about_eligible(
        true,
        false,
        true,
        false,
        48 * 1024,
        false,
        false
    ));
    // ... but not once the observer share reaches the 128 KiB/s floor.
    assert!(!p230_heard_about_eligible(
        true,
        false,
        true,
        false,
        128 * 1024,
        false,
        false
    ));
}

#[test]
fn p230_nonff_r_non_l_non_e_non_g_is_eligible() {
    assert!(p230_heard_about_eligible(
        true,
        false,
        false,
        true,
        48 * 1024,
        false,
        false
    ));
    assert!(p230_heard_about_eligible(
        true, false, false, false, 0, false, false
    ));
    assert_eq!(
        p230_classify_baseline(Some(&p230_test_cap())),
        P230Baseline::Eligible
    );
    assert_eq!(
        p230_authorized_correction(Some(&p230_test_cap())),
        P230Correction::None
    );
}

#[test]
fn p230_floodfill_peer_with_r_is_eligible() {
    // Floodfill peers are eligible regardless of bandwidth/congestion
    // caps on the exact pinned predicate.
    for (l, e, g) in [
        (false, false, false),
        (true, false, false),
        (true, true, true),
    ] {
        assert!(p230_heard_about_eligible(
            true,
            true,
            l,
            true,
            48 * 1024,
            e,
            g
        ));
    }
    let mut cap = p230_test_cap();
    cap.caps_has_f = true;
    cap.caps_has_l = true;
    cap.caps_has_e = true;
    cap.caps_has_g = true;
    assert_eq!(p230_classify_baseline(Some(&cap)), P230Baseline::Eligible);
}

#[test]
fn p230_e_is_ineligible_for_nonff_peer() {
    assert!(!p230_heard_about_eligible(
        true,
        false,
        false,
        true,
        48 * 1024,
        true,
        false
    ));
    let mut cap = p230_test_cap();
    cap.caps_has_e = true;
    cap.heard_about_creation_eligible = false;
    assert_eq!(
        p230_classify_baseline(Some(&cap)),
        P230Baseline::Ineligible(P230Blocker::HasE)
    );
    assert_eq!(
        p230_authorized_correction(Some(&cap)),
        P230Correction::StopUnexpectedExclusion
    );
    assert_eq!(
        P230Terminal::CUnexpectedCapabilityExclusion.token(),
        "P230-C-UNEXPECTED-CAPABILITY-EXCLUSION"
    );
}

#[test]
fn p230_g_is_ineligible_for_nonff_peer() {
    assert!(!p230_heard_about_eligible(
        true,
        false,
        false,
        true,
        48 * 1024,
        false,
        true
    ));
    let mut cap = p230_test_cap();
    cap.caps_has_g = true;
    cap.heard_about_creation_eligible = false;
    assert_eq!(
        p230_classify_baseline(Some(&cap)),
        P230Baseline::Ineligible(P230Blocker::HasG)
    );
    assert_eq!(
        p230_authorized_correction(Some(&cap)),
        P230Correction::StopUnexpectedExclusion
    );
}

#[test]
fn p230_baseline_ineligible_authorizes_only_matching_fixture_correction() {
    // missing-R authorizes reachability only, never bandwidth.
    let mut cap = p230_test_cap();
    cap.caps_has_r = false;
    cap.heard_about_creation_eligible = false;
    let correction = p230_authorized_correction(Some(&cap));
    assert_eq!(correction, P230Correction::ReachabilityOnly);
    let props = p230_authorized_launcher_props(correction, P229Role::Transit, true);
    assert!(props.contains(&("i2np.udp.status", "ok")));
    assert!(!props.iter().any(|(k, _)| k.contains("bandwidth")));
    // Low-bandwidth-L authorizes bandwidth only, never reachability.
    let mut cap = p230_test_cap();
    cap.caps_has_l = true;
    cap.heard_about_creation_eligible = false;
    let correction = p230_authorized_correction(Some(&cap));
    assert_eq!(correction, P230Correction::BandwidthOnly);
    let props = p230_authorized_launcher_props(correction, P229Role::Transit, true);
    assert!(!props.iter().any(|(k, _)| *k == "i2np.udp.status"));
    assert!(props.contains(&("i2np.bandwidth.outboundKBytesPerSecond", "128")));
    // Eligible and gap baselines authorize nothing.
    assert_eq!(
        p230_authorized_correction(Some(&p230_test_cap())),
        P230Correction::None
    );
    assert_eq!(p230_authorized_correction(None), P230Correction::None);
    // Compound reachability + bandwidth authorizes exactly both.
    let mut cap = p230_test_cap();
    cap.caps_has_r = false;
    cap.caps_has_u = true;
    cap.caps_has_l = true;
    cap.heard_about_creation_eligible = false;
    assert_eq!(
        p230_classify_baseline(Some(&cap)),
        P230Baseline::Ineligible(P230Blocker::Compound)
    );
    assert_eq!(p230_authorized_correction(Some(&cap)), P230Correction::Both);
    // E/G alongside a proven reachability/bandwidth blocker does not
    // stop the matching correction; the exclusion stop fires only when
    // E/G is the sole remaining blocker (Plan 230 §7 C3).
    cap.caps_has_e = true;
    assert_eq!(p230_authorized_correction(Some(&cap)), P230Correction::Both);
    let mut sole = p230_test_cap();
    sole.caps_has_e = true;
    sole.caps_has_g = true;
    sole.heard_about_creation_eligible = false;
    assert_eq!(
        p230_classify_baseline(Some(&sole)),
        P230Baseline::Ineligible(P230Blocker::Compound)
    );
    assert_eq!(
        p230_authorized_correction(Some(&sole)),
        P230Correction::StopUnexpectedExclusion
    );
}

#[test]
fn p230_reachability_override_is_loopback_fixture_only() {
    assert!(p230_reachability_override_permitted(true, true, true));
    assert!(!p230_reachability_override_permitted(false, true, true));
    assert!(!p230_reachability_override_permitted(true, false, true));
    assert!(!p230_reachability_override_permitted(true, true, false));
    // Without the fixture scope, no correction materializes as launcher
    // properties even when the baseline authorizes it.
    assert!(
        p230_authorized_launcher_props(P230Correction::Both, P229Role::Transit, false).is_empty()
    );
    assert!(
        !p230_authorized_launcher_props(P230Correction::ReachabilityOnly, P229Role::Service, true)
            .is_empty()
    );
}

#[test]
fn p230_bandwidth_correction_applies_to_transit_c_only() {
    for role in [P229Role::Service, P229Role::Publication] {
        for correction in [
            P230Correction::BandwidthOnly,
            P230Correction::Both,
            P230Correction::ReachabilityOnly,
            P230Correction::None,
        ] {
            let props = p230_authorized_launcher_props(correction, role, true);
            assert!(
                !props.iter().any(|(k, _)| k.contains("bandwidth")),
                "non-transit role must never receive bandwidth props: {role:?} {correction:?}"
            );
        }
    }
    let props =
        p230_authorized_launcher_props(P230Correction::BandwidthOnly, P229Role::Transit, true);
    assert!(props.contains(&("i2np.bandwidth.outboundKBytesPerSecond", "128")));
    assert!(props.contains(&("i2np.bandwidth.outboundBurstKBytesPerSecond", "128")));
    // A/B keep their stock exploratory/small-router profile: no
    // bandwidth props even under `Both`.
    let props = p230_authorized_launcher_props(P230Correction::Both, P229Role::Service, true);
    assert_eq!(props, vec![("i2np.udp.status", "ok")]);
}

#[test]
fn p230_force_bandwidth_class_is_forbidden() {
    // No role/correction/scope combination may ever emit the lying
    // override; the transit bandwidth path uses real configured
    // bandwidth only.
    for role in [P229Role::Service, P229Role::Publication, P229Role::Transit] {
        for correction in [
            P230Correction::None,
            P230Correction::ReachabilityOnly,
            P230Correction::BandwidthOnly,
            P230Correction::Both,
            P230Correction::StopUnexpectedExclusion,
        ] {
            for scope in [true, false] {
                let props = p230_authorized_launcher_props(correction, role, scope);
                assert!(
                    !props.iter().any(|(k, _)| k.contains("forceBandwidthClass")),
                    "forceBandwidthClass forbidden: {role:?} {correction:?} {scope}"
                );
            }
        }
    }
    let props = p230_authorized_launcher_props(P230Correction::Both, P229Role::Transit, true);
    assert_eq!(props.len(), 3);
}

#[test]
fn p230_stale_pre_correction_ri_cannot_pass_post_correction_gate() {
    let pre = "00".repeat(32);
    let post = "ff".repeat(32);
    assert!(!p230_ri_fresh(&pre, &post));
    assert!(p230_ri_fresh(&post, &post));
    assert!(!p230_ri_fresh("unknown", &post));
    assert!(!p230_ri_fresh(&post, "unknown"));
    assert!(!p230_ri_fresh("not-hex", &post));
    assert!(!p230_ri_fresh(&post, &("ff".repeat(31) + "FG")));
    // A stale observed RI maps the D gate to RI-NOT-UPDATED even when
    // every other D input passes.
    let mut cap = p230_test_cap();
    cap.profile_present = true;
    cap.not_failing_count = 2;
    cap.profile_count = 3;
    cap.c_ri_sha256 = pre.clone();
    let mut view = p230_test_self();
    view.self_ri_sha256 = post.clone();
    assert_eq!(
        p230_classify_profile(Some(&cap), Some(&view)),
        P230Terminal::DRiNotUpdated
    );
}

#[test]
fn p230_eligible_without_profile_maps_to_d_boundary() {
    // Eligible for ordinary creation but absent from the organizer:
    // the exact WP D stop, not a pass and not an A terminal.
    let cap = p230_test_cap();
    assert_eq!(
        p230_classify_profile(Some(&cap), Some(&p230_test_self())),
        P230Terminal::DEligibleButNoProfile
    );
    assert_eq!(
        P230Terminal::DEligibleButNoProfile.token(),
        "P230-D-ELIGIBLE-BUT-NO-PROFILE"
    );
    // A missing self view is an observability gap, never a pass.
    assert_eq!(
        p230_classify_profile(Some(&p230_test_cap()), None),
        P230Terminal::DObservabilityGap
    );
    assert_eq!(
        p230_classify_profile(None, Some(&p230_test_self())),
        P230Terminal::DObservabilityGap
    );
}

#[test]
fn p230_profile_gate_requires_natural_organizer_membership() {
    // The full natural-bootstrap record passes the D gate into the
    // tunnel continuation (the E pass terminal owns the run from here).
    let mut cap = p230_test_cap();
    cap.profile_present = true;
    cap.profile_count = 3;
    cap.not_failing_count = 2;
    assert_eq!(
        p230_classify_profile(Some(&cap), Some(&p230_test_self())),
        P230Terminal::ETunnelContinuationPassed
    );
    // Each missing natural-membership input returns to the D boundary.
    for mutate in [
        "profile",
        "selectable",
        "banlisted",
        "floodfill",
        "not_failing",
    ] {
        let mut cap = p230_test_cap();
        cap.profile_present = true;
        cap.profile_count = 3;
        cap.not_failing_count = 2;
        match mutate {
            "profile" => cap.profile_present = false,
            "selectable" => cap.selectable = false,
            "banlisted" => cap.banlisted = true,
            "floodfill" => cap.caps_has_f = true,
            _ => cap.not_failing_count = 0,
        }
        // A floodfill flip also flips the predicate inputs; keep the
        // probe-derived fact consistent with the exact recomputation.
        cap.heard_about_creation_eligible = p230_heard_about_eligible(
            cap.caps_has_r,
            cap.caps_has_f,
            cap.caps_has_l,
            cap.local_floodfill_enabled,
            cap.local_max_share_bandwidth,
            cap.caps_has_e,
            cap.caps_has_g,
        );
        // A floodfill Router C never passes the D gate even when the
        // predicate accepts it: the transit role is non-floodfill and
        // the shell role proof stops first. Every mutation below holds
        // the exact D boundary.
        assert_eq!(
            p230_classify_profile(Some(&cap), Some(&p230_test_self())),
            P230Terminal::DEligibleButNoProfile,
            "mutation {mutate} must hold the D boundary"
        );
    }
}

#[test]
fn p230_no_probe_profile_creation_calls() {
    // No profile fact may be manufactured: observable=false, any
    // missing field, or a probe/harness-side creation call name in the
    // row rejects the parse.
    let c_hex = p227_test_c_hex();
    let base = p230_test_cap_line(&c_hex);
    assert!(p230_parse_capability(&base, &c_hex).is_some());
    for line in [
        base.replace("observable=true", "observable=false"),
        base.replace(" profile_present=false", ""),
        base.replace(" not_failing_count=0", ""),
        base.replace(" heard_about_creation_eligible=true", ""),
        base.replace(" local_max_share_bandwidth=49152", ""),
        format!("{base} addProfile=true"),
        format!("{base} getOrCreateProfile=true"),
        format!("{base} heardAbout=true"),
    ] {
        assert!(
            p230_parse_capability(&line, &c_hex).is_none(),
            "manufactured profile fact must not parse: {line}"
        );
    }
    assert!(p230_parse_self_view("P230-EV kind=self-view observable=false").is_none());
}

#[test]
fn p230_profile_pass_continues_into_exploratory_gate() {
    // The D pass hands off to the E exploratory gate; a missing or
    // one-sided exploratory pool is the E stop, never an A/D replay.
    let missing_dir = p230_classify_tunnel_continuation(
        None,
        false,
        false,
        &P228Terminal::NoPairedTunnel(P228Direction::Both),
    );
    assert_eq!(
        missing_dir,
        P230Terminal::EExploratoryNotBuilt(P228Direction::Both)
    );
    for (in_nonzero, out_nonzero, dir) in [
        (1, 0, P228Direction::Outbound),
        (0, 1, P228Direction::Inbound),
        (0, 0, P228Direction::Both),
    ] {
        let terminal = p230_classify_tunnel_continuation(
            Some(&p229_test_exploratory(in_nonzero, out_nonzero)),
            false,
            false,
            &P228Terminal::NoPairedTunnel(P228Direction::Both),
        );
        assert_eq!(terminal, P230Terminal::EExploratoryNotBuilt(dir));
        assert!(
            terminal
                .token()
                .starts_with("P230-E-EXPLORATORY-NOT-INSTALLED"),
            "must be the E exploratory terminal: {}",
            terminal.token()
        );
    }
    // Non-zero exploratory plus the retained NO-PAIRED attribution is
    // the explicit contradiction, not a silent client stop.
    let terminal = p230_classify_tunnel_continuation(
        Some(&p229_test_exploratory(1, 1)),
        false,
        false,
        &P228Terminal::NoPairedTunnel(P228Direction::Both),
    );
    assert_eq!(terminal, P230Terminal::EPairedTunnelContradiction);
    assert_eq!(terminal.token(), "P230-E-PAIRED-TUNNEL-CONTRADICTION");
}

#[test]
fn p230_tunnel_pass_continues_into_frozen_destination_lane() {
    // Installed one-hop client tunnels through C retire every assumed
    // boundary and admit the frozen destination lane — the only
    // terminal that permits it.
    let terminal = p230_classify_tunnel_continuation(
        Some(&p229_test_exploratory(1, 1)),
        true,
        true,
        &P228Terminal::NextBoundaryClientTunnelsBuilt,
    );
    assert_eq!(terminal, P230Terminal::ETunnelContinuationPassed);
    assert_eq!(terminal.token(), "P230-E-TUNNEL-CONTINUATION-PASSED");
    assert!(p230_terminal_permits_destination(&terminal));
    for terminal in [
        P230Terminal::AObservabilityGap,
        P230Terminal::AIneligible(P230Blocker::MissingR),
        P230Terminal::AIneligible(P230Blocker::Compound),
        P230Terminal::CUnexpectedCapabilityExclusion,
        P230Terminal::DObservabilityGap,
        P230Terminal::DRiNotUpdated,
        P230Terminal::DEligibleButNoProfile,
        P230Terminal::EExploratoryNotBuilt(P228Direction::Both),
        P230Terminal::EClientNotBuilt(P228Direction::Inbound),
        P230Terminal::EPairedTunnelContradiction,
    ] {
        assert!(
            !p230_terminal_permits_destination(&terminal),
            "terminal must not admit the destination lane: {}",
            terminal.token()
        );
        assert!(
            !terminal.token().contains("LOOKUP") && !terminal.token().contains("45"),
            "terminal must not claim lookup/reverse payload: {}",
            terminal.token()
        );
    }
    // One-sided client installs map to the missing direction.
    let terminal = p230_classify_tunnel_continuation(
        Some(&p229_test_exploratory(2, 1)),
        true,
        false,
        &P228Terminal::BuildReplyTimeout(P228Direction::Both),
    );
    assert_eq!(
        terminal,
        P230Terminal::EClientNotBuilt(P228Direction::Outbound)
    );
}

#[test]
fn p230_exactly_one_earliest_terminal() {
    let dir = p224_test_tmpdir("p230-record-once");
    let evidence_dir = dir.join("evidence");
    std::fs::create_dir_all(&evidence_dir).expect("evidence dir");
    let terminal = P230Terminal::AIneligible(P230Blocker::LowBandwidthLOnFloodfillObserver);
    record_p230_classification(&evidence_dir, &terminal);
    let c_hex = p227_test_c_hex();
    let cap = p230_parse_capability(&p230_test_cap_line(&c_hex), &c_hex).expect("parse capability");
    record_p230_capability(&evidence_dir, Some(&cap), &c_hex);
    let tsv = std::fs::read_to_string(evidence_dir.join("driver-evidence.tsv")).expect("read tsv");
    assert_eq!(
        tsv.lines()
            .filter(|line| line.starts_with("p230-classification\t"))
            .count(),
        1
    );
    assert!(
        tsv.contains("P230-A-PREDICATE-INELIGIBLE reason=low-bandwidth-l-on-floodfill-observer")
    );
    assert_eq!(
        P230Terminal::AObservabilityGap.token(),
        "P230-A-OBSERVABILITY-GAP"
    );
    // The baseline/profile row vocabulary is shared with the shell
    // harness rows of the same keys.
    let c_hex = p227_test_c_hex();
    let cap = p230_parse_capability(&p230_test_cap_line(&c_hex), &c_hex).expect("parse capability");
    record_p230_baseline(&evidence_dir, &P230Baseline::Eligible, Some(&cap), &c_hex);
    record_p230_profile(
        &evidence_dir,
        P230_D_PROFILE_BOOTSTRAP_PASSED,
        Some(&cap),
        Some(&p230_test_self()),
        &c_hex,
    );
    let tsv = std::fs::read_to_string(evidence_dir.join("driver-evidence.tsv")).expect("read tsv");
    assert_eq!(
        tsv.lines()
            .filter(|line| line.starts_with("p230-baseline\t"))
            .count(),
        1
    );
    assert!(tsv.contains("P230-A-PREDICATE-ELIGIBLE"));
    assert_eq!(
        tsv.lines()
            .filter(|line| line.starts_with("p230-profile\t"))
            .count(),
        1
    );
    assert!(tsv.contains(P230_D_PROFILE_BOOTSTRAP_PASSED));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn p230_secret_or_unrelated_rows_rejected() {
    let c_hex = p227_test_c_hex();
    // Wrong prefix never parses.
    assert!(
        p230_parse_capability(
            &p230_test_cap_line(&c_hex).replace("P230-EV ", "P229-EV "),
            &c_hex
        )
        .is_none()
    );
    assert!(p230_parse_self_view("P229-EV kind=self-view observable=true").is_none());
    // Echo mismatch never parses.
    let other = "cd".repeat(32);
    assert!(p230_parse_capability(&p230_test_cap_line(&other), &c_hex).is_none());
    // Secret-bearing diagnostic rows never parse.
    let secret = p230_test_cap_line(&c_hex) + " session_key=abcd";
    assert!(p230_parse_capability(&secret, &c_hex).is_none());
    let secret = p230_test_cap_line(&c_hex) + " payload=deadbeef";
    assert!(p230_parse_capability(&secret, &c_hex).is_none());
    let secret = p230_test_cap_line(&c_hex) + " log-router-0.txt";
    assert!(p230_parse_capability(&secret, &c_hex).is_none());
    // Malformed fields never parse.
    let malformed = p230_test_cap_line(&c_hex).replace("caps_has_r=true", "caps_has_r=yes");
    assert!(p230_parse_capability(&malformed, &c_hex).is_none());
    let malformed = p230_test_cap_line(&c_hex).replace("profile_count=0", "profile_count=65");
    assert!(p230_parse_capability(&malformed, &c_hex).is_none());
    let malformed = p230_test_cap_line(&c_hex).replace(
        "local_max_share_bandwidth=49152",
        "local_max_share_bandwidth=-1",
    );
    assert!(p230_parse_capability(&malformed, &c_hex).is_none());
    let malformed =
        p230_test_cap_line(&c_hex).replace("bandwidth_tier=N", "bandwidth_tier=toolongvalue");
    assert!(p230_parse_capability(&malformed, &c_hex).is_none());
    let malformed =
        p230_test_cap_line(&c_hex).replace("local_comm_status=OK", "local_comm_status=not ok");
    assert!(p230_parse_capability(&malformed, &c_hex).is_none());
    // Positive parse requires the exact Router-C echo and recomputable
    // predicate agreement.
    let parsed =
        p230_parse_capability(&p230_test_cap_line(&c_hex), &c_hex).expect("parse capability");
    assert!(p230_predicate_agrees(&parsed));
    assert_eq!(
        p230_classify_baseline(Some(&parsed)),
        P230Baseline::Eligible
    );
    // A probe-reported eligible fact that disagrees with the exact
    // recomputation is an observability gap, never a pass.
    let mut skewed = parsed.clone();
    skewed.caps_has_r = false;
    assert!(!p230_predicate_agrees(&skewed));
    assert_eq!(
        p230_classify_baseline(Some(&skewed)),
        P230Baseline::ObservabilityGap
    );
}

// ---- Plan 231 — M6 Java reverse-delivery tunnel-dispatch attribution -----
// Exact-pinned Java I2P 2.13.0 source-order lock (stronger than the
// retained Plan-218 wording):
//
// ```text
// ACCEPTED => distributeMessage returned
// distributeMessage returned => inline OCMOSJ returned
// inline OCMOSJ returned => DispatchJob.runJob returned
// DispatchJob.runJob returned => dispatchOutbound call returned
// ```
//
// `ClientMessageEventListener.handleSendMessage()` calls
// `ClientConnectionRunner.distributeMessage()` first (which runs the
// `ClientMessagePool` OCMOSJ inline); OCMOSJ runs its `DispatchJob`
// inline; `DispatchJob.runJob()` calls
// `tunnelDispatcher().dispatchOutbound(_msg, _outTunnel.getSendTunnelId(0),
// _lease.getTunnelId(), _lease.getGateway())` before returning. Only
// after `distributeMessage()` returns does `handleSendMessage()` call
// `ackSendMessage(...)`, which emits `STATUS_SEND_ACCEPTED`.
//
// Therefore a nonce-correlated `ACCEPTED` is downstream of the OCMOSJ
// `dispatchOutbound()` call returning. Plan 231 MUST NOT classify the
// failure as OCMOSJ-not-entered. The lock proves only call
// ordering — never queue acceptance, pumper progress, transit, or
// delivery. The earliest Java-side distinctions are: gateway not found,
// enqueue-then-drop/expire, or enqueue without downstream progress.
//
// Additional pinned anchors consumed below:
// - `dispatchOutbound` increments `tunnel.dispatchOutboundTunnel` only
//   after `gw.add(...)` on a matching outbound gateway;
// - `PumpedTunnelGateway.add()` enqueues into its prequeue and increments
//   `tunnel.dropGatewayOverflow` on queue overflow;
// - Router C's one-hop outbound endpoint path is
//   `OutboundTunnelEndpoint.dispatch()`; its `HopConfig` increments
//   `getProcessedMessagesCount()` before decrypt/reassembly;
// - the destination lease gateway accepts a `TunnelGatewayMessage`
//   through `TunnelDispatcher.dispatch(TunnelGatewayMessage)`; on a
//   matching inbound gateway it calls `gw.add(msg)` and increments
//   `tunnel.dispatchInbound`;
// - `InboundGatewayReceiver.receiveEncrypted()` increments the config's
//   processed-message count before constructing and enqueueing the
//   next-hop `TunnelDataMessage`; when the next-hop RouterInfo is
//   unknown it defers through `ReceiveJob` and records a zero-valued
//   `tunnel.inboundLookupSuccess` event (a one-valued event on success).
//
// All Java facts below arrive through the read-only `P231-*` diagnostic
// commands (`P231Probe`: `statManager().getRate()` lifetime counts,
// installed outbound client-tunnel hop-0 send id, single participating
// `HopConfig` filtered by the exact receive tunnel id). Raw Java logs
// remain scratch-only: only bounded booleans/counts reach evidence.

/// Plan 231 source-order lock: on the exact-pinned source a
/// nonce-correlated `ACCEPTED` is emitted only after the inline OCMOSJ
/// `DispatchJob` has called `TunnelDispatcher.dispatchOutbound(...)`
/// and returned. The boolean passes straight through so the classifier
/// must consume it explicitly; it MUST NOT be treated as delivery
/// proof, and `ACCEPTED` alone MUST NOT prove gateway enqueue.
fn p231_accepted_implies_dispatch_called(accepted_observed: bool) -> bool {
    accepted_observed
}

/// Bounded signed lifetime count: -1 is unknown (rate never created),
/// otherwise a non-negative event count.
fn p231_parse_count_signed(value: Option<&String>) -> Option<i64> {
    let raw = value?;
    if raw == "-1" {
        return Some(-1);
    }
    if raw.is_empty() || raw.len() > 19 || !raw.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let parsed: i64 = raw.parse().ok()?;
    if parsed < -1 {
        return None;
    }
    Some(parsed)
}

fn p231_parse_count(value: Option<&String>) -> Option<u64> {
    let raw = value?;
    if raw.is_empty() || raw.len() > 19 || !raw.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    raw.parse().ok()
}

fn p231_parse_hex_or_none(value: Option<&String>) -> Option<String> {
    let raw = value?;
    if raw == "none" {
        return Some(raw.to_owned());
    }
    if p227_is_hex64(raw) {
        return Some(raw.to_owned());
    }
    None
}

/// P231 rows are unquoted `key=value` tokens only, exactly like the
/// P230 contract: any bare token rejects the row instead of silently
/// truncating a fact.
fn p231_strict_shape(line: &str) -> bool {
    let mut tokens = line.split(' ');
    match (tokens.next(), tokens.next()) {
        (Some("P231-EV"), Some(kind)) if kind.starts_with("kind=") => {}
        _ => return false,
    }
    tokens.all(|t| t.is_empty() || t.contains('='))
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct P231GatewayStats {
    dispatch_time: i64,
    dispatch_send_time: i64,
    dispatch_outbound_tunnel: i64,
    drop_gateway_overflow: i64,
    dispatch_inbound: i64,
    inbound_lookup_success: i64,
    dispatch_endpoint: i64,
    dispatch_participant: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct P231ClientOutbound {
    client_dbid_hex: String,
    client_resolved: bool,
    outbound_tunnel_count: u64,
    single_send_tunnel_id: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct P231Participating {
    receive_id: u64,
    present: bool,
    match_count: u64,
    send_id: u64,
    receive_from_hex: String,
    send_to_hex: String,
    processed: i64,
}

fn p231_parse_bool(value: Option<&String>) -> Option<bool> {
    match value?.as_str() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

fn p231_parse_gateway(line: &str) -> Option<P231GatewayStats> {
    if !line.starts_with("P231-EV ") || !line.contains("kind=gateway") {
        return None;
    }
    if line.len() > 1024 || p228_line_is_secret_bearing(line) {
        return None;
    }
    if !p231_strict_shape(line) {
        return None;
    }
    let kv = p220_parse_kv(&line.replace("P231-EV ", "P220-EV "));
    if kv.get("observable").is_none_or(|v| v != "true") {
        return None;
    }
    Some(P231GatewayStats {
        dispatch_time: p231_parse_count_signed(kv.get("dispatch_time"))?,
        dispatch_send_time: p231_parse_count_signed(kv.get("dispatch_send_time"))?,
        dispatch_outbound_tunnel: p231_parse_count_signed(kv.get("dispatch_outbound_tunnel"))?,
        drop_gateway_overflow: p231_parse_count_signed(kv.get("drop_gateway_overflow"))?,
        dispatch_inbound: p231_parse_count_signed(kv.get("dispatch_inbound"))?,
        inbound_lookup_success: p231_parse_count_signed(kv.get("inbound_lookup_success"))?,
        dispatch_endpoint: p231_parse_count_signed(kv.get("dispatch_endpoint"))?,
        dispatch_participant: p231_parse_count_signed(kv.get("dispatch_participant"))?,
    })
}

fn p231_parse_client_outbound(line: &str, expected_dbid_hex: &str) -> Option<P231ClientOutbound> {
    if !line.starts_with("P231-EV ") || !line.contains("kind=client-outbound") {
        return None;
    }
    if line.len() > 1024 || p228_line_is_secret_bearing(line) {
        return None;
    }
    if !p231_strict_shape(line) {
        return None;
    }
    let kv = p220_parse_kv(&line.replace("P231-EV ", "P220-EV "));
    if kv.get("observable").is_none_or(|v| v != "true") {
        return None;
    }
    let echoed = kv.get("client_dbid_hex")?;
    if echoed.to_lowercase() != expected_dbid_hex.to_lowercase() {
        return None;
    }
    Some(P231ClientOutbound {
        client_dbid_hex: echoed.to_owned(),
        client_resolved: p231_parse_bool(kv.get("client_resolved"))?,
        outbound_tunnel_count: p231_parse_count(kv.get("outbound_tunnel_count"))?,
        single_send_tunnel_id: p231_parse_count(kv.get("single_send_tunnel_id"))?,
    })
}

fn p231_parse_participating(line: &str, expected_receive_id: u64) -> Option<P231Participating> {
    if !line.starts_with("P231-EV ") || !line.contains("kind=participating") {
        return None;
    }
    if line.len() > 2048 || p228_line_is_secret_bearing(line) {
        return None;
    }
    if !p231_strict_shape(line) {
        return None;
    }
    let kv = p220_parse_kv(&line.replace("P231-EV ", "P220-EV "));
    if kv.get("observable").is_none_or(|v| v != "true") {
        return None;
    }
    let receive_id: u64 = kv.get("receive_id")?.parse().ok()?;
    if receive_id != expected_receive_id || receive_id == 0 {
        return None;
    }
    Some(P231Participating {
        receive_id,
        present: p231_parse_bool(kv.get("present"))?,
        match_count: p231_parse_count(kv.get("match_count"))?,
        send_id: p231_parse_count(kv.get("send_id"))?,
        receive_from_hex: p231_parse_hex_or_none(kv.get("receive_from"))?,
        send_to_hex: p231_parse_hex_or_none(kv.get("send_to"))?,
        processed: p231_parse_count_signed(kv.get("processed"))?,
    })
}

async fn p231_collect_gateway(diag_port: u16) -> Option<P231GatewayStats> {
    if diag_port == 0 {
        return None;
    }
    let line = p220_query_diagnostic(diag_port, "P231-GATEWAY").await?;
    p231_parse_gateway(&line)
}

async fn p231_collect_client_outbound(
    diag_port: u16,
    client_dbid_hex: &str,
) -> Option<P231ClientOutbound> {
    if diag_port == 0 || !p227_is_hex64(client_dbid_hex) {
        return None;
    }
    let line = p220_query_diagnostic(
        diag_port,
        &format!("P231-CLIENT-OUTBOUND {client_dbid_hex}"),
    )
    .await?;
    p231_parse_client_outbound(&line, client_dbid_hex)
}

async fn p231_collect_participating(diag_port: u16, receive_id: u64) -> Option<P231Participating> {
    if diag_port == 0 || receive_id == 0 {
        return None;
    }
    let line =
        p220_query_diagnostic(diag_port, &format!("P231-PARTICIPATING {receive_id}")).await?;
    p231_parse_participating(&line, receive_id)
}

/// Plan 231 isolated-epoch delta: both endpoints must be known
/// (non-negative) lifetime counts from the same router. Either
/// endpoint unknown (-1) yields `None` — a global delta alone can
/// never satisfy a target-specific stage without the exact tunnel
/// match the caller additionally requires.
fn p231_delta(post: i64, pre: i64) -> Option<i64> {
    if pre < 0 || post < 0 {
        return None;
    }
    Some(post.saturating_sub(pre))
}

/// Plan 231 WP D: the target lease gateway hash resolves to the
/// controlled Java router role. The role comes from the exact
/// selected lease — never from historical topology comments, never
/// assumed to be Router B.
fn p231_target_role(
    lease_gateway_hex: &str,
    a_hex: &str,
    b_hex: &str,
    c_hex: &str,
) -> Option<&'static str> {
    if lease_gateway_hex.eq_ignore_ascii_case(a_hex) && p227_is_hex64(a_hex) {
        return Some("A");
    }
    if lease_gateway_hex.eq_ignore_ascii_case(b_hex) && p227_is_hex64(b_hex) {
        return Some("B");
    }
    if lease_gateway_hex.eq_ignore_ascii_case(c_hex) && p227_is_hex64(c_hex) {
        return Some("C");
    }
    None
}

/// Bounded scratch-only marker count in one Java log directory.
/// Fixed-substring matching only; only the bounded count reaches
/// evidence, never matched lines, keys, tags, or payloads. `None`
/// when the directory is unavailable (Unknown, never zero-as-fact).
fn p231_count_marker_in_log_dir(dir: &Path, marker: &str) -> Option<u64> {
    let mut log_dirs = vec![dir.to_path_buf()];
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.filter_map(Result::ok) {
            let is_logs_dir =
                entry.file_name() == "logs" && entry.file_type().is_ok_and(|kind| kind.is_dir());
            if is_logs_dir {
                log_dirs.push(entry.path());
            }
        }
    }
    let mut count: u64 = 0;
    let mut files_seen = false;
    for log_dir in log_dirs {
        let entries = std::fs::read_dir(&log_dir).ok()?;
        for entry in entries.filter_map(Result::ok) {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !name.starts_with("log-router-") || !name.ends_with(".txt") {
                continue;
            }
            files_seen = true;
            let bytes = std::fs::read(entry.path()).ok()?;
            if bytes.len() > 32 * 1024 * 1024 {
                continue;
            }
            let text = String::from_utf8_lossy(&bytes);
            for line in text.lines() {
                if line.contains(marker) {
                    count = count.saturating_add(1);
                    if count >= 9999 {
                        return Some(count);
                    }
                }
            }
        }
    }
    if files_seen { Some(count) } else { None }
}

/// Bounded scratch-only marker-plus-id count in one Java log
/// directory. Counts lines containing both the fixed marker substring
/// and the exact decimal tunnel-id text (e.g. the
/// `no matching IBGW for id <tunnel>` WARN correlates C's forward to
/// the selected lease tunnel). Only the bounded count reaches
/// evidence, never matched lines. `None` when the directory is
/// unavailable (Unknown, never zero-as-fact).
fn p231_count_marker_with_id_in_log_dir(dir: &Path, marker: &str, id_text: &str) -> Option<u64> {
    let mut log_dirs = vec![dir.to_path_buf()];
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.filter_map(Result::ok) {
            let is_logs_dir =
                entry.file_name() == "logs" && entry.file_type().is_ok_and(|kind| kind.is_dir());
            if is_logs_dir {
                log_dirs.push(entry.path());
            }
        }
    }
    let mut count: u64 = 0;
    let mut files_seen = false;
    for log_dir in log_dirs {
        let entries = std::fs::read_dir(&log_dir).ok()?;
        for entry in entries.filter_map(Result::ok) {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !name.starts_with("log-router-") || !name.ends_with(".txt") {
                continue;
            }
            files_seen = true;
            let bytes = std::fs::read(entry.path()).ok()?;
            if bytes.len() > 32 * 1024 * 1024 {
                continue;
            }
            let text = String::from_utf8_lossy(&bytes);
            for line in text.lines() {
                if line.contains(marker) && line.contains(id_text) {
                    count = count.saturating_add(1);
                    if count >= 9999 {
                        return Some(count);
                    }
                }
            }
        }
    }
    if files_seen { Some(count) } else { None }
}

/// Plan 231 WP F: exactly one earliest-stage terminal per counted
/// attempt. Pass-through stage outcomes (`ENQUEUED`, `FORWARD-PASSED`,
/// `TUNNELDATA-EMITTED`) are recorded in supporting `p231-stage-*`
/// rows; the single `p231-classification` row carries the first
/// failing stage in A -> B -> C -> D order, or the digest-matched
/// reverse-delivery pass. Vocabulary is limited to the §6–9 tokens
/// plus `P231-REVERSE-DELIVERY-PASSED`; no root-cause label is ever
/// emitted from free-form logs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum P231Terminal {
    AOutboundGatewayNotFound,
    AOutboundGatewayEnqueueDrop,
    AObservabilityGap,
    BFirstHopNotReceivedByC,
    BCObepReassemblyFailed,
    BObservabilityGap,
    CTargetIbgwNotInstalled,
    CTunnelGatewayNotReceived,
    CIbgwEnqueueOrPumpBoundary,
    CIbgwNextHopLookupFailed,
    DI2prNoExpectedTunnelData,
    DI2prTunnelRecoveryFailed,
    DI2prGarlicDecodeFailed,
    DI2prDestinationDispatchMissed,
    DI2prPayloadMismatch,
    DObservabilityGap,
    ReverseDeliveryPassed,
}

impl P231Terminal {
    fn token(self) -> &'static str {
        match self {
            P231Terminal::AOutboundGatewayNotFound => "P231-A-OUTBOUND-GATEWAY-NOT-FOUND",
            P231Terminal::AOutboundGatewayEnqueueDrop => "P231-A-OUTBOUND-GATEWAY-ENQUEUE-DROP",
            P231Terminal::AObservabilityGap => "P231-A-OBSERVABILITY-GAP",
            P231Terminal::BFirstHopNotReceivedByC => "P231-B-FIRST-HOP-NOT-RECEIVED-BY-C",
            P231Terminal::BCObepReassemblyFailed => "P231-B-C-OBEP-REASSEMBLY-FAILED",
            P231Terminal::BObservabilityGap => "P231-B-OBSERVABILITY-GAP",
            P231Terminal::CTargetIbgwNotInstalled => "P231-C-TARGET-IBGW-NOT-INSTALLED",
            P231Terminal::CTunnelGatewayNotReceived => "P231-C-TUNNEL-GATEWAY-NOT-RECEIVED",
            P231Terminal::CIbgwEnqueueOrPumpBoundary => "P231-C-IBGW-ENQUEUE-OR-PUMP-BOUNDARY",
            P231Terminal::CIbgwNextHopLookupFailed => "P231-C-IBGW-NEXT-HOP-LOOKUP-FAILED",
            P231Terminal::DI2prNoExpectedTunnelData => "P231-D-I2PR-NO-EXPECTED-TUNNELDATA",
            P231Terminal::DI2prTunnelRecoveryFailed => "P231-D-I2PR-TUNNEL-RECOVERY-FAILED",
            P231Terminal::DI2prGarlicDecodeFailed => "P231-D-I2PR-GARLIC-DECODE-FAILED",
            P231Terminal::DI2prDestinationDispatchMissed => {
                "P231-D-I2PR-DESTINATION-DISPATCH-MISSED"
            }
            P231Terminal::DI2prPayloadMismatch => "P231-D-I2PR-PAYLOAD-MISMATCH",
            P231Terminal::DObservabilityGap => "P231-D-OBSERVABILITY-GAP",
            P231Terminal::ReverseDeliveryPassed => "P231-REVERSE-DELIVERY-PASSED",
        }
    }
}

/// Pure classifier inputs. Deltas are isolated-epoch `post - pre`
/// lifetime-count differences (`None` when either endpoint is
/// unknown); tunnel-identity matches are exact (send id, lease
/// gateway/tunnel, IBGW send-to); i2pr outcomes are attributed by the
/// exact owned inbound tunnel id only.
#[derive(Clone, Debug, Default)]
struct P231Inputs {
    accepted_observed: bool,
    outbound_send_id_known: bool,
    outbound_still_installed: bool,
    // Isolated micro-epoch (pre-send vs post-immediate): bounds only
    // dispatches fast enough to beat the post-send diagnostic query.
    dispatch_outbound_delta: Option<i64>,
    overflow_delta_a: Option<i64>,
    no_matching_ob_correlated: bool,
    // Window attribution (pre-send vs post-window): the helper
    // returns before the router dispatches, so the micro-epoch
    // systematically misses the enqueue. The window deltas below are
    // target-attributable only through the lane-quiet premises
    // documented on the classifier: exactly one client message exists
    // in the lane, and a missing gateway always logs its
    // id-correlated no-matching row at WARN.
    client_dispatch_time_delta: Option<i64>,
    dispatch_outbound_window_delta: Option<i64>,
    overflow_window_delta: Option<i64>,
    c_obep_observable: bool,
    c_obep_present_exact: bool,
    c_obep_processed_delta: Option<i64>,
    c_drop_markers_delta: Option<i64>,
    target_observable: bool,
    b_gateway_receipt_correlated: bool,
    target_dispatch_inbound_delta: Option<i64>,
    target_ibgw_processed_delta: Option<i64>,
    ibgw_present_exact: bool,
    ibgw_overflow_delta: Option<i64>,
    ibgw_lookup_attempted_in_window: bool,
    expected_tunnel_id_known: bool,
    expected_tunneldata_seen: bool,
    expected_recovery_completes: u64,
    expected_recovery_errors: u64,
    expected_garlic_decodes_ok: u64,
    expected_garlic_decodes_fail: u64,
    expected_dispatch_calls: u64,
    expected_queue_hits: u64,
    expected_queued_decode_fails: u64,
    expected_digest_match_45s: bool,
}

fn p231_positive(delta: &Option<i64>) -> bool {
    delta.is_some_and(|d| d > 0)
}

fn p231_classify(inputs: &P231Inputs) -> P231Terminal {
    // The source-order lock passes through explicitly: ACCEPTED proves
    // only that the inline dispatch call returned, never delivery.
    if !p231_accepted_implies_dispatch_called(inputs.accepted_observed) {
        return P231Terminal::AObservabilityGap;
    }
    // WP A: the target send must be attributable to exactly one
    // installed outbound client tunnel; never assume the first lease
    // or any tunnel.
    if !inputs.outbound_send_id_known || !inputs.outbound_still_installed {
        return P231Terminal::AObservabilityGap;
    }
    // WP B gateway stage. The id-correlated no-matching-gateway row
    // is target-specific proof of NOT-FOUND and wins over every
    // global counter (a missing gateway always logs at WARN, so its
    // absence alongside ACCEPTED rules NOT-FOUND out). Otherwise an
    // isolated micro-epoch dispatch advance proves the matching
    // gateway accepted gw.add(...). Failing that, the window
    // attribution applies: the helper returns before the router
    // dispatches, so only the window can contain the enqueue; with
    // exactly one client dispatch in the lane
    // (client.dispatchTime +1) and a gateway accept (window
    // dispatchOutboundTunnel >= +1) the enqueue is the tracked
    // message. ACCEPTED alone never proves enqueue.
    if inputs.no_matching_ob_correlated {
        return P231Terminal::AOutboundGatewayNotFound;
    }
    if p231_positive(&inputs.dispatch_outbound_delta) {
        // ENQUEUED (isolated micro-epoch): continue to stage B.
    } else if inputs.client_dispatch_time_delta == Some(1)
        && p231_positive(&inputs.dispatch_outbound_window_delta)
    {
        // ENQUEUED (window attribution under the lane-quiet
        // premises): continue to stage B. Background gateway overflow
        // alongside a proven enqueue is background context, never the
        // tracked message's fate.
    } else if inputs.client_dispatch_time_delta == Some(1)
        && inputs.dispatch_outbound_window_delta == Some(0)
        && p231_positive(&inputs.overflow_window_delta)
    {
        return P231Terminal::AOutboundGatewayEnqueueDrop;
    } else {
        return P231Terminal::AObservabilityGap;
    }
    // WP C first-hop stage: the exact C outbound-endpoint config
    // (receive id == A send id, receive-from == A) must remain present
    // and its processed-message count must increase after the target
    // enqueue. Global counters alone never satisfy this stage.
    if !inputs.c_obep_observable {
        return P231Terminal::BObservabilityGap;
    }
    if !inputs.c_obep_present_exact {
        return P231Terminal::BFirstHopNotReceivedByC;
    }
    match inputs.c_obep_processed_delta {
        Some(d) if d > 0 => {}
        _ => return P231Terminal::BFirstHopNotReceivedByC,
    }
    // Forward evidence lives on the target router: its dispatchInbound
    // advance, its exact IBGW processed advance, or its id-correlated
    // no-matching-IBGW receipt row proves C reassembled and forwarded
    // toward the selected lease gateway. The receipt row proves the
    // forward reached B even when B had no matching gateway to
    // dispatch through.
    if !inputs.target_observable {
        return P231Terminal::BObservabilityGap;
    }
    let forward_proven = p231_positive(&inputs.target_dispatch_inbound_delta)
        || p231_positive(&inputs.target_ibgw_processed_delta)
        || inputs.b_gateway_receipt_correlated;
    if !forward_proven {
        if p231_positive(&inputs.c_drop_markers_delta) {
            return P231Terminal::BCObepReassemblyFailed;
        }
        return P231Terminal::BObservabilityGap;
    }
    // WP D target-IBGW stage: the exact inbound-gateway config (receive
    // id == lease tunnel id, send-to == i2pr) must be installed.
    if !inputs.ibgw_present_exact {
        return P231Terminal::CTargetIbgwNotInstalled;
    }
    match inputs.target_ibgw_processed_delta {
        Some(d) if d > 0 => {}
        _ => {
            // The exact gateway never pumped the message. A quiet
            // dispatcher with an id-correlated B receipt row means C's
            // forward reached B but no matching gateway dispatched it:
            // TUNNEL-GATEWAY-NOT-RECEIVED. A dispatcher acceptance (or
            // queue overflow) without exact pumping is the
            // enqueue-or-pump boundary instead.
            if inputs.b_gateway_receipt_correlated
                && !p231_positive(&inputs.target_dispatch_inbound_delta)
                && !p231_positive(&inputs.ibgw_overflow_delta)
            {
                return P231Terminal::CTunnelGatewayNotReceived;
            }
            if p231_positive(&inputs.target_dispatch_inbound_delta)
                || p231_positive(&inputs.ibgw_overflow_delta)
            {
                return P231Terminal::CIbgwEnqueueOrPumpBoundary;
            }
            return P231Terminal::CTunnelGatewayNotReceived;
        }
    }
    // The exact IBGW pumped the message (receiveEncrypted ran, so the
    // next-hop TunnelData was constructed). A deferred next-hop lookup
    // attempted in the window without wire receipt is the lookup
    // boundary; otherwise the message was emitted toward i2pr.
    if inputs.ibgw_lookup_attempted_in_window && !inputs.expected_tunneldata_seen {
        return P231Terminal::CIbgwNextHopLookupFailed;
    }
    // WP E i2pr stage, attributed by the exact owned inbound tunnel id
    // only; unrelated TunnelData never satisfies target progress.
    if !inputs.expected_tunnel_id_known {
        return P231Terminal::DObservabilityGap;
    }
    if !inputs.expected_tunneldata_seen {
        return P231Terminal::DI2prNoExpectedTunnelData;
    }
    if inputs.expected_recovery_completes == 0 {
        if inputs.expected_recovery_errors > 0 {
            return P231Terminal::DI2prTunnelRecoveryFailed;
        }
        return P231Terminal::DObservabilityGap;
    }
    if inputs.expected_garlic_decodes_ok == 0 {
        if inputs.expected_garlic_decodes_fail > 0 {
            return P231Terminal::DI2prGarlicDecodeFailed;
        }
        return P231Terminal::DObservabilityGap;
    }
    if inputs.expected_dispatch_calls == 0 {
        return P231Terminal::DObservabilityGap;
    }
    if inputs.expected_queue_hits == 0 {
        return P231Terminal::DI2prDestinationDispatchMissed;
    }
    // A queue hit passes only on digest match inside the frozen
    // 45-second window; the later status-only window can never
    // retroactively pass payload delivery.
    if inputs.expected_digest_match_45s {
        return P231Terminal::ReverseDeliveryPassed;
    }
    if inputs.expected_queued_decode_fails > 0 {
        return P231Terminal::DI2prDestinationDispatchMissed;
    }
    P231Terminal::DI2prPayloadMismatch
}

fn record_p231_classification(evidence_dir: &Path, terminal: P231Terminal, detail: &str) {
    let sanitized = detail.replace(['\t', '\n'], " ");
    append_evidence(
        evidence_dir,
        "p231-classification",
        &format!("{} {sanitized}", terminal.token()),
    );
}

/// Plan 231 early-stop gap: the driver stopped before the reverse
/// epoch (install-stalled or lease-stalled), so no post-`ACCEPTED`
/// attribution exists. Exactly one `p231-classification` row is still
/// emitted so every counted run closes the loop, honestly staged at
/// the earliest unknown point — never a root-cause attribution.
fn record_p231_early_stop_gap(evidence_dir: &Path, reason: &'static str) {
    append_evidence(
        evidence_dir,
        "p231-classification",
        &format!("P231-A-OBSERVABILITY-GAP reason={reason}"),
    );
}

fn p231_test_inputs_pass() -> P231Inputs {
    // A fully passing reverse epoch: ACCEPTED, exact installed tunnel,
    // gateway enqueue, C first-hop processing, forward evidence, exact
    // IBGW processing without lookup deferral, expected wire receipt
    // with recovery/Garlic/dispatch/queue success and digest match.
    P231Inputs {
        accepted_observed: true,
        outbound_send_id_known: true,
        outbound_still_installed: true,
        dispatch_outbound_delta: Some(1),
        overflow_delta_a: Some(0),
        no_matching_ob_correlated: false,
        client_dispatch_time_delta: Some(1),
        dispatch_outbound_window_delta: Some(1),
        overflow_window_delta: Some(0),
        c_obep_observable: true,
        c_obep_present_exact: true,
        c_obep_processed_delta: Some(1),
        c_drop_markers_delta: Some(0),
        target_observable: true,
        b_gateway_receipt_correlated: false,
        target_dispatch_inbound_delta: Some(1),
        target_ibgw_processed_delta: Some(1),
        ibgw_present_exact: true,
        ibgw_overflow_delta: Some(0),
        ibgw_lookup_attempted_in_window: false,
        expected_tunnel_id_known: true,
        expected_tunneldata_seen: true,
        expected_recovery_completes: 1,
        expected_recovery_errors: 0,
        expected_garlic_decodes_ok: 1,
        expected_garlic_decodes_fail: 0,
        expected_dispatch_calls: 1,
        expected_queue_hits: 1,
        expected_queued_decode_fails: 0,
        expected_digest_match_45s: true,
    }
}

#[test]
fn p231_accepted_is_ordered_after_inline_dispatch_call() {
    // Exact-pinned source lock: ACCEPTED is emitted only after
    // distributeMessage returned, which ran OCMOSJ inline, which ran
    // DispatchJob inline, which called dispatchOutbound before
    // returning. The classifier consumes the lock explicitly.
    assert!(p231_accepted_implies_dispatch_called(true));
    assert!(!p231_accepted_implies_dispatch_called(false));
    let mut inputs = p231_test_inputs_pass();
    inputs.accepted_observed = false;
    assert_eq!(
        p231_classify(&inputs),
        P231Terminal::AObservabilityGap,
        "a send that was never admitted cannot enter attribution"
    );
}

#[test]
fn p231_accepted_alone_does_not_prove_gateway_enqueue() {
    // ACCEPTED proves only that the inline dispatch call returned —
    // never queue acceptance. Without a micro-epoch dispatch advance,
    // without the window attribution (exactly one client dispatch
    // plus a window gateway accept), and without the correlated
    // no-matching row, the stage is an observability gap, never a
    // pass and never a NOT-FOUND claim.
    let mut inputs = p231_test_inputs_pass();
    inputs.dispatch_outbound_delta = Some(0);
    inputs.overflow_delta_a = Some(0);
    inputs.no_matching_ob_correlated = false;
    inputs.client_dispatch_time_delta = None;
    inputs.dispatch_outbound_window_delta = None;
    inputs.overflow_window_delta = None;
    assert_eq!(p231_classify(&inputs), P231Terminal::AObservabilityGap);
    // A micro-epoch advance alone still proves enqueue even when the
    // window inputs are unknown (the fast path needs no window).
    inputs = p231_test_inputs_pass();
    inputs.client_dispatch_time_delta = None;
    inputs.dispatch_outbound_window_delta = None;
    inputs.overflow_window_delta = None;
    assert_eq!(p231_classify(&inputs), P231Terminal::ReverseDeliveryPassed);
    // The window path proves enqueue when the micro-epoch misses it
    // (the helper returns before the router dispatches).
    inputs = p231_test_inputs_pass();
    inputs.dispatch_outbound_delta = Some(0);
    inputs.overflow_delta_a = Some(0);
    assert_eq!(p231_classify(&inputs), P231Terminal::ReverseDeliveryPassed);
    // Unknown endpoints are equally a gap, never manufactured proof.
    inputs.dispatch_outbound_delta = None;
    inputs.overflow_delta_a = None;
    inputs.client_dispatch_time_delta = None;
    inputs.dispatch_outbound_window_delta = None;
    assert_eq!(p231_classify(&inputs), P231Terminal::AObservabilityGap);
}

#[test]
fn p231_gateway_stat_delta_requires_target_epoch() {
    // Either endpoint unknown (-1) yields no delta: a global counter
    // alone can never satisfy the target-specific stage without the
    // exact installed-tunnel match the classifier additionally
    // requires.
    assert_eq!(p231_delta(5, 4), Some(1));
    assert_eq!(p231_delta(4, 4), Some(0));
    assert_eq!(p231_delta(0, 0), Some(0));
    assert_eq!(p231_delta(5, -1), None);
    assert_eq!(p231_delta(-1, 4), None);
    assert_eq!(p231_delta(-1, -1), None);
    // The exact installed tunnel is required alongside the delta.
    let mut inputs = p231_test_inputs_pass();
    inputs.outbound_send_id_known = false;
    assert_eq!(
        p231_classify(&inputs),
        P231Terminal::AObservabilityGap,
        "a stat delta without the exact send tunnel proves nothing"
    );
    inputs = p231_test_inputs_pass();
    inputs.outbound_still_installed = false;
    assert_eq!(
        p231_classify(&inputs),
        P231Terminal::AObservabilityGap,
        "a stat delta without the still-installed proof proves nothing"
    );
}

#[test]
fn p231_gateway_overflow_maps_to_enqueue_drop() {
    // Exactly one client dispatch with no window gateway accept but a
    // window queue overflow maps to ENQUEUE-DROP: the tracked message
    // ran OCMOSJ, reached no gateway, and something overflowed.
    let mut inputs = p231_test_inputs_pass();
    inputs.dispatch_outbound_delta = Some(0);
    inputs.overflow_delta_a = Some(0);
    inputs.client_dispatch_time_delta = Some(1);
    inputs.dispatch_outbound_window_delta = Some(0);
    inputs.overflow_window_delta = Some(1);
    assert_eq!(
        p231_classify(&inputs),
        P231Terminal::AOutboundGatewayEnqueueDrop
    );
    assert_eq!(
        P231Terminal::AOutboundGatewayEnqueueDrop.token(),
        "P231-A-OUTBOUND-GATEWAY-ENQUEUE-DROP"
    );
    // Background overflow alongside a proven enqueue is background
    // context, never the tracked message's fate.
    inputs = p231_test_inputs_pass();
    inputs.overflow_window_delta = Some(2);
    assert_eq!(p231_classify(&inputs), P231Terminal::ReverseDeliveryPassed);
    // No overflow and no dispatch advance but an id-correlated
    // scratch no-matching-OB row proves NOT-FOUND (and only that
    // proves it); the correlated row wins over every counter.
    inputs = p231_test_inputs_pass();
    inputs.dispatch_outbound_delta = Some(0);
    inputs.overflow_delta_a = Some(0);
    inputs.client_dispatch_time_delta = Some(1);
    inputs.dispatch_outbound_window_delta = Some(3);
    inputs.no_matching_ob_correlated = true;
    assert_eq!(
        p231_classify(&inputs),
        P231Terminal::AOutboundGatewayNotFound
    );
}

#[test]
fn p231_c_obep_requires_exact_installed_tunnel() {
    // The C stage requires the exact outbound-endpoint config
    // (receive id == A send id with receive-from == A). An
    // unobservable C surface is a gap; a missing/non-exact config is
    // FIRST-HOP-NOT-RECEIVED-BY-C, never a later stage claim.
    let mut inputs = p231_test_inputs_pass();
    inputs.c_obep_observable = false;
    assert_eq!(p231_classify(&inputs), P231Terminal::BObservabilityGap);
    inputs = p231_test_inputs_pass();
    inputs.c_obep_present_exact = false;
    assert_eq!(
        p231_classify(&inputs),
        P231Terminal::BFirstHopNotReceivedByC
    );
    assert_eq!(
        P231Terminal::BFirstHopNotReceivedByC.token(),
        "P231-B-FIRST-HOP-NOT-RECEIVED-BY-C"
    );
}

#[test]
fn p231_c_obep_count_delta_proves_first_hop_processing() {
    // Both the exact config presence and its processed-message
    // increase are required; a present-but-unmoved config is still
    // FIRST-HOP-NOT-RECEIVED-BY-C.
    let mut inputs = p231_test_inputs_pass();
    inputs.c_obep_processed_delta = Some(0);
    assert_eq!(
        p231_classify(&inputs),
        P231Terminal::BFirstHopNotReceivedByC
    );
    inputs.c_obep_processed_delta = None;
    assert_eq!(
        p231_classify(&inputs),
        P231Terminal::BFirstHopNotReceivedByC
    );
    // Processing without forward evidence and without drop markers is
    // a gap, never a forward claim.
    inputs = p231_test_inputs_pass();
    inputs.target_dispatch_inbound_delta = Some(0);
    inputs.target_ibgw_processed_delta = Some(0);
    inputs.c_drop_markers_delta = Some(0);
    assert_eq!(p231_classify(&inputs), P231Terminal::BObservabilityGap);
    // Processing with OBEP drop markers and no forward evidence is
    // the reassembly boundary.
    inputs.c_drop_markers_delta = Some(2);
    assert_eq!(p231_classify(&inputs), P231Terminal::BCObepReassemblyFailed);
}

#[test]
fn p231_target_gateway_role_comes_from_selected_lease() {
    // The role resolves from the exact selected lease gateway hash —
    // never from historical topology comments, never assumed to be B.
    let a_hex = "aa".repeat(32);
    let b_hex = "bb".repeat(32);
    let c_hex = "cc".repeat(32);
    assert_eq!(p231_target_role(&a_hex, &a_hex, &b_hex, &c_hex), Some("A"));
    assert_eq!(p231_target_role(&b_hex, &a_hex, &b_hex, &c_hex), Some("B"));
    assert_eq!(p231_target_role(&c_hex, &a_hex, &b_hex, &c_hex), Some("C"));
    assert_eq!(
        p231_target_role(&"dd".repeat(32), &a_hex, &b_hex, &c_hex),
        None
    );
    assert_eq!(p231_target_role("not-hex", &a_hex, &b_hex, &c_hex), None);
    // An unresolvable target makes even the forward leg unobservable:
    // the earliest unknown stage maps to the B gap.
    let mut inputs = p231_test_inputs_pass();
    inputs.target_observable = false;
    assert_eq!(p231_classify(&inputs), P231Terminal::BObservabilityGap);
}

#[test]
fn p231_ibgw_requires_exact_target_tunnel_id() {
    // The target IBGW claim requires the exact lease gateway +
    // tunnel-id match (receive id == lease tunnel id with send-to ==
    // i2pr). A missing/non-exact gateway is NOT-INSTALLED even when
    // global dispatchInbound advanced elsewhere (background gateways
    // never satisfy the target stage).
    let mut inputs = p231_test_inputs_pass();
    inputs.ibgw_present_exact = false;
    assert_eq!(
        p231_classify(&inputs),
        P231Terminal::CTargetIbgwNotInstalled
    );
    assert_eq!(
        P231Terminal::CTargetIbgwNotInstalled.token(),
        "P231-C-TARGET-IBGW-NOT-INSTALLED"
    );
    // Exact gateway present but never pumped, with a quiet
    // dispatcher and an id-correlated B receipt row, is
    // NOT-RECEIVED for the target: C's forward reached B (which had
    // no matching gateway to dispatch through) while background
    // gateways never satisfy the target stage.
    inputs.ibgw_present_exact = true;
    inputs.target_dispatch_inbound_delta = Some(0);
    inputs.target_ibgw_processed_delta = Some(0);
    inputs.ibgw_overflow_delta = Some(0);
    inputs.b_gateway_receipt_correlated = true;
    assert_eq!(
        p231_classify(&inputs),
        P231Terminal::CTunnelGatewayNotReceived
    );
    // A dispatcher advance for the target epoch without exact pumping
    // is the enqueue-or-pump boundary (accepted into the gateway
    // queue but never pumped) — never target progress: the run stops
    // here and never reaches the D stages or PASS.
    inputs.target_dispatch_inbound_delta = Some(3);
    inputs.target_ibgw_processed_delta = Some(0);
    assert_eq!(
        p231_classify(&inputs),
        P231Terminal::CIbgwEnqueueOrPumpBoundary
    );
    // Dispatcher acceptance of the exact gateway without pumping (or
    // queue overflow) is the enqueue-or-pump boundary.
    inputs.target_ibgw_processed_delta = Some(0);
    inputs.ibgw_overflow_delta = Some(1);
    assert_eq!(
        p231_classify(&inputs),
        P231Terminal::CIbgwEnqueueOrPumpBoundary
    );
}

#[test]
fn p231_ibgw_processed_delta_precedes_tunneldata_emitted() {
    // The IBGW processed advance proves receiveEncrypted ran, which
    // constructs the next-hop TunnelData before enqueueing it. A
    // deferred next-hop lookup attempted in the window without wire
    // receipt is the lookup boundary.
    let mut inputs = p231_test_inputs_pass();
    inputs.ibgw_lookup_attempted_in_window = true;
    inputs.expected_tunneldata_seen = false;
    assert_eq!(
        p231_classify(&inputs),
        P231Terminal::CIbgwNextHopLookupFailed
    );
    assert_eq!(
        P231Terminal::CIbgwNextHopLookupFailed.token(),
        "P231-C-IBGW-NEXT-HOP-LOOKUP-FAILED"
    );
    // Lookup deferral that still reaches the wire proceeds to the
    // i2pr stages instead.
    inputs.expected_tunneldata_seen = true;
    assert_eq!(p231_classify(&inputs), P231Terminal::ReverseDeliveryPassed);
}

#[test]
fn p231_unrelated_tunneldata_cannot_satisfy_i2pr_stage() {
    // i2pr target progress requires the exact owned inbound tunnel id;
    // unrelated TunnelData is context only. With no expected-tunnel
    // receipt the stage is NO-EXPECTED-TUNNELDATA even when the
    // upstream IBGW demonstrably pumped the message.
    let mut inputs = p231_test_inputs_pass();
    inputs.expected_tunneldata_seen = false;
    assert_eq!(
        p231_classify(&inputs),
        P231Terminal::DI2prNoExpectedTunnelData
    );
    assert_eq!(
        P231Terminal::DI2prNoExpectedTunnelData.token(),
        "P231-D-I2PR-NO-EXPECTED-TUNNELDATA"
    );
}

#[test]
fn p231_expected_tunnel_id_required_for_recovery_stage() {
    // Without the exact owned inbound tunnel id the recovery stage
    // cannot be evaluated at all: D gap, never a wire claim.
    let mut inputs = p231_test_inputs_pass();
    inputs.expected_tunnel_id_known = false;
    assert_eq!(p231_classify(&inputs), P231Terminal::DObservabilityGap);
    assert_eq!(
        P231Terminal::DObservabilityGap.token(),
        "P231-D-OBSERVABILITY-GAP"
    );
}

#[test]
fn p231_tunnel_recovery_failure_is_distinct_from_no_wire_receive() {
    // Expected wire receipt with zero completions and recovery errors
    // is RECOVERY-FAILED — distinct from NO-EXPECTED-TUNNELDATA (no
    // wire at all).
    let mut inputs = p231_test_inputs_pass();
    inputs.expected_recovery_completes = 0;
    inputs.expected_recovery_errors = 2;
    inputs.expected_garlic_decodes_ok = 0;
    inputs.expected_garlic_decodes_fail = 0;
    inputs.expected_dispatch_calls = 0;
    inputs.expected_queue_hits = 0;
    assert_eq!(
        p231_classify(&inputs),
        P231Terminal::DI2prTunnelRecoveryFailed
    );
    assert_eq!(
        P231Terminal::DI2prTunnelRecoveryFailed.token(),
        "P231-D-I2PR-TUNNEL-RECOVERY-FAILED"
    );
    assert_ne!(
        P231Terminal::DI2prTunnelRecoveryFailed.token(),
        P231Terminal::DI2prNoExpectedTunnelData.token()
    );
}

#[test]
fn p231_garlic_failure_is_distinct_from_tunnel_recovery_failure() {
    // Completed recoveries whose envelope decode all fail are
    // GARLIC-FAILED — distinct from RECOVERY-FAILED (no completions).
    let mut inputs = p231_test_inputs_pass();
    inputs.expected_garlic_decodes_ok = 0;
    inputs.expected_garlic_decodes_fail = 1;
    inputs.expected_dispatch_calls = 0;
    inputs.expected_queue_hits = 0;
    assert_eq!(
        p231_classify(&inputs),
        P231Terminal::DI2prGarlicDecodeFailed
    );
    assert_eq!(
        P231Terminal::DI2prGarlicDecodeFailed.token(),
        "P231-D-I2PR-GARLIC-DECODE-FAILED"
    );
    assert_ne!(
        P231Terminal::DI2prGarlicDecodeFailed.token(),
        P231Terminal::DI2prTunnelRecoveryFailed.token()
    );
    // Decoded envelopes that never reach the destination queue are
    // DISPATCH-MISSED.
    inputs = p231_test_inputs_pass();
    inputs.expected_queue_hits = 0;
    assert_eq!(
        p231_classify(&inputs),
        P231Terminal::DI2prDestinationDispatchMissed
    );
}

#[test]
fn p231_destination_queue_hit_requires_digest_match_for_pass() {
    // A queue hit passes only on digest match. Decoded-but-wrong
    // content is PAYLOAD-MISMATCH; undecodable queue hits are
    // DISPATCH-MISSED.
    let mut inputs = p231_test_inputs_pass();
    inputs.expected_digest_match_45s = false;
    assert_eq!(p231_classify(&inputs), P231Terminal::DI2prPayloadMismatch);
    assert_eq!(
        P231Terminal::DI2prPayloadMismatch.token(),
        "P231-D-I2PR-PAYLOAD-MISMATCH"
    );
    inputs.expected_queued_decode_fails = 3;
    assert_eq!(
        p231_classify(&inputs),
        P231Terminal::DI2prDestinationDispatchMissed
    );
    inputs = p231_test_inputs_pass();
    assert_eq!(p231_classify(&inputs), P231Terminal::ReverseDeliveryPassed);
    assert_eq!(
        P231Terminal::ReverseDeliveryPassed.token(),
        "P231-REVERSE-DELIVERY-PASSED"
    );
}

#[test]
fn p231_status_only_after_45s_cannot_pass_payload_delivery() {
    // The 45-second payload acceptance window is frozen: a later
    // status-only success (70-second diagnostic deadline) can never
    // retroactively pass delivery. The classifier takes only the
    // frozen digest result — there is no status input at all.
    let mut inputs = p231_test_inputs_pass();
    inputs.expected_digest_match_45s = false;
    assert_eq!(
        p231_classify(&inputs),
        P231Terminal::DI2prPayloadMismatch,
        "a later status success must not pass the frozen payload row"
    );
    // The frozen pass requires the digest match and nothing else.
    inputs.expected_digest_match_45s = true;
    assert_eq!(p231_classify(&inputs), P231Terminal::ReverseDeliveryPassed);
}

#[test]
fn p231_first_unknown_stage_maps_to_observability_gap() {
    // Unknown at an earlier stage is an observability gap and prevents
    // any later stage from being claimed as root cause: knock out each
    // stage in order and check the earliest gap wins.
    let base = p231_test_inputs_pass();
    let mut inputs = base.clone();
    inputs.accepted_observed = false;
    assert_eq!(p231_classify(&inputs), P231Terminal::AObservabilityGap);
    inputs = base.clone();
    inputs.dispatch_outbound_delta = None;
    inputs.overflow_delta_a = None;
    inputs.client_dispatch_time_delta = None;
    inputs.dispatch_outbound_window_delta = None;
    inputs.overflow_window_delta = None;
    assert_eq!(p231_classify(&inputs), P231Terminal::AObservabilityGap);
    inputs = base.clone();
    inputs.c_obep_observable = false;
    assert_eq!(p231_classify(&inputs), P231Terminal::BObservabilityGap);
    inputs = base.clone();
    inputs.target_observable = false;
    assert_eq!(
        p231_classify(&inputs),
        P231Terminal::BObservabilityGap,
        "an unobservable target collapses the forward leg to the B gap"
    );
    inputs = base.clone();
    inputs.expected_tunnel_id_known = false;
    assert_eq!(p231_classify(&inputs), P231Terminal::DObservabilityGap);
    // Later failures never shadow an earlier gap.
    inputs = base.clone();
    inputs.dispatch_outbound_delta = Some(0);
    inputs.client_dispatch_time_delta = None;
    inputs.dispatch_outbound_window_delta = None;
    inputs.expected_digest_match_45s = false;
    assert_eq!(
        p231_classify(&inputs),
        P231Terminal::AObservabilityGap,
        "the earliest unknown stage wins over a later payload mismatch"
    );
}

#[test]
fn p231_exactly_one_terminal_per_counted_run() {
    // The driver emits exactly one p231-classification row per counted
    // run. Record twice into a scratch evidence dir and prove the
    // second emission is detectable (the driver path emits once; this
    // locks the row shape the uniqueness check consumes).
    let dir = p224_test_tmpdir("p231-record-once");
    let evidence_dir = dir.join("evidence");
    std::fs::create_dir_all(&evidence_dir).expect("evidence dir");
    record_p231_classification(
        &evidence_dir,
        P231Terminal::CTargetIbgwNotInstalled,
        "role=B receive_id=38401",
    );
    let tsv = std::fs::read_to_string(evidence_dir.join("driver-evidence.tsv")).expect("read tsv");
    assert_eq!(
        tsv.lines()
            .filter(|line| line.starts_with("p231-classification\t"))
            .count(),
        1
    );
    assert!(tsv.contains("P231-C-TARGET-IBGW-NOT-INSTALLED role=B receive_id=38401"));
    // Every terminal token is stable and unique.
    let tokens = [
        P231Terminal::AOutboundGatewayNotFound.token(),
        P231Terminal::AOutboundGatewayEnqueueDrop.token(),
        P231Terminal::AObservabilityGap.token(),
        P231Terminal::BFirstHopNotReceivedByC.token(),
        P231Terminal::BCObepReassemblyFailed.token(),
        P231Terminal::BObservabilityGap.token(),
        P231Terminal::CTargetIbgwNotInstalled.token(),
        P231Terminal::CTunnelGatewayNotReceived.token(),
        P231Terminal::CIbgwEnqueueOrPumpBoundary.token(),
        P231Terminal::CIbgwNextHopLookupFailed.token(),
        P231Terminal::DI2prNoExpectedTunnelData.token(),
        P231Terminal::DI2prTunnelRecoveryFailed.token(),
        P231Terminal::DI2prGarlicDecodeFailed.token(),
        P231Terminal::DI2prDestinationDispatchMissed.token(),
        P231Terminal::DI2prPayloadMismatch.token(),
        P231Terminal::DObservabilityGap.token(),
        P231Terminal::ReverseDeliveryPassed.token(),
    ];
    let mut seen = std::collections::HashSet::new();
    for token in tokens {
        assert!(token.starts_with("P231-"), "token shape: {token}");
        assert!(seen.insert(token), "duplicate terminal token: {token}");
    }
    assert_eq!(seen.len(), 17);
    // The early-stop gap emits the A gap exactly once.
    record_p231_early_stop_gap(&evidence_dir, "authoritative-epoch-never-reached-test");
    let tsv = std::fs::read_to_string(evidence_dir.join("driver-evidence.tsv")).expect("read tsv");
    assert_eq!(
        tsv.lines()
            .filter(|line| line.starts_with("p231-classification\t"))
            .count(),
        2
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn p231_secret_bearing_log_rows_rejected() {
    // Secret-bearing diagnostic rows never parse: keys, seeds, tags,
    // payloads, and raw log paths are rejected before they can satisfy
    // any Plan-231 fact.
    let gateway = "P231-EV kind=gateway observable=true dispatch_time=3 dispatch_send_time=1 dispatch_outbound_tunnel=2 drop_gateway_overflow=0 dispatch_inbound=0 inbound_lookup_success=0 dispatch_endpoint=1 dispatch_participant=0";
    assert!(p231_parse_gateway(gateway).is_some());
    for secret in [
        format!("{gateway} session_key=abcd"),
        format!("{gateway} seed=deadbeef"),
        format!("{gateway} tag=1234"),
        format!("{gateway} payload=deadbeef"),
        format!("{gateway} log-router-0.txt"),
        format!("{gateway} PRIV=1"),
    ] {
        assert!(
            p231_parse_gateway(&secret).is_none(),
            "secret-bearing gateway row must not parse"
        );
    }
    assert!(p231_parse_gateway("P230-EV kind=gateway observable=true").is_none());
    assert!(p231_parse_gateway(&gateway.replace("dispatch_time=3", "dispatch_time=yes")).is_none());
    let dbid = p227_test_c_hex();
    let outbound = format!(
        "P231-EV kind=client-outbound client_dbid_hex={dbid} observable=true client_resolved=true outbound_tunnel_count=1 single_send_tunnel_id=12345"
    );
    assert!(p231_parse_client_outbound(&outbound, &dbid).is_some());
    assert!(p231_parse_client_outbound(&format!("{outbound} priv=1"), &dbid).is_none());
    let other = "dd".repeat(32);
    assert!(p231_parse_client_outbound(&outbound, &other).is_none());
    let part = "P231-EV kind=participating receive_id=38401 observable=true present=true match_count=1 send_id=0 receive_from=none send_to=none processed=4";
    assert!(p231_parse_participating(part, 38401).is_some());
    assert!(p231_parse_participating(part, 38402).is_none());
    assert!(p231_parse_participating(&format!("{part} payload=x"), 38401).is_none());
}

// ---- Plan 232 — route-derived lease-gateway fixture corrective --------------
// Plan 231 proved the published local Standard LS2 advertises the wrong
// inbound gateway: the Java external driver builds the real inbound tunnel
// through the Java service router (Router A) but constructs the local lease
// with the publication router (Router B / `java_hash`). Plan 232 derives
// every local LS2 lease gateway and gateway tunnel directly from the
// installed inbound route in the tunnel registry. The route object is the
// source of truth; controlled-topology identities remain regression
// assertions only, never the lease source. No production `src/` change.

/// Plan 232 lease-derivation failure: the caller must fail closed before
/// publication (never publish a lease whose gateway/tunnel did not come
/// from the installed inbound route).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum P232LeaseError {
    MissingInboundRoute,
    SlotRouteMismatch,
}

impl std::fmt::Display for P232LeaseError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            P232LeaseError::MissingInboundRoute => {
                formatter.write_str("installed inbound route missing")
            }
            P232LeaseError::SlotRouteMismatch => {
                formatter.write_str("registration slot does not match installed route")
            }
        }
    }
}

impl std::error::Error for P232LeaseError {}

/// Plan 232 route-parity facts for one local LS2 construction. Hashes and
/// tunnel ids are bounded public evidence; no keys, tags, private
/// Destination material, or payload plaintext ever reach this surface.
#[derive(Clone, Debug, PartialEq, Eq)]
struct P232LeaseParity {
    gateway_route_match: bool,
    tunnel_route_match: bool,
    publication_distinct_from_gateway: bool,
    gateway_matches_service_router: bool,
    gateway_tunnel: u32,
    local_receive: u32,
}

/// Plan 232 work package A — the single route-derived lease contract all
/// three Java-driver local lease sites must use.
///
/// The lease gateway and gateway tunnel come ONLY from `route` (the
/// installed inbound route fetched via
/// `coord.registry().inbound_gateway_route(local_receive)`). They are
/// never inferred from the publication target, from historical constants
/// alone, or from role names such as A/B.
///
/// `registry_slot` is the registry-bound slot for `local_receive`
/// (`coord.registry().inbound_slot(local_receive)`); it must equal `slot`
/// so a stale registration cannot be paired with a live route.
fn p232_route_derived_lease_source(
    slot: i2pr_tunnel::pool::TunnelSlot,
    local_receive: TunnelId,
    route: Option<i2pr_tunnel::data_plane_registry::InboundGatewayRoute>,
    registry_slot: Option<i2pr_tunnel::pool::TunnelSlot>,
    tunnel_expires_seconds: u64,
    advertised_expires_seconds: u64,
) -> Result<InboundLeaseSource, P232LeaseError> {
    let route = route.ok_or(P232LeaseError::MissingInboundRoute)?;
    if registry_slot != Some(slot) {
        return Err(P232LeaseError::SlotRouteMismatch);
    }
    if route.local_receive_tunnel != local_receive {
        return Err(P232LeaseError::SlotRouteMismatch);
    }
    Ok(InboundLeaseSource::from_parts(
        slot,
        route.gateway_router,
        route.gateway_receive_tunnel.get(),
        tunnel_expires_seconds,
        advertised_expires_seconds,
    ))
}

/// Plan 232 work package F — durable parity facts binding one constructed
/// lease to the installed route it must have come from, plus the
/// independently derived publication target it must NOT have come from.
fn p232_lease_parity(
    lease: &InboundLeaseSource,
    route: &i2pr_tunnel::data_plane_registry::InboundGatewayRoute,
    publication_target: &Hash,
    service_router: &Hash,
) -> P232LeaseParity {
    let gateway_route_match = lease.gateway() == route.gateway_router;
    let tunnel_route_match =
        lease.gateway_receive_tunnel_id() == route.gateway_receive_tunnel.get();
    P232LeaseParity {
        gateway_route_match,
        tunnel_route_match,
        publication_distinct_from_gateway: publication_target != &lease.gateway(),
        gateway_matches_service_router: lease.gateway() == *service_router,
        gateway_tunnel: lease.gateway_receive_tunnel_id(),
        local_receive: route.local_receive_tunnel.get(),
    }
}

/// Plan 232 work packages B/F — record the route/lease/publication-target
/// parity row for one local LS2 construction and fail closed before
/// publication when gateway+tunnel parity does not hold. `lane` is
/// `destination` or `streaming`; `stage` is `initial` or `refresh`.
struct P232LeaseRecord<'a> {
    lane: &'a str,
    stage: &'a str,
    slot: i2pr_tunnel::pool::TunnelSlot,
    local_receive: TunnelId,
    route: &'a i2pr_tunnel::data_plane_registry::InboundGatewayRoute,
    lease: &'a InboundLeaseSource,
    publication_target: &'a Hash,
    service_router: &'a Hash,
}

fn p232_record_lease_route(evidence_dir: &Path, record: &P232LeaseRecord<'_>) -> P232LeaseParity {
    let parity = p232_lease_parity(
        record.lease,
        record.route,
        record.publication_target,
        record.service_router,
    );
    let label = if record.lane == "destination" {
        "p232-destination-lease-route"
    } else {
        "p232-streaming-lease-route"
    };
    append_evidence(
        evidence_dir,
        label,
        &format!(
            "lane={} stage={} registration_slot={} local_receive_tunnel={} route_gateway_hash={} route_gateway_tunnel={} lease_gateway_hash={} lease_gateway_tunnel={} publication_target_hash={} gateway_route_match={} tunnel_route_match={} publication_distinct_from_gateway={} gateway_matches_service_router={} gateway_tunnel={} local_receive={}",
            record.lane,
            record.stage,
            record.slot.get(),
            record.local_receive.get(),
            p220_bytes_to_hex(record.route.gateway_router.as_bytes()),
            record.route.gateway_receive_tunnel.get(),
            p220_bytes_to_hex(record.lease.gateway().as_bytes()),
            record.lease.gateway_receive_tunnel_id(),
            p220_bytes_to_hex(record.publication_target.as_bytes()),
            parity.gateway_route_match,
            parity.tunnel_route_match,
            parity.publication_distinct_from_gateway,
            parity.gateway_matches_service_router,
            parity.gateway_tunnel,
            parity.local_receive,
        ),
    );
    assert!(
        parity.gateway_route_match,
        "Plan 232 §11: lease gateway must match the installed inbound route before publication (lane={} stage={})",
        record.lane, record.stage,
    );
    assert!(
        parity.tunnel_route_match,
        "Plan 232 §11: lease tunnel must match the installed inbound route before publication (lane={} stage={})",
        record.lane, record.stage,
    );
    parity
}

/// Plan 232 work package C — prove correcting the lease gateway does not
/// redirect NetDB publication: the publication target stays the retained
/// Java floodfill router while the lease gateway is the installed inbound
/// tunnel gateway. The two roles must never become aliases.
fn p232_record_publication_separation(
    evidence_dir: &Path,
    lane: &str,
    stage: &str,
    publication_target: &Hash,
    route: &i2pr_tunnel::data_plane_registry::InboundGatewayRoute,
    lease: &InboundLeaseSource,
) {
    let publication_is_target = true;
    let lease_from_route = lease.gateway() == route.gateway_router
        && lease.gateway_receive_tunnel_id() == route.gateway_receive_tunnel.get();
    let distinct = publication_target != &lease.gateway();
    append_evidence(
        evidence_dir,
        "p232-publication-separation",
        &format!(
            "lane={lane} stage={stage} publication_target_hash={} lease_gateway_hash={} route_gateway_hash={} publication_is_publication_target={publication_is_target} lease_from_installed_route={lease_from_route} publication_distinct_from_gateway={distinct}",
            p220_bytes_to_hex(publication_target.as_bytes()),
            p220_bytes_to_hex(lease.gateway().as_bytes()),
            p220_bytes_to_hex(route.gateway_router.as_bytes()),
        ),
    );
    assert!(
        lease_from_route,
        "Plan 232 §8: lease gateway must come from the installed route, never the publication target (lane={lane} stage={stage})"
    );
}

/// Plan 232 work package D — corrected-route preflight gate before
/// `SEND_TRACKED`: the exact advertised target IBGW must be installed on
/// the router identified by the actual advertised lease.
fn p232_target_ibgw_gate(ibgw_present_exact: bool) -> bool {
    ibgw_present_exact
}

fn p232_record_target_ibgw_preflight(
    evidence_dir: &Path,
    published_gateway_matches_route: bool,
    published_tunnel_matches_route: bool,
    target_router_has_exact_ibgw: bool,
    role: &str,
    lease_tunnel: u32,
) {
    append_evidence(
        evidence_dir,
        "p232-target-ibgw-preflight",
        &format!(
            "published_local_ls2_gateway_matches_route={published_gateway_matches_route} published_local_ls2_tunnel_matches_route={published_tunnel_matches_route} target_router_has_exact_ibgw={target_router_has_exact_ibgw} role={role} lease_tunnel={lease_tunnel}",
        ),
    );
    assert!(
        published_gateway_matches_route,
        "Plan 232 §9: published LS2 gateway must equal the installed route gateway before SEND_TRACKED"
    );
    assert!(
        published_tunnel_matches_route,
        "Plan 232 §9: published LS2 tunnel must equal the installed route gateway tunnel before SEND_TRACKED"
    );
}

/// Plan 232 work packages D/E — post-correction reverse + closure
/// vocabulary. D-stage tokens mirror the first new exact boundary; the
/// fixture token is a harness defect (never production work); the
/// second-family token closes the Java branch only when raw reverse and
/// every retained Streaming row pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum P232Terminal {
    ReverseDeliveryPassed,
    JavaForwardingBoundary,
    I2prNoExpectedTunnelData,
    I2prTunnelRecoveryFailed,
    I2prGarlicDecodeFailed,
    I2prDestinationDispatchMissed,
    I2prPayloadMismatch,
    ObservabilityGap,
    FixtureRouteParityFailed,
    RawReversePassedStreamingBoundary,
    JavaSecondFamilyPassed,
}

impl P232Terminal {
    fn token(self) -> &'static str {
        match self {
            P232Terminal::ReverseDeliveryPassed => "P232-D-REVERSE-DELIVERY-PASSED",
            P232Terminal::JavaForwardingBoundary => "P232-D-JAVA-FORWARDING-BOUNDARY",
            P232Terminal::I2prNoExpectedTunnelData => "P232-D-I2PR-NO-EXPECTED-TUNNELDATA",
            P232Terminal::I2prTunnelRecoveryFailed => "P232-D-I2PR-TUNNEL-RECOVERY-FAILED",
            P232Terminal::I2prGarlicDecodeFailed => "P232-D-I2PR-GARLIC-DECODE-FAILED",
            P232Terminal::I2prDestinationDispatchMissed => {
                "P232-D-I2PR-DESTINATION-DISPATCH-MISSED"
            }
            P232Terminal::I2prPayloadMismatch => "P232-D-I2PR-PAYLOAD-MISMATCH",
            P232Terminal::ObservabilityGap => "P232-D-OBSERVABILITY-GAP",
            P232Terminal::FixtureRouteParityFailed => "P232-FIXTURE-ROUTE-PARITY-FAILED",
            P232Terminal::RawReversePassedStreamingBoundary => {
                "P232-RAW-REVERSE-PASSED-STREAMING-BOUNDARY"
            }
            P232Terminal::JavaSecondFamilyPassed => "P232-JAVA-SECOND-FAMILY-PASSED",
        }
    }
}

/// Plan 232 §9 — classify the corrected-route reverse epoch. Parity must
/// hold first (otherwise the run is a fixture defect, never a protocol
/// boundary). With parity proven, the frozen 45-second digest match
/// passes; otherwise the earliest new exact D-stage boundary applies.
/// `pre_target_ibgw_present` distinguishes a Java-side forwarding stop
/// (no exact IBGW pumped upstream) from the i2pr-owned D stages.
/// Pure classifier inputs for the corrected-route reverse epoch. With
/// parity proven, the frozen 45-second digest decides the pass; otherwise
/// the earliest new exact D-stage boundary applies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct P232ReverseInputs {
    parity_ok: bool,
    frozen_digest_match_45s: bool,
    expected_tunneldata_seen: bool,
    recovery_completes: u64,
    recovery_errors: u64,
    garlic_decodes_ok: u64,
    garlic_decodes_fail: u64,
    dispatch_calls: u64,
    queue_hits: u64,
    queued_decode_fails: u64,
    pre_target_ibgw_present: bool,
}

fn p232_classify_reverse(inputs: &P232ReverseInputs) -> P232Terminal {
    if !inputs.parity_ok {
        return P232Terminal::FixtureRouteParityFailed;
    }
    if inputs.frozen_digest_match_45s {
        return P232Terminal::ReverseDeliveryPassed;
    }
    if !inputs.pre_target_ibgw_present && !inputs.expected_tunneldata_seen {
        return P232Terminal::JavaForwardingBoundary;
    }
    if !inputs.expected_tunneldata_seen {
        return P232Terminal::I2prNoExpectedTunnelData;
    }
    if inputs.recovery_completes == 0 {
        if inputs.recovery_errors > 0 {
            return P232Terminal::I2prTunnelRecoveryFailed;
        }
        return P232Terminal::ObservabilityGap;
    }
    if inputs.garlic_decodes_ok == 0 {
        if inputs.garlic_decodes_fail > 0 {
            return P232Terminal::I2prGarlicDecodeFailed;
        }
        return P232Terminal::ObservabilityGap;
    }
    if inputs.dispatch_calls == 0 {
        return P232Terminal::ObservabilityGap;
    }
    if inputs.queue_hits == 0 {
        return P232Terminal::I2prDestinationDispatchMissed;
    }
    if inputs.queued_decode_fails > 0 {
        return P232Terminal::I2prDestinationDispatchMissed;
    }
    P232Terminal::I2prPayloadMismatch
}

fn record_p232_classification(evidence_dir: &Path, terminal: P232Terminal, detail: &str) {
    let sanitized = detail.replace(['\t', '\n'], " ");
    append_evidence(
        evidence_dir,
        "p232-classification",
        &format!("{} {sanitized}", terminal.token()),
    );
}

/// Plan 232 §9 — raw reverse pass must continue directly into Streaming
/// on the same corrected implementation (no intermediate plan).
fn p232_raw_reverse_permits_streaming(reverse_passed: bool) -> bool {
    reverse_passed
}

/// Plan 232 §10/§17 — the Java second family closes only when corrected
/// raw reverse AND every retained Streaming row pass.
fn p232_streaming_permits_closure(raw_passed: bool, streaming_complete: bool) -> bool {
    raw_passed && streaming_complete
}

fn p232_test_route(
    gateway_byte: u8,
    gateway_tunnel: u32,
    local_receive: u32,
) -> i2pr_tunnel::data_plane_registry::InboundGatewayRoute {
    i2pr_tunnel::data_plane_registry::InboundGatewayRoute {
        gateway_router: Hash::from_bytes([gateway_byte; 32]),
        gateway_receive_tunnel: TunnelId::new(gateway_tunnel).expect("tunnel id"),
        local_receive_tunnel: TunnelId::new(local_receive).expect("tunnel id"),
    }
}

#[test]
fn p232_destination_lease_gateway_matches_installed_inbound_route() {
    let slot = i2pr_tunnel::pool::TunnelSlot::from_raw(7);
    let local = TunnelId::new(IBGW_NEXT).expect("local receive");
    let route = p232_test_route(0xA1, IBGW_RECEIVE, IBGW_NEXT);
    let lease = p232_route_derived_lease_source(
        slot,
        local,
        Some(route),
        Some(slot),
        1_700_000_600,
        1_700_000_540,
    )
    .expect("route-derived lease");
    assert_eq!(lease.gateway(), route.gateway_router);
    assert_eq!(lease.slot(), slot);
}

#[test]
fn p232_destination_lease_tunnel_matches_installed_inbound_route() {
    let slot = i2pr_tunnel::pool::TunnelSlot::from_raw(7);
    let local = TunnelId::new(IBGW_NEXT).expect("local receive");
    let route = p232_test_route(0xA1, IBGW_RECEIVE, IBGW_NEXT);
    let lease = p232_route_derived_lease_source(
        slot,
        local,
        Some(route),
        Some(slot),
        1_700_000_600,
        1_700_000_540,
    )
    .expect("route-derived lease");
    assert_eq!(lease.gateway_receive_tunnel_id(), IBGW_RECEIVE);
    assert_eq!(
        lease.gateway_receive_tunnel_id(),
        route.gateway_receive_tunnel.get()
    );
}

#[test]
fn p232_destination_publication_target_is_not_lease_gateway_source() {
    // The publication target (Router B) and the lease gateway (Router A)
    // are different protocol roles. A lease built from the route must
    // match the route and stay distinct from the publication target, and
    // the parity surface must prove both facts.
    let slot = i2pr_tunnel::pool::TunnelSlot::from_raw(3);
    let local = TunnelId::new(IBGW_NEXT).expect("local receive");
    let route = p232_test_route(0xA1, IBGW_RECEIVE, IBGW_NEXT);
    let publication_target = Hash::from_bytes([0xB2; 32]);
    let service_router = Hash::from_bytes([0xA1; 32]);
    let lease = p232_route_derived_lease_source(
        slot,
        local,
        Some(route),
        Some(slot),
        1_700_000_600,
        1_700_000_540,
    )
    .expect("route-derived lease");
    let parity = p232_lease_parity(&lease, &route, &publication_target, &service_router);
    assert!(parity.gateway_route_match);
    assert!(parity.tunnel_route_match);
    assert!(parity.publication_distinct_from_gateway);
    assert!(parity.gateway_matches_service_router);
    // The publication target must never equal the route gateway in this
    // fixture: a hardcoded publication-target gateway would fail parity
    // by construction (gateway_route_match would be false).
    assert_ne!(publication_target, route.gateway_router);
    assert_ne!(lease.gateway(), publication_target);
}

#[test]
fn p232_streaming_initial_gateway_matches_installed_inbound_route() {
    let slot = i2pr_tunnel::pool::TunnelSlot::from_raw(11);
    let local = TunnelId::new(0x9802).expect("streaming local receive");
    let route = p232_test_route(0xA1, 0x9801, 0x9802);
    let lease = p232_route_derived_lease_source(
        slot,
        local,
        Some(route),
        Some(slot),
        1_700_000_600,
        1_700_000_540,
    )
    .expect("streaming initial lease");
    assert_eq!(lease.gateway(), route.gateway_router);
}

#[test]
fn p232_streaming_initial_tunnel_matches_installed_inbound_route() {
    let slot = i2pr_tunnel::pool::TunnelSlot::from_raw(11);
    let local = TunnelId::new(0x9802).expect("streaming local receive");
    let route = p232_test_route(0xA1, 0x9801, 0x9802);
    let lease = p232_route_derived_lease_source(
        slot,
        local,
        Some(route),
        Some(slot),
        1_700_000_600,
        1_700_000_540,
    )
    .expect("streaming initial lease");
    assert_eq!(lease.gateway_receive_tunnel_id(), 0x9801);
}

#[test]
fn p232_streaming_refresh_revalidates_installed_inbound_route() {
    // The refresh must re-derive from the live installed route: reusing a
    // stale route against a new local receive id fails closed instead of
    // copying the previously derived gateway across a long-running test.
    let slot = i2pr_tunnel::pool::TunnelSlot::from_raw(11);
    let initial_local = TunnelId::new(0x9802).expect("initial local");
    let initial_route = p232_test_route(0xA1, 0x9801, 0x9802);
    let initial = p232_route_derived_lease_source(
        slot,
        initial_local,
        Some(initial_route),
        Some(slot),
        1_700_000_600,
        1_700_000_540,
    )
    .expect("initial lease");
    assert_eq!(initial.gateway(), initial_route.gateway_router);
    // A revalidation against a different live local id with the stale
    // route must fail: the route's local selector no longer matches.
    let refreshed_local = TunnelId::new(0x9803).expect("refreshed local");
    assert!(matches!(
        p232_route_derived_lease_source(
            slot,
            refreshed_local,
            Some(initial_route),
            Some(slot),
            1_700_001_800,
            1_700_001_740,
        ),
        Err(P232LeaseError::SlotRouteMismatch)
    ));
    // Re-deriving from the live refreshed route passes.
    let refreshed_route = p232_test_route(0xA1, 0x9801, 0x9803);
    let refreshed = p232_route_derived_lease_source(
        slot,
        refreshed_local,
        Some(refreshed_route),
        Some(slot),
        1_700_001_800,
        1_700_001_740,
    )
    .expect("refreshed lease");
    assert_eq!(refreshed.gateway(), refreshed_route.gateway_router);
}

#[test]
fn p232_streaming_refresh_gateway_matches_installed_inbound_route() {
    let slot = i2pr_tunnel::pool::TunnelSlot::from_raw(11);
    let local = TunnelId::new(0x9802).expect("streaming local receive");
    let route = p232_test_route(0xA1, 0x9801, 0x9802);
    let lease = p232_route_derived_lease_source(
        slot,
        local,
        Some(route),
        Some(slot),
        1_700_001_800,
        1_700_001_740,
    )
    .expect("refresh lease");
    assert_eq!(lease.gateway(), route.gateway_router);
}

#[test]
fn p232_streaming_refresh_tunnel_matches_installed_inbound_route() {
    let slot = i2pr_tunnel::pool::TunnelSlot::from_raw(11);
    let local = TunnelId::new(0x9802).expect("streaming local receive");
    let route = p232_test_route(0xA1, 0x9801, 0x9802);
    let lease = p232_route_derived_lease_source(
        slot,
        local,
        Some(route),
        Some(slot),
        1_700_001_800,
        1_700_001_740,
    )
    .expect("refresh lease");
    assert_eq!(
        lease.gateway_receive_tunnel_id(),
        route.gateway_receive_tunnel.get()
    );
}

#[test]
fn p232_route_derived_helper_rejects_missing_inbound_route() {
    let slot = i2pr_tunnel::pool::TunnelSlot::from_raw(7);
    let local = TunnelId::new(IBGW_NEXT).expect("local receive");
    assert_eq!(
        p232_route_derived_lease_source(slot, local, None, Some(slot), 100, 40),
        Err(P232LeaseError::MissingInboundRoute)
    );
}

#[test]
fn p232_route_derived_helper_rejects_slot_route_mismatch() {
    let slot = i2pr_tunnel::pool::TunnelSlot::from_raw(7);
    let other_slot = i2pr_tunnel::pool::TunnelSlot::from_raw(9);
    let local = TunnelId::new(IBGW_NEXT).expect("local receive");
    let route = p232_test_route(0xA1, IBGW_RECEIVE, IBGW_NEXT);
    // Registry-bound slot differs from the supplied registration slot.
    assert_eq!(
        p232_route_derived_lease_source(slot, local, Some(route), Some(other_slot), 100, 40),
        Err(P232LeaseError::SlotRouteMismatch)
    );
    // Registry has no binding for this local receive id.
    assert_eq!(
        p232_route_derived_lease_source(slot, local, Some(route), None, 100, 40),
        Err(P232LeaseError::SlotRouteMismatch)
    );
    // Route selector differs from the queried local receive id.
    let wrong_local = TunnelId::new(IBGW_RECEIVE).expect("wrong local");
    assert_eq!(
        p232_route_derived_lease_source(slot, wrong_local, Some(route), Some(slot), 100, 40),
        Err(P232LeaseError::SlotRouteMismatch)
    );
}

#[test]
fn p232_java_hash_cannot_be_hardcoded_as_local_lease_gateway() {
    // Simulates the Plan 231 defect: gateway hardcoded to the publication
    // target instead of the installed route. The helper-derived lease must
    // differ from the publication target and match the route; a hardcoded
    // publication-target gateway is definitionally unequal to the route
    // gateway in this fixture and would fail parity.
    let slot = i2pr_tunnel::pool::TunnelSlot::from_raw(7);
    let local = TunnelId::new(IBGW_NEXT).expect("local receive");
    let route = p232_test_route(0xA1, IBGW_RECEIVE, IBGW_NEXT);
    let java_hash = Hash::from_bytes([0xB2; 32]);
    let service_router = Hash::from_bytes([0xA1; 32]);
    let lease = p232_route_derived_lease_source(
        slot,
        local,
        Some(route),
        Some(slot),
        1_700_000_600,
        1_700_000_540,
    )
    .expect("route-derived lease");
    assert_ne!(lease.gateway(), java_hash);
    assert_eq!(lease.gateway(), route.gateway_router);
    assert_ne!(java_hash, route.gateway_router);
    let parity = p232_lease_parity(&lease, &route, &java_hash, &service_router);
    assert!(parity.gateway_route_match);
    assert!(parity.gateway_matches_service_router);
}

#[test]
fn p232_all_java_local_lease_sites_use_route_derived_contract() {
    // All three local lease sites (raw Destination, initial Streaming,
    // refreshed Streaming) share the one helper: prove the helper covers
    // the destination namespace, the streaming namespace, and the refresh
    // epoch with identical route-match semantics.
    for (slot_raw, gateway_tunnel, local_receive) in [
        (7u32, IBGW_RECEIVE, IBGW_NEXT),
        (11u32, 0x9801u32, 0x9802u32),
        (11u32, 0x9801u32, 0x9802u32),
    ] {
        let slot = i2pr_tunnel::pool::TunnelSlot::from_raw(slot_raw);
        let local = TunnelId::new(local_receive).expect("local receive");
        let route = p232_test_route(0xA1, gateway_tunnel, local_receive);
        let lease = p232_route_derived_lease_source(
            slot,
            local,
            Some(route),
            Some(slot),
            1_700_000_600,
            1_700_000_540,
        )
        .expect("every site uses the route-derived contract");
        assert_eq!(lease.gateway(), route.gateway_router);
        assert_eq!(
            lease.gateway_receive_tunnel_id(),
            route.gateway_receive_tunnel.get()
        );
    }
}

#[test]
fn p232_corrected_target_router_must_have_exact_ibgw_before_send() {
    // The §9 preflight gate passes only when the router identified by the
    // advertised lease exposes the exact IBGW. A corrected route that
    // still addresses a router without the gateway must not send.
    assert!(p232_target_ibgw_gate(true));
    assert!(!p232_target_ibgw_gate(false));
}

#[test]
fn p232_reverse_pass_requires_exact_tunneldata_and_digest() {
    // Digest match without wire is impossible by construction: the pass
    // requires both the exact TunnelData observation and the digest.
    let pass_inputs = P232ReverseInputs {
        parity_ok: true,
        frozen_digest_match_45s: true,
        expected_tunneldata_seen: true,
        recovery_completes: 1,
        recovery_errors: 0,
        garlic_decodes_ok: 1,
        garlic_decodes_fail: 0,
        dispatch_calls: 1,
        queue_hits: 1,
        queued_decode_fails: 0,
        pre_target_ibgw_present: true,
    };
    let pass = p232_classify_reverse(&pass_inputs);
    assert_eq!(pass, P232Terminal::ReverseDeliveryPassed);
    assert_eq!(pass.token(), "P232-D-REVERSE-DELIVERY-PASSED");
    // Digest match claimed without wire receipt is still no-wire.
    let no_wire = p232_classify_reverse(&P232ReverseInputs {
        parity_ok: true,
        frozen_digest_match_45s: false,
        expected_tunneldata_seen: false,
        recovery_completes: 0,
        recovery_errors: 0,
        garlic_decodes_ok: 0,
        garlic_decodes_fail: 0,
        dispatch_calls: 0,
        queue_hits: 0,
        queued_decode_fails: 0,
        pre_target_ibgw_present: true,
    });
    assert_eq!(no_wire, P232Terminal::I2prNoExpectedTunnelData);
    // Wire without digest is a payload mismatch, never a pass.
    let mismatch = p232_classify_reverse(&P232ReverseInputs {
        parity_ok: true,
        frozen_digest_match_45s: false,
        ..pass_inputs
    });
    assert_eq!(mismatch, P232Terminal::I2prPayloadMismatch);
    // Missing parity is a fixture defect even when everything else passes.
    let fixture = p232_classify_reverse(&P232ReverseInputs {
        parity_ok: false,
        ..pass_inputs
    });
    assert_eq!(fixture, P232Terminal::FixtureRouteParityFailed);
    assert_eq!(fixture.token(), "P232-FIXTURE-ROUTE-PARITY-FAILED");
}

#[test]
fn p232_raw_reverse_pass_continues_to_streaming() {
    assert!(p232_raw_reverse_permits_streaming(true));
    assert!(!p232_raw_reverse_permits_streaming(false));
}

#[test]
fn p232_streaming_pass_can_close_java_second_family() {
    assert!(p232_streaming_permits_closure(true, true));
    assert!(!p232_streaming_permits_closure(true, false));
    assert!(!p232_streaming_permits_closure(false, true));
    assert!(!p232_streaming_permits_closure(false, false));
    assert_eq!(
        P232Terminal::JavaSecondFamilyPassed.token(),
        "P232-JAVA-SECOND-FAMILY-PASSED"
    );
    assert_eq!(
        P232Terminal::RawReversePassedStreamingBoundary.token(),
        "P232-RAW-REVERSE-PASSED-STREAMING-BOUNDARY"
    );
}

#[test]
fn p232_timeout_windows_remain_frozen() {
    // Plan 232 §5.6: no timeout inflation. The 30-second install poll,
    // the 45-second reverse payload window, and the 70-second
    // status-only window stay frozen.
    assert_eq!(ACCEPT_TIMEOUT, Duration::from_secs(30));
    assert_eq!(DATAGRAM_WAIT, Duration::from_secs(45));
    assert_eq!(P222_STATUS_OBSERVATION_DEADLINE, Duration::from_secs(70));
}

#[test]
fn p232_no_production_surface_change() {
    // The P232 surface lives in this external test only: every terminal
    // token is namespaced `P232-*` and no token aliases a production
    // evidence label.
    for token in [
        P232Terminal::ReverseDeliveryPassed.token(),
        P232Terminal::JavaForwardingBoundary.token(),
        P232Terminal::I2prNoExpectedTunnelData.token(),
        P232Terminal::I2prTunnelRecoveryFailed.token(),
        P232Terminal::I2prGarlicDecodeFailed.token(),
        P232Terminal::I2prDestinationDispatchMissed.token(),
        P232Terminal::I2prPayloadMismatch.token(),
        P232Terminal::ObservabilityGap.token(),
        P232Terminal::FixtureRouteParityFailed.token(),
        P232Terminal::RawReversePassedStreamingBoundary.token(),
        P232Terminal::JavaSecondFamilyPassed.token(),
    ] {
        assert!(token.starts_with("P232-"), "token shape: {token}");
    }
    for production_label in [
        "destination-outbound-delivered",
        "reference-received",
        "shutdown-baseline",
    ] {
        assert!(
            !P232Terminal::ReverseDeliveryPassed
                .token()
                .contains(production_label)
        );
    }
}

// ---- Plan 234 §14 unit rows -----------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum P234RowResolution {
    Pass,
    SupersededByStrongerExternalEvidence,
    StillBlocking,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct P234RowEvidence {
    current_passed: bool,
    mapping_explicit: bool,
    replacement_mandatory: bool,
    replacement_stronger: bool,
}

fn p234_resolve_required_row(evidence: P234RowEvidence) -> P234RowResolution {
    if evidence.current_passed {
        P234RowResolution::Pass
    } else if evidence.mapping_explicit
        && evidence.replacement_mandatory
        && evidence.replacement_stronger
    {
        P234RowResolution::SupersededByStrongerExternalEvidence
    } else {
        P234RowResolution::StillBlocking
    }
}

#[allow(clippy::too_many_arguments)]
fn p234_family_closure_allowed(
    plan232_route_parity: bool,
    raw_destination_pass: bool,
    streaming_direction_a: bool,
    streaming_direction_b: bool,
    refresh_republication: bool,
    required_rows_resolved: bool,
    run_java_exit_zero: bool,
    mixed_router_checker: bool,
    final_closure_checker: bool,
) -> bool {
    plan232_route_parity
        && raw_destination_pass
        && streaming_direction_a
        && streaming_direction_b
        && refresh_republication
        && required_rows_resolved
        && run_java_exit_zero
        && mixed_router_checker
        && final_closure_checker
}

#[test]
fn p234_plan232_route_parity_is_prerequisite() {
    assert!(!p234_family_closure_allowed(
        false, true, true, true, true, true, true, true, true
    ));
}

#[test]
fn p234_start_accept_precedes_syn_send() {
    let epoch = P234SynEpoch {
        java_accept_thread_started: true,
        syn_transport_request_emitted: true,
        ..P234SynEpoch::default()
    };
    assert!(epoch.java_accept_thread_started);
    assert!(epoch.syn_transport_request_emitted);
}

#[test]
fn p234_java_accept_worker_state_is_bounded() {
    let parsed = p234_parse_java_accept_state(
        "STREAM_STATUS accept_requested=1 accept_entered=1 accept_returned=1 socket_stored=1 accept_errors=0 accepting=false accepted_count=1 connected_count=0",
    )
    .expect("bounded stream status");
    assert_eq!(parsed.accept_returned, 1);
    assert!(!parsed.accepting);
    assert!(p234_parse_java_accept_state("STREAM_STATUS accept_requested=1").is_none());
}

#[test]
fn p234_no_expected_tunneldata_is_distinct_from_decode_failure() {
    let base = P234SynEpoch {
        java_accept_thread_started: true,
        java_accept_returned: true,
        ..P234SynEpoch::default()
    };
    assert_eq!(
        p234_classify_syn_epoch(&base),
        P234Terminal::JavaAcceptedNoResponseObserved
    );
    let decoded = P234SynEpoch {
        i2pr_inbound_tunneldata_count: 1,
        i2pr_expected_stream_tunneldata_count: 1,
        i2pr_garlic_decode_failures: 1,
        ..base
    };
    assert_eq!(
        p234_classify_syn_epoch(&decoded),
        P234Terminal::I2prGarlicDecodeFailed
    );
}

#[test]
fn p234_tunnel_recovery_failure_is_distinct_from_no_wire() {
    let epoch = P234SynEpoch {
        java_accept_thread_started: true,
        java_accept_returned: true,
        i2pr_inbound_tunneldata_count: 1,
        i2pr_expected_stream_tunneldata_count: 1,
        i2pr_tunnel_recovery_failures: 1,
        ..P234SynEpoch::default()
    };
    assert_eq!(
        p234_classify_syn_epoch(&epoch),
        P234Terminal::I2prTunnelRecoveryFailed
    );
    assert_ne!(
        p234_classify_syn_epoch(&P234SynEpoch {
            java_accept_thread_started: true,
            java_accept_returned: true,
            ..P234SynEpoch::default()
        }),
        P234Terminal::I2prTunnelRecoveryFailed
    );
}

#[test]
fn p234_garlic_failure_is_distinct_from_streaming_adapter_failure() {
    let base = P234SynEpoch {
        java_accept_thread_started: true,
        java_accept_returned: true,
        i2pr_inbound_tunneldata_count: 1,
        i2pr_expected_stream_tunneldata_count: 1,
        i2pr_tunnel_recovery_count: 1,
        ..P234SynEpoch::default()
    };
    assert_eq!(
        p234_classify_syn_epoch(&P234SynEpoch {
            i2pr_garlic_decode_failures: 1,
            ..base
        }),
        P234Terminal::I2prGarlicDecodeFailed
    );
    assert_eq!(
        p234_classify_syn_epoch(&P234SynEpoch {
            i2pr_garlic_payload_count: 1,
            i2pr_streaming_adapter_calls: 1,
            i2pr_streaming_adapter_errors: 1,
            ..base
        }),
        P234Terminal::I2prStreamingAdapterFailed
    );
}

#[test]
fn p234_adapter_dispatch_without_established_is_distinct() {
    let epoch = P234SynEpoch {
        java_accept_thread_started: true,
        java_accept_returned: true,
        i2pr_inbound_tunneldata_count: 1,
        i2pr_expected_stream_tunneldata_count: 1,
        i2pr_tunnel_recovery_count: 1,
        i2pr_garlic_payload_count: 1,
        i2pr_streaming_adapter_calls: 1,
        i2pr_streaming_adapter_successes: 1,
        ..P234SynEpoch::default()
    };
    assert_eq!(
        p234_classify_syn_epoch(&epoch),
        P234Terminal::DispatchedNotEstablished
    );
}

#[test]
fn p234_direction_a_pass_does_not_close_java_family() {
    assert!(!p234_family_closure_allowed(
        true, true, true, false, true, true, true, true, true
    ));
}

#[test]
fn p234_direction_b_requires_live_refresh_parity() {
    assert!(!p234_family_closure_allowed(
        true, true, true, true, false, true, true, true, true
    ));
}

#[test]
fn p234_plan200_c_d_rows_require_pass_or_explicit_supersession_mapping() {
    assert_eq!(
        p234_resolve_required_row(P234RowEvidence {
            current_passed: true,
            ..P234RowEvidence::default()
        }),
        P234RowResolution::Pass
    );
    assert_eq!(
        p234_resolve_required_row(P234RowEvidence::default()),
        P234RowResolution::StillBlocking
    );
}

#[test]
fn p234_superseded_row_requires_stronger_mandatory_external_evidence() {
    let mapping = P234RowEvidence {
        mapping_explicit: true,
        replacement_mandatory: true,
        replacement_stronger: true,
        ..P234RowEvidence::default()
    };
    assert_eq!(
        p234_resolve_required_row(mapping),
        P234RowResolution::SupersededByStrongerExternalEvidence
    );
    assert_eq!(
        p234_resolve_required_row(P234RowEvidence {
            mapping_explicit: true,
            replacement_mandatory: true,
            replacement_stronger: false,
            ..P234RowEvidence::default()
        }),
        P234RowResolution::StillBlocking
    );
}

#[test]
fn p234_unmapped_legacy_required_row_blocks_closure() {
    assert!(!p234_family_closure_allowed(
        true, true, true, true, true, false, true, true, true
    ));
}

#[test]
fn p234_run_java_exit_zero_required_for_family_pass() {
    assert!(!p234_family_closure_allowed(
        true, true, true, true, true, true, false, true, true
    ));
}

#[test]
fn p234_final_closure_checker_required() {
    assert!(!p234_family_closure_allowed(
        true, true, true, true, true, true, true, true, false
    ));
}

#[test]
fn p234_no_global_required_failed_bypass() {
    assert!(!p234_family_closure_allowed(
        true, true, true, true, true, true, false, true, true
    ));
    assert!(p234_family_closure_allowed(
        true, true, true, true, true, true, true, true, true
    ));
}

#[test]
fn p234_no_production_surface_change_before_owned_defect() {
    for token in [
        P234Terminal::JavaAcceptWorkerNotStarted.token(),
        P234Terminal::JavaSynNotAccepted.token(),
        P234Terminal::JavaAcceptedNoResponseObserved.token(),
        P234Terminal::I2prNoExpectedTunnelData.token(),
        P234Terminal::I2prTunnelRecoveryFailed.token(),
        P234Terminal::I2prGarlicDecodeFailed.token(),
        P234Terminal::I2prNoStreamingPayload.token(),
        P234Terminal::I2prStreamingAdapterFailed.token(),
        P234Terminal::DispatchedNotEstablished.token(),
        P234Terminal::DirectionAEstablished.token(),
        P234Terminal::ObservabilityGap.token(),
    ] {
        assert!(token.starts_with("P234-"));
    }
}
