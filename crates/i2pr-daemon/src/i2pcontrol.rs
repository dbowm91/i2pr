//! Plan 287 secure base I2PControl JSON-RPC, authentication, and TLS server.
//!
//! The daemon owns the listener, accepted sockets, TLS identity/loading,
//! token generation/storage/expiry, source-IP authentication throttling,
//! connection/request permits, request body and batch admission, request
//! deadlines/cancellation, and typed dispatch into later capability
//! handles. `i2pr-i2pcontrol` owns only decode/validation/envelope/error
//! semantics.
//!
//! The surface is disabled by default and loopback-only by default. There
//! is no plaintext fallback and no fallback from bad explicit TLS material
//! to managed TLS. Tokens live in memory only and die with the service.
//!
//! Wire contract (frozen by Plan 286):
//! - JSON-RPC exactly 2.0 over HTTPS `POST`; named-object params only.
//! - `Authenticate` is the single public method; `RouterInfo`,
//!   `AddressBook`, `TunnelManager`, and `ClientServicesInfo` require a
//!   live token in `params.Token` (the `X-I2PControl-Token` header is
//!   accepted for compatibility and must agree when both are present).
//! - One request per connection; the connection always closes afterwards.
//! - Single requests and non-empty batches (≤ 32 elements) execute
//!   sequentially under one held in-flight permit; responses preserve
//!   input order; `Authenticate` mints inside a batch are invisible to
//!   sibling elements.
//! - Dispatch: `Authenticate` executes; `RouterInfo` and
//!   `ClientServicesInfo` answer the select form over inspection
//!   handles; `TunnelManager` executes the seven lifecycle actions over
//!   installed control state; `AddressBook` keeps the typed
//!   not-yet-available floor; unknown methods answer method-not-found.
//!   No router state is fabricated to exercise dispatch.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use i2pr_i2pcontrol::{
    AuthErrorCode, ContractInventory, JsonRpcErrorCode, JsonRpcRequest, MAX_BATCH_ELEMENTS,
    MAX_LIVE_TOKENS, MAX_PRESENTED_TOKEN_LEN, RequestId as JsonRpcRequestId, TOKEN_BYTES,
    TOKEN_HEADER, TOKEN_LIFETIME_SECS, conformance, error_envelope, jsonrpc, limits,
    matrix_mirrors_inventories, success_envelope,
};
use rand_core::TryRngCore;
use subtle::ConstantTimeEq;
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tracing::{debug, info, warn};
use zeroize::Zeroizing;

use crate::config::{I2pControlConfig, I2pControlPassword};
use crate::i2pcontrol_inspection::{
    InspectionHandles, ServiceEndpoint, client_service_result, router_info_result,
    select_client_services, select_router_info,
};
use crate::i2pcontrol_tunnels::TunnelControlState;
use i2pr_runtime::{CancellationToken, ChildScope};

/// Token lifetime in milliseconds (one day, monotonic).
const TOKEN_LIFETIME_MS: u64 = TOKEN_LIFETIME_SECS * 1_000;
/// Fixed-capacity throttle table (entries, one per observed source IP).
const THROTTLE_CAPACITY: usize = 1_024;
/// Throttle observation window in milliseconds (failure counts reset after it).
const THROTTLE_WINDOW_MS: u64 = 60_000;
/// Free failed attempts per source before a bounded delay applies.
const THROTTLE_FREE_FAILURES: u64 = 4;
/// Delay step per failure beyond the free budget.
const THROTTLE_DELAY_STEP_MS: u64 = 50;
/// Hard upper bound for any authentication delay.
const THROTTLE_DELAY_MAX_MS: u64 = 2_000;
/// Maximum HTTP head (request line + headers) in bytes.
const MAX_HTTP_HEAD_BYTES: usize = 16_384;
/// Maximum captured compatibility-header token in bytes (transport bound;
/// the contract ceiling applies at dispatch).
const MAX_HEADER_TOKEN_BYTES: usize = 512;
/// Concurrent in-flight request ceiling (matches the contract inventory).
const INFLIGHT_REQUESTS: usize = i2pr_i2pcontrol::MAX_INFLIGHT_REQUESTS;

/// Typed service errors. No variant carries passwords, tokens, or payloads.
#[derive(Debug, Error)]
pub enum I2pControlServiceError {
    /// Configuration or TLS material is unusable; nothing was bound.
    #[error("invalid I2PControl configuration: {0}")]
    InvalidConfig(String),
    /// Listener bind failed.
    #[error("I2PControl bind failed on {address}: {source}")]
    Bind {
        /// Requested bind address.
        address: SocketAddr,
        /// Bind failure.
        source: std::io::Error,
    },
    /// Graceful shutdown did not complete before the hard deadline.
    #[error("I2PControl shutdown timed out")]
    ShutdownTimeout,
}

/// Operator password handle. The secret never appears in `Debug` output.
struct ServicePassword(Zeroizing<Vec<u8>>);

impl ServicePassword {
    /// Wraps a copy of the configured password bytes.
    fn new(password: &I2pControlPassword) -> Self {
        Self(Zeroizing::new(password.as_str().as_bytes().to_vec()))
    }

    /// Borrows the secret bytes for bounded constant-time comparison.
    fn as_bytes(&self) -> &[u8] {
        self.0.as_slice()
    }
}

impl std::fmt::Debug for ServicePassword {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("ServicePassword([redacted])")
    }
}

/// Bounded constant-time password comparison over a fixed step budget.
///
/// Runs exactly `MAX_PASSWORD_LEN` byte steps regardless of input lengths
/// so success and failure are indistinguishable by timing up to the bound.
/// Lengths still gate acceptance: only equal lengths can match.
fn ct_password_eq(stored: &[u8], presented: &[u8]) -> bool {
    let len_eq = u8::from(stored.len() == presented.len());
    let mut diff: u8 = 0;
    for index in 0..limits::MAX_PASSWORD_LEN {
        let left = stored.get(index).copied().unwrap_or(0);
        let right = presented.get(index).copied().unwrap_or(0);
        diff |= left ^ right;
    }
    bool::from(diff.ct_eq(&0)) && len_eq == 1
}

/// Mints one opaque hex token from 32 OS-random bytes.
fn mint_token() -> Result<String, I2pControlServiceError> {
    let mut bytes = [0_u8; TOKEN_BYTES];
    i2pr_crypto::OsRng
        .try_fill_bytes(&mut bytes)
        .map_err(|_| I2pControlServiceError::InvalidConfig("entropy unavailable".to_owned()))?;
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut text = String::with_capacity(TOKEN_BYTES * 2);
    for byte in bytes {
        text.push(HEX[usize::from(byte >> 4)] as char);
        text.push(HEX[usize::from(byte & 0x0F)] as char);
    }
    Ok(text)
}

/// Token-table lookup outcome.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TokenOutcome {
    /// Live token; dispatch may proceed.
    Valid,
    /// Unknown token.
    Unknown,
    /// First use after expiry; the record is removed as observed.
    Expired,
}

/// Bounded in-memory token table with deterministic FIFO eviction.
struct TokenTable {
    /// Live records by opaque token.
    entries: HashMap<String, u64>,
    /// Insertion order for deterministic eviction.
    order: VecDeque<String>,
}

impl TokenTable {
    /// Empty table.
    fn new() -> Self {
        Self {
            entries: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    /// Inserts a token minted at `now_ms`, evicting the oldest entries
    /// when the live ceiling is reached.
    fn insert(&mut self, token: String, now_ms: u64) {
        while self.entries.len() >= MAX_LIVE_TOKENS {
            match self.order.pop_front() {
                Some(oldest) => {
                    self.entries.remove(&oldest);
                }
                None => break,
            }
        }
        self.entries.insert(token.clone(), now_ms);
        self.order.push_back(token);
    }

    /// Validates a presented token at `now_ms`. An expired record is
    /// removed on first observation so the next use reports unknown.
    fn validate(&mut self, token: &str, now_ms: u64) -> TokenOutcome {
        match self.entries.get(token).copied() {
            None => TokenOutcome::Unknown,
            Some(created_ms) => {
                if now_ms.saturating_sub(created_ms) >= TOKEN_LIFETIME_MS {
                    self.entries.remove(token);
                    TokenOutcome::Expired
                } else {
                    TokenOutcome::Valid
                }
            }
        }
    }

    /// Live token count.
    fn len(&self) -> usize {
        self.entries.len()
    }
}

/// One source-IP throttle record.
struct ThrottleEntry {
    /// Observed source address (port is never a key).
    ip: IpAddr,
    /// Failures inside the current window.
    failures: u64,
    /// Window start in milliseconds.
    window_start_ms: u64,
}

/// Fixed-capacity source-IP failed-authentication throttle.
struct AuthThrottle {
    /// Bounded records in observation order (oldest first).
    entries: VecDeque<ThrottleEntry>,
}

impl AuthThrottle {
    /// Empty throttle.
    fn new() -> Self {
        Self {
            entries: VecDeque::new(),
        }
    }

    /// Records one failed authentication from `ip` at `now_ms` and
    /// returns the resulting in-window failure count. Entry lookup and
    /// insertion are a single atomic reservation under the caller's lock.
    fn record_failure(&mut self, ip: IpAddr, now_ms: u64) -> u64 {
        for entry in self.entries.iter_mut() {
            if entry.ip == ip {
                if now_ms.saturating_sub(entry.window_start_ms) >= THROTTLE_WINDOW_MS {
                    entry.failures = 1;
                    entry.window_start_ms = now_ms;
                } else {
                    entry.failures = entry.failures.saturating_add(1);
                }
                return entry.failures;
            }
        }
        while self.entries.len() >= THROTTLE_CAPACITY {
            if self.entries.pop_front().is_none() {
                break;
            }
        }
        self.entries.push_back(ThrottleEntry {
            ip,
            failures: 1,
            window_start_ms: now_ms,
        });
        1
    }

    /// Bounded delay for an in-window failure count.
    fn delay_for(failures: u64) -> Duration {
        if failures <= THROTTLE_FREE_FAILURES {
            Duration::ZERO
        } else {
            let steps = failures.saturating_sub(THROTTLE_FREE_FAILURES);
            let delay_ms = steps
                .saturating_mul(THROTTLE_DELAY_STEP_MS)
                .min(THROTTLE_DELAY_MAX_MS);
            Duration::from_millis(delay_ms)
        }
    }

    /// Current in-window failure count for `ip` (zero when unobserved).
    fn failure_count(&self, ip: IpAddr, now_ms: u64) -> u64 {
        for entry in self.entries.iter() {
            if entry.ip == ip {
                if now_ms.saturating_sub(entry.window_start_ms) >= THROTTLE_WINDOW_MS {
                    return 0;
                }
                return entry.failures;
            }
        }
        0
    }

    /// Throttle table size.
    fn len(&self) -> usize {
        self.entries.len()
    }
}

/// Privacy-safe service snapshot: counters only, never secrets.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct I2pControlServiceSnapshot {
    /// Accepted connections since startup.
    pub connections_accepted: u64,
    /// Connections rejected by admission ceilings.
    pub connections_rejected: u64,
    /// Request elements processed (batch members count individually).
    pub requests_processed: u64,
    /// Failed `Authenticate` attempts.
    pub auth_failures: u64,
    /// Currently live tokens.
    pub live_tokens: usize,
    /// Currently tracked throttle entries.
    pub throttle_entries: usize,
    /// Currently available in-flight request permits.
    pub inflight_available: usize,
}

/// One decoded Plan 294 `AddressBook` request: exactly one mode.
enum AddressBookRequest {
    /// Entry operation (`Type` + `Hostname` + optional `Destination` /
    /// `Delete`, where `Delete` presence selects deletion).
    Entry {
        book: i2pr_addressbook::BookKind,
        hostname: String,
        destination: Option<String>,
        delete: bool,
    },
    /// Subscription-set replacement.
    Subscriptions { urls: Vec<String> },
    /// Configuration replacement.
    Config { entries: BTreeMap<String, String> },
}

/// Decodes one `AddressBook` params object into exactly one mode.
/// `Token` is skipped (authentication already consumed it). Unknown
/// fields, mixed modes, and malformed values fail with a static
/// message; request values never render into errors.
fn decode_addressbook_request(
    params: &serde_json::Map<String, serde_json::Value>,
) -> Result<AddressBookRequest, &'static str> {
    const VOCABULARY: [&str; 6] = [
        "Type",
        "Hostname",
        "Destination",
        "Delete",
        "SetSubscriptions",
        "SetConfig",
    ];
    for key in params.keys() {
        if key != "Token" && !VOCABULARY.contains(&key.as_str()) {
            return Err("unknown AddressBook field");
        }
    }
    let entry_mode = params.contains_key("Type")
        || params.contains_key("Hostname")
        || params.contains_key("Destination")
        || params.contains_key("Delete");
    let subscriptions_mode = params.contains_key("SetSubscriptions");
    let config_mode = params.contains_key("SetConfig");
    let modes = u8::from(entry_mode) + u8::from(subscriptions_mode) + u8::from(config_mode);
    if modes != 1 {
        return Err("AddressBook request selects no single operation");
    }
    if subscriptions_mode {
        let urls = match params.get("SetSubscriptions") {
            Some(serde_json::Value::Array(items)) => items
                .iter()
                .map(|item| {
                    item.as_str()
                        .map(str::to_owned)
                        .ok_or("malformed AddressBook field")
                })
                .collect::<Result<Vec<String>, &'static str>>()?,
            _ => return Err("malformed AddressBook field"),
        };
        return Ok(AddressBookRequest::Subscriptions { urls });
    }
    if config_mode {
        let entries = match params.get("SetConfig") {
            Some(serde_json::Value::Object(map)) => map
                .iter()
                .map(|(key, value)| {
                    i2pr_i2pcontrol::address_book::parse_set_config_key(key)
                        .map_err(|_| "malformed AddressBook field")?;
                    let internal_key = match key.as_str() {
                        "subscriptions" => "subscriptions",
                        "update_delay" => "refresh_interval",
                        "published_addressbook" => "published_book",
                        "router_addressbook" => "router_book",
                        "local_addressbook" => "local_book",
                        "private_addressbook" => "private_book",
                        "proxy_port" => "proxy_port",
                        "proxy_host" => "proxy_host",
                        "log" => "log_file",
                        "theme" => "theme",
                        // Canonical values are retained so dispatch can report
                        // the owning capability gap instead of misclassifying
                        // valid Proposal vocabulary as malformed input.
                        "should_publish" => "should_publish",
                        "etags" => "etags",
                        "last_modified" => "last_modified",
                        _ => return Err("malformed AddressBook field"),
                    };
                    value
                        .as_str()
                        .map(|text| (internal_key.to_owned(), text.to_owned()))
                        .ok_or("malformed AddressBook field")
                })
                .collect::<Result<BTreeMap<String, String>, &'static str>>()?,
            _ => return Err("malformed AddressBook field"),
        };
        return Ok(AddressBookRequest::Config { entries });
    }
    let book = match params.get("Type").and_then(serde_json::Value::as_str) {
        Some(name) => {
            i2pr_addressbook::BookKind::parse(name).map_err(|_| "malformed AddressBook field")?
        }
        None => return Err("malformed AddressBook field"),
    };
    let hostname = match params.get("Hostname").and_then(serde_json::Value::as_str) {
        Some(name) => name.to_owned(),
        None => return Err("malformed AddressBook field"),
    };
    let destination = match params.get("Destination") {
        None => None,
        Some(serde_json::Value::String(text)) => Some(text.clone()),
        Some(_) => return Err("malformed AddressBook field"),
    };
    Ok(AddressBookRequest::Entry {
        book,
        hostname,
        destination,
        delete: params.contains_key("Delete"),
    })
}

/// Supervised Plan 287 I2PControl service state (daemon-owned).
pub struct I2pControlServiceState {
    /// Validated service configuration.
    config: I2pControlConfig,
    /// Operator password (redacted handle).
    password: ServicePassword,
    /// TLS server configuration (managed or explicit).
    tls: Arc<rustls::ServerConfig>,
    /// Live token table.
    tokens: Mutex<TokenTable>,
    /// Source-IP throttle table.
    throttle: Mutex<AuthThrottle>,
    /// Bounded accepted-connection permits.
    connection_permits: Arc<Semaphore>,
    /// Bounded in-flight request permits (one held per request body).
    inflight_permits: Arc<Semaphore>,
    /// Monotonic epoch for token/throttle timestamps.
    epoch: Instant,
    /// Accepted-connection counter.
    connections_accepted: AtomicU64,
    /// Rejected-connection counter.
    connections_rejected: AtomicU64,
    /// Processed-request counter.
    requests_processed: AtomicU64,
    /// Failed-authentication counter.
    auth_failures: AtomicU64,
    /// Connection-id allocator (observability only).
    next_connection_id: AtomicU64,
    /// Plan 288 narrow inspection handles (static config truth plus
    /// publish-gated live snapshots from owning services).
    inspection: Arc<InspectionHandles>,
    /// Plan 289 TunnelManager control state (None for standalone
    /// construction; production composition always installs it).
    control: Mutex<Option<Arc<TunnelControlState>>>,
    /// Plan 294 AddressBook manager (None for standalone
    /// construction; production composition installs it when the
    /// subsystem is configured).
    addressbook: Mutex<Option<Arc<crate::addressbook::AddressBookManager>>>,
}

impl I2pControlServiceState {
    /// Constructs the service from validated configuration.
    ///
    /// Refuses disabled configuration (nothing is allocated), checks the
    /// frozen contract inventory, wraps the password in a redacted
    /// handle, and builds TLS material before any listener bind. No
    /// managed-certificate side effect escapes construction: the managed
    /// certificate is an in-memory value owned by this state.
    ///
    /// Standalone construction assumes the controlled network id (`2`)
    /// with no service inventory and unpublished live owners; production
    /// composition always uses [`Self::new_with_inspection`] with
    /// [`InspectionHandles::from_config`].
    pub fn new(config: I2pControlConfig) -> Result<Self, I2pControlServiceError> {
        let inspection = Arc::new(InspectionHandles::new(
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
        ));
        Self::new_with_inspection(config, inspection)
    }

    /// Constructs the service with explicit inspection handles.
    ///
    /// Production composition builds the handles from the validated
    /// daemon configuration so RouterInfo/ClientServicesInfo report
    /// static truth and owner-published snapshots.
    pub fn new_with_inspection(
        config: I2pControlConfig,
        inspection: Arc<InspectionHandles>,
    ) -> Result<Self, I2pControlServiceError> {
        if !config.enabled {
            return Err(I2pControlServiceError::InvalidConfig(
                "I2PControl service is disabled".to_owned(),
            ));
        }
        if !conformance::assert_frozen_counts(&ContractInventory::current()) {
            return Err(I2pControlServiceError::InvalidConfig(
                "contract inventory drifted from the frozen Plan 286 counts".to_owned(),
            ));
        }
        if !matrix_mirrors_inventories() {
            return Err(I2pControlServiceError::InvalidConfig(
                "source matrix drifted from the frozen Plan 286 inventories".to_owned(),
            ));
        }
        let tls = build_tls_config(&config)?;
        let max_connections = usize::try_from(config.max_connections)
            .map_err(|_| I2pControlServiceError::InvalidConfig("max_connections".to_owned()))?;
        let password = ServicePassword::new(&config.password);
        Ok(Self {
            config,
            password,
            tls,
            tokens: Mutex::new(TokenTable::new()),
            throttle: Mutex::new(AuthThrottle::new()),
            connection_permits: Arc::new(Semaphore::new(max_connections.max(1))),
            inflight_permits: Arc::new(Semaphore::new(INFLIGHT_REQUESTS)),
            epoch: Instant::now(),
            connections_accepted: AtomicU64::new(0),
            connections_rejected: AtomicU64::new(0),
            requests_processed: AtomicU64::new(0),
            auth_failures: AtomicU64::new(0),
            next_connection_id: AtomicU64::new(1),
            inspection,
            control: Mutex::new(None),
            addressbook: Mutex::new(None),
        })
    }

    /// Monotonic milliseconds since construction.
    fn now_ms(&self) -> u64 {
        self.epoch.elapsed().as_millis().min(u128::from(u64::MAX)) as u64
    }

    /// Privacy-safe snapshot.
    pub fn snapshot(&self) -> I2pControlServiceSnapshot {
        I2pControlServiceSnapshot {
            connections_accepted: self.connections_accepted.load(Ordering::Relaxed),
            connections_rejected: self.connections_rejected.load(Ordering::Relaxed),
            requests_processed: self.requests_processed.load(Ordering::Relaxed),
            auth_failures: self.auth_failures.load(Ordering::Relaxed),
            live_tokens: self.tokens.lock().map(|table| table.len()).unwrap_or(0),
            throttle_entries: self
                .throttle
                .lock()
                .map(|throttle| throttle.len())
                .unwrap_or(0),
            inflight_available: self.inflight_permits.available_permits(),
        }
    }

    /// Live token count (tests).
    pub fn live_token_count(&self) -> usize {
        self.tokens.lock().map(|table| table.len()).unwrap_or(0)
    }

    /// In-window throttle failures for `ip` at the current instant (tests).
    pub fn throttle_failure_count(&self, ip: IpAddr) -> u64 {
        let now_ms = self.now_ms();
        self.throttle
            .lock()
            .map(|throttle| throttle.failure_count(ip, now_ms))
            .unwrap_or(0)
    }

    /// Available in-flight permits (shutdown-baseline tests).
    pub fn inflight_available(&self) -> usize {
        self.inflight_permits.available_permits()
    }

    /// Inspection handles backing RouterInfo/ClientServicesInfo.
    pub fn inspection(&self) -> &Arc<InspectionHandles> {
        &self.inspection
    }

    /// Installs the Plan 289 TunnelManager control state. Production
    /// composition calls this once before serving; standalone
    /// construction leaves control unavailable by design.
    pub fn set_control_manager(&self, control: Arc<TunnelControlState>) {
        if let Ok(mut slot) = self.control.lock() {
            *slot = Some(control);
        }
    }

    /// Installs the Plan 294 AddressBook manager. Production
    /// composition calls this once after subsystem activation;
    /// standalone construction leaves AddressBook unavailable with
    /// an explicit Plan 294 marker.
    pub fn set_addressbook_manager(&self, manager: Arc<crate::addressbook::AddressBookManager>) {
        if let Ok(mut slot) = self.addressbook.lock() {
            *slot = Some(manager);
        }
    }

    /// Runs the supervised listener until cancellation or fatal bind failure.
    pub async fn run(
        self: Arc<Self>,
        bind_address: SocketAddr,
        children: ChildScope,
        cancellation: CancellationToken,
    ) -> Result<(), I2pControlServiceError> {
        // Plan 289 control startup runs before the listener binds so
        // restored tunnels are serving when the control plane answers.
        // Per-definition failures isolate; they never fail the service.
        // (The lock scope ends before the await: no guard crosses it.)
        let control = self
            .control
            .lock()
            .map(|guard| guard.clone())
            .unwrap_or(None);
        if let Some(control) = control {
            let failures = control.startup(&children, &cancellation).await;
            for (name, reason) in failures {
                warn!(tunnel = %name, reason = %reason, "control tunnel failed at startup");
            }
            // Plan 292: the daemon-owned idle sweeper ticks over
            // control-owned runtimes for the whole service lifetime.
            control.spawn_idle_sweeper(&children, &cancellation);
        }
        let (listener, _bound_address) = self.bind(bind_address).await?;
        self.serve(listener, children, cancellation).await
    }

    /// Binds a [`TcpListener`] to the configured address.
    pub async fn bind(
        &self,
        bind_address: SocketAddr,
    ) -> Result<(TcpListener, SocketAddr), I2pControlServiceError> {
        let listener = TcpListener::bind(bind_address).await.map_err(|source| {
            I2pControlServiceError::Bind {
                address: bind_address,
                source,
            }
        })?;
        let bound_address = listener.local_addr().unwrap_or(bind_address);
        Ok((listener, bound_address))
    }

    /// Accepts TLS connections until cancellation fires, then cancels
    /// every child request. Permits release on every drop path.
    pub async fn serve(
        self: Arc<Self>,
        listener: TcpListener,
        children: ChildScope,
        cancellation: CancellationToken,
    ) -> Result<(), I2pControlServiceError> {
        let bind_address = listener.local_addr().map_err(|_| {
            I2pControlServiceError::InvalidConfig("listener had no local_addr".to_owned())
        })?;
        info!(
            address = %bind_address,
            max_connections = self.config.max_connections,
            "I2PControl loopback TLS listener bound"
        );
        let child_token = cancellation.child_token();
        loop {
            tokio::select! {
                biased;
                _ = child_token.cancelled() => {
                    debug!("i2pcontrol listener cancellation observed");
                    break;
                }
                accept = listener.accept() => {
                    let (stream, peer) = match accept {
                        Ok(value) => value,
                        Err(error) => {
                            warn!(error = %error, "i2pcontrol accept failed");
                            continue;
                        }
                    };
                    let permit = match Arc::clone(&self.connection_permits).try_acquire_owned() {
                        Ok(permit) => permit,
                        Err(_) => {
                            self.connections_rejected.fetch_add(1, Ordering::Relaxed);
                            drop(stream);
                            continue;
                        }
                    };
                    let peer_ip = peer.ip();
                    let connection_id = self.next_connection_id.fetch_add(1, Ordering::Relaxed);
                    self.connections_accepted.fetch_add(1, Ordering::Relaxed);
                    let state = Arc::clone(&self);
                    if let Err(error) = children.clone().spawn(move |task_cancellation| {
                        let state = state;
                        async move {
                            handle_connection(state, connection_id, stream, peer_ip, permit, task_cancellation).await;
                            Ok(())
                        }
                    }) {
                        warn!(error = %error, "failed to spawn I2PControl connection task");
                    }
                }
            }
        }
        let _ = child_token.cancel(i2pr_core::CancellationReason::ParentScope);
        Ok(())
    }

    /// Decodes, authenticates, and dispatches one HTTP body.
    ///
    /// Pure w.r.t. I/O: operates only on the bounded token/throttle
    /// tables under `&self` locks, so concurrent calls serialize
    /// atomically. Callers must enforce the body ceiling before calling.
    pub(crate) async fn dispatch_body(
        &self,
        body: &[u8],
        header_token: Option<&str>,
        peer_ip: IpAddr,
        now_ms: u64,
    ) -> DispatchOutcome {
        let parsed: serde_json::Value = match serde_json::from_slice(body) {
            Ok(value) => value,
            Err(_) => {
                return DispatchOutcome::json(
                    error_envelope(
                        None,
                        JsonRpcErrorCode::ParseError.code(),
                        JsonRpcErrorCode::ParseError.message(),
                    ),
                    Duration::ZERO,
                );
            }
        };
        let (is_batch, elements) = match jsonrpc::split_body(&parsed) {
            Ok(split) => split,
            Err(_) => {
                return DispatchOutcome::json(
                    error_envelope(
                        None,
                        JsonRpcErrorCode::InvalidRequest.code(),
                        JsonRpcErrorCode::InvalidRequest.message(),
                    ),
                    Duration::ZERO,
                );
            }
        };
        if is_batch {
            // Empty batches and over-ceiling batches are body-level
            // admission failures: exactly one invalid-request response.
            if elements.is_empty() || elements.len() > MAX_BATCH_ELEMENTS {
                return DispatchOutcome::json(
                    error_envelope(
                        None,
                        JsonRpcErrorCode::InvalidRequest.code(),
                        JsonRpcErrorCode::InvalidRequest.message(),
                    ),
                    Duration::ZERO,
                );
            }
            let mut responses = Vec::with_capacity(elements.len());
            let mut deferred_mints: Vec<(String, u64)> = Vec::new();
            let mut post_delay = Duration::ZERO;
            let processed = elements.len();
            for element in elements {
                let (response, delay) = self
                    .process_element(
                        element,
                        header_token,
                        peer_ip,
                        now_ms,
                        &mut deferred_mints,
                        true,
                    )
                    .await;
                if delay > post_delay {
                    post_delay = delay;
                }
                if let Some(response) = response {
                    responses.push(response);
                }
            }
            // Deferred batch mints become visible only after every
            // sibling element was processed: no token propagation.
            if let Ok(mut tokens) = self.tokens.lock() {
                for (token, minted_ms) in deferred_mints {
                    tokens.insert(token, minted_ms);
                }
            }
            self.requests_processed
                .fetch_add(processed as u64, Ordering::Relaxed);
            if responses.is_empty() {
                DispatchOutcome::no_content(post_delay)
            } else {
                DispatchOutcome::json(serde_json::Value::Array(responses), post_delay)
            }
        } else {
            let (response, delay) = self
                .process_element(
                    &parsed,
                    header_token,
                    peer_ip,
                    now_ms,
                    &mut Vec::new(),
                    false,
                )
                .await;
            self.requests_processed.fetch_add(1, Ordering::Relaxed);
            match response {
                Some(response) => DispatchOutcome::json(response, delay),
                None => DispatchOutcome::no_content(delay),
            }
        }
    }

    /// Processes one batch element or single body.
    async fn process_element(
        &self,
        element: &serde_json::Value,
        header_token: Option<&str>,
        peer_ip: IpAddr,
        now_ms: u64,
        deferred_mints: &mut Vec<(String, u64)>,
        in_batch: bool,
    ) -> (Option<serde_json::Value>, Duration) {
        let request = match JsonRpcRequest::decode(element) {
            Ok(request) => request,
            Err(_) => {
                return (
                    Some(error_envelope(
                        None,
                        JsonRpcErrorCode::InvalidRequest.code(),
                        JsonRpcErrorCode::InvalidRequest.message(),
                    )),
                    Duration::ZERO,
                );
            }
        };
        let is_notification = request.is_notification();
        let (response, delay) = self
            .process_request(
                &request,
                header_token,
                peer_ip,
                now_ms,
                deferred_mints,
                in_batch,
            )
            .await;
        if is_notification {
            (None, delay)
        } else {
            (Some(response), delay)
        }
    }

    /// Authenticates and dispatches one decoded request.
    async fn process_request(
        &self,
        request: &JsonRpcRequest,
        header_token: Option<&str>,
        peer_ip: IpAddr,
        now_ms: u64,
        deferred_mints: &mut Vec<(String, u64)>,
        in_batch: bool,
    ) -> (serde_json::Value, Duration) {
        let id = request.id.as_ref();
        let method = match i2pr_i2pcontrol::Method::parse(&request.method) {
            Ok(method) => method,
            Err(_) => {
                return (
                    error_envelope(
                        id,
                        JsonRpcErrorCode::MethodNotFound.code(),
                        JsonRpcErrorCode::MethodNotFound.message(),
                    ),
                    Duration::ZERO,
                );
            }
        };
        match method {
            i2pr_i2pcontrol::Method::Authenticate => self.process_authenticate(
                id,
                &request.params,
                peer_ip,
                now_ms,
                deferred_mints,
                in_batch,
            ),
            i2pr_i2pcontrol::Method::RouterInfo => {
                match self.check_token(&request.params, header_token, now_ms) {
                    Err((code, message)) => (error_envelope(id, code, message), Duration::ZERO),
                    Ok(()) => self.process_router_info(id, &request.params, now_ms),
                }
            }
            i2pr_i2pcontrol::Method::ClientServicesInfo => {
                match self.check_token(&request.params, header_token, now_ms) {
                    Err((code, message)) => (error_envelope(id, code, message), Duration::ZERO),
                    Ok(()) => self.process_client_services(id, &request.params),
                }
            }
            i2pr_i2pcontrol::Method::TunnelManager => {
                match self.check_token(&request.params, header_token, now_ms) {
                    Err((code, message)) => (error_envelope(id, code, message), Duration::ZERO),
                    Ok(()) => self.process_tunnel_manager(id, &request.params).await,
                }
            }
            i2pr_i2pcontrol::Method::AddressBook => {
                match self.check_token(&request.params, header_token, now_ms) {
                    Err((code, message)) => (error_envelope(id, code, message), Duration::ZERO),
                    Ok(()) => self.process_addressbook(id, &request.params),
                }
            }
        }
    }

    /// Dispatches an authenticated `AddressBook` request over the Plan
    /// 294 canonical form. Exactly one mode per request: an entry
    /// operation (`Type` + `Hostname` + optional `Destination` /
    /// `Delete`), a subscription replacement (`SetSubscriptions`), or
    /// a config replacement (`SetConfig`). Mixed modes, unknown
    /// fields, and malformed values are invalid params; the whole
    /// request validates before any mutation. Without an installed
    /// manager (standalone construction) every request fails
    /// explicitly with the Plan 294 marker.
    fn process_addressbook(
        &self,
        id: Option<&JsonRpcRequestId>,
        params: &serde_json::Map<String, serde_json::Value>,
    ) -> (serde_json::Value, Duration) {
        let manager = match self.addressbook.lock().ok().and_then(|slot| slot.clone()) {
            Some(manager) => manager,
            None => {
                return (
                    error_envelope(
                        id,
                        JsonRpcErrorCode::InternalError.code(),
                        "AddressBook owner is not installed (Plan 294)",
                    ),
                    Duration::ZERO,
                );
            }
        };
        if !manager.is_active() {
            return (
                error_envelope(
                    id,
                    JsonRpcErrorCode::InternalError.code(),
                    "AddressBook subsystem is not active",
                ),
                Duration::ZERO,
            );
        }
        let request = match decode_addressbook_request(params) {
            Ok(request) => request,
            Err(message) => {
                return (
                    error_envelope(id, JsonRpcErrorCode::InvalidParams.code(), message),
                    Duration::ZERO,
                );
            }
        };
        match request {
            AddressBookRequest::Entry {
                book,
                hostname,
                destination,
                delete,
            } => match manager.apply_entry(i2pr_addressbook::EntryMutation {
                book,
                hostname,
                destination,
                delete,
            }) {
                Ok(i2pr_addressbook::EntryOutcome::Created) => {
                    Self::addressbook_success(id, "entry created")
                }
                Ok(i2pr_addressbook::EntryOutcome::Updated) => {
                    Self::addressbook_success(id, "entry updated")
                }
                Ok(i2pr_addressbook::EntryOutcome::Deleted) => {
                    Self::addressbook_success(id, "entry deleted")
                }
                Err(error) => Self::addressbook_manager_error(id, &error),
            },
            AddressBookRequest::Subscriptions { urls } => {
                match manager.replace_subscriptions(&urls) {
                    Ok(true) => {
                        manager.run_queued_refreshes(
                            i2pr_addressbook::RefreshReason::SubscriptionsReplaced,
                        );
                        Self::addressbook_success(id, "subscriptions replaced")
                    }
                    Ok(false) => Self::addressbook_success(id, "subscriptions unchanged"),
                    Err(error) => Self::addressbook_manager_error(id, &error),
                }
            }
            AddressBookRequest::Config { entries } => {
                if ["should_publish", "etags", "last_modified"]
                    .iter()
                    .any(|key| entries.contains_key(*key))
                {
                    return (
                        error_envelope(
                            id,
                            JsonRpcErrorCode::InternalError.code(),
                            "AddressBook config field owner is unavailable (Plan 321)",
                        ),
                        Duration::ZERO,
                    );
                }
                match manager.apply_config(&entries) {
                    Ok(true) => Self::addressbook_success(id, "config applied"),
                    Ok(false) => Self::addressbook_success(id, "config unchanged"),
                    Err(error) => Self::addressbook_manager_error(id, &error),
                }
            }
        }
    }

    /// Builds a canonical `{success, message}` result envelope.
    fn addressbook_success(
        id: Option<&JsonRpcRequestId>,
        message: &'static str,
    ) -> (serde_json::Value, Duration) {
        (
            success_envelope(id, serde_json::json!({"success": true, "message": message})),
            Duration::ZERO,
        )
    }

    /// Maps a manager error to its wire envelope (static strings only;
    /// request values never render).
    fn addressbook_manager_error(
        id: Option<&JsonRpcRequestId>,
        error: &crate::addressbook::AddressBookManagerError,
    ) -> (serde_json::Value, Duration) {
        use crate::addressbook::AddressBookManagerError as ManagerError;
        match error {
            ManagerError::Inactive | ManagerError::StoreUnavailable => (
                error_envelope(
                    id,
                    JsonRpcErrorCode::InternalError.code(),
                    &error.to_string(),
                ),
                Duration::ZERO,
            ),
            ManagerError::Rejected(_) => (
                error_envelope(
                    id,
                    JsonRpcErrorCode::InvalidParams.code(),
                    &error.to_string(),
                ),
                Duration::ZERO,
            ),
        }
    }

    /// Dispatches an authenticated `TunnelManager` request over the Plan
    /// 289 envelope. Envelope rejections are invalid params; control
    /// errors carry their own wire code. Without installed control
    /// state (standalone construction) every request fails explicitly
    /// with the Plan 289 marker.
    async fn process_tunnel_manager(
        &self,
        id: Option<&JsonRpcRequestId>,
        params: &serde_json::Map<String, serde_json::Value>,
    ) -> (serde_json::Value, Duration) {
        let request = match i2pr_i2pcontrol::decode_tunnel_request(params) {
            Ok(request) => request,
            Err(error) => {
                return (
                    error_envelope(
                        id,
                        JsonRpcErrorCode::InvalidParams.code(),
                        &format!("{error}"),
                    ),
                    Duration::ZERO,
                );
            }
        };
        if request.all {
            return (
                error_envelope(
                    id,
                    JsonRpcErrorCode::InternalError.code(),
                    "TunnelManager All action is unavailable (Plan 323)",
                ),
                Duration::ZERO,
            );
        }
        let control = match self
            .control
            .lock()
            .map(|guard| guard.clone())
            .unwrap_or(None)
        {
            Some(control) => control,
            None => {
                return (
                    error_envelope(
                        id,
                        JsonRpcErrorCode::InternalError.code(),
                        "TunnelManager control state unavailable (Plan 289)",
                    ),
                    Duration::ZERO,
                );
            }
        };
        match control.dispatch(&request).await {
            Ok(value) => (success_envelope(id, value), Duration::ZERO),
            Err(error) => (
                error_envelope(id, error.wire_code(), &format!("{error}")),
                Duration::ZERO,
            ),
        }
    }

    /// Dispatches an authenticated `RouterInfo` request over the base API
    /// and Proposal 170 selector namespaces.
    ///
    /// Selector values are ignored; unknown keys fail with invalid params.
    /// An empty
    /// selection answers with an empty result object. Any unavailable or
    /// unpublished selection fails the whole request explicitly with the
    /// owning-plan marker; no partial response is emitted and no state is
    /// mutated.
    fn process_router_info(
        &self,
        id: Option<&JsonRpcRequestId>,
        params: &serde_json::Map<String, serde_json::Value>,
        now_ms: u64,
    ) -> (serde_json::Value, Duration) {
        let selection = match select_router_info(params) {
            Ok(selection) => selection,
            Err(_) => {
                return (
                    error_envelope(
                        id,
                        JsonRpcErrorCode::InvalidParams.code(),
                        JsonRpcErrorCode::InvalidParams.message(),
                    ),
                    Duration::ZERO,
                );
            }
        };
        // Control-plane uptime in whole seconds (truncating, saturating).
        let uptime_secs = now_ms / 1000;
        let mut result = serde_json::Map::with_capacity(selection.len());
        for field in selection {
            if field.key == "i2p.router.uptime" {
                result.insert(field.key.to_owned(), serde_json::Value::from(now_ms));
                continue;
            }
            let Some(selector) = field.adapter else {
                return (
                    error_envelope(
                        id,
                        JsonRpcErrorCode::InternalError.code(),
                        "RouterInfo selector source is unavailable",
                    ),
                    Duration::ZERO,
                );
            };
            match router_info_result(selector, &self.inspection, uptime_secs) {
                Ok(value) => {
                    let value = match field.key {
                        "i2p.router.netdb.knownpeers" | "i2p.router.netdb.activepeers" => {
                            match value.as_array() {
                                Some(peers) => serde_json::Value::from(peers.len() as u64),
                                None => {
                                    return (
                                        error_envelope(
                                            id,
                                            JsonRpcErrorCode::InternalError.code(),
                                            "RouterInfo owner returned an invalid field shape",
                                        ),
                                        Duration::ZERO,
                                    );
                                }
                            }
                        }
                        _ => value,
                    };
                    result.insert(field.key.to_owned(), value);
                }
                Err(gap) => {
                    return (
                        error_envelope(id, JsonRpcErrorCode::InternalError.code(), &gap.message()),
                        Duration::ZERO,
                    );
                }
            }
        }
        (
            success_envelope(id, serde_json::Value::Object(result)),
            Duration::ZERO,
        )
    }

    /// Dispatches an authenticated `ClientServicesInfo` request over the
    /// same select form. Every service row answers (disabled is truthful
    /// state), so this path is infallible after select validation.
    fn process_client_services(
        &self,
        id: Option<&JsonRpcRequestId>,
        params: &serde_json::Map<String, serde_json::Value>,
    ) -> (serde_json::Value, Duration) {
        let selection = match select_client_services(params) {
            Ok(selection) => selection,
            Err(_) => {
                return (
                    error_envelope(
                        id,
                        JsonRpcErrorCode::InvalidParams.code(),
                        JsonRpcErrorCode::InvalidParams.message(),
                    ),
                    Duration::ZERO,
                );
            }
        };
        let mut result = serde_json::Map::with_capacity(selection.len());
        for service in selection {
            result.insert(
                service.name().to_owned(),
                client_service_result(service, &self.inspection),
            );
        }
        (
            success_envelope(id, serde_json::Value::Object(result)),
            Duration::ZERO,
        )
    }

    /// Executes API version 1 `Authenticate` with the standard error
    /// inventory. Strict floor: only `API` and `Password` travel.
    fn process_authenticate(
        &self,
        id: Option<&JsonRpcRequestId>,
        params: &serde_json::Map<String, serde_json::Value>,
        peer_ip: IpAddr,
        now_ms: u64,
        deferred_mints: &mut Vec<(String, u64)>,
        in_batch: bool,
    ) -> (serde_json::Value, Duration) {
        if params.keys().any(|key| key != "API" && key != "Password") {
            return (
                error_envelope(
                    id,
                    JsonRpcErrorCode::InvalidParams.code(),
                    JsonRpcErrorCode::InvalidParams.message(),
                ),
                Duration::ZERO,
            );
        }
        match params.get("API") {
            None => {
                return (
                    error_envelope(
                        id,
                        AuthErrorCode::MissingApiVersion.code(),
                        AuthErrorCode::MissingApiVersion.message(),
                    ),
                    Duration::ZERO,
                );
            }
            Some(serde_json::Value::Number(number)) => match number.as_u64() {
                Some(1) => {}
                _ => {
                    return (
                        error_envelope(
                            id,
                            AuthErrorCode::UnsupportedApiVersion.code(),
                            AuthErrorCode::UnsupportedApiVersion.message(),
                        ),
                        Duration::ZERO,
                    );
                }
            },
            Some(_) => {
                return (
                    error_envelope(
                        id,
                        JsonRpcErrorCode::InvalidParams.code(),
                        JsonRpcErrorCode::InvalidParams.message(),
                    ),
                    Duration::ZERO,
                );
            }
        }
        let presented = match params.get("Password") {
            Some(serde_json::Value::String(password)) => password.as_str(),
            _ => return self.failed_auth(id, peer_ip, now_ms),
        };
        if presented.len() > limits::MAX_PASSWORD_LEN
            || !ct_password_eq(self.password.as_bytes(), presented.as_bytes())
        {
            return self.failed_auth(id, peer_ip, now_ms);
        }
        let token = match mint_token() {
            Ok(token) => token,
            Err(_) => {
                return (
                    error_envelope(
                        id,
                        JsonRpcErrorCode::InternalError.code(),
                        JsonRpcErrorCode::InternalError.message(),
                    ),
                    Duration::ZERO,
                );
            }
        };
        let minted = token.clone();
        if in_batch {
            deferred_mints.push((token, now_ms));
        } else if let Ok(mut tokens) = self.tokens.lock() {
            tokens.insert(token, now_ms);
        }
        (
            success_envelope(id, serde_json::json!({"API": 1, "Token": minted})),
            Duration::ZERO,
        )
    }

    /// Records a failed authentication and maps it to `-32001` plus the
    /// bounded source-IP delay.
    fn failed_auth(
        &self,
        id: Option<&JsonRpcRequestId>,
        peer_ip: IpAddr,
        now_ms: u64,
    ) -> (serde_json::Value, Duration) {
        self.auth_failures.fetch_add(1, Ordering::Relaxed);
        let failures = self
            .throttle
            .lock()
            .map(|mut throttle| throttle.record_failure(peer_ip, now_ms))
            .unwrap_or(THROTTLE_FREE_FAILURES + 1);
        (
            error_envelope(
                id,
                AuthErrorCode::InvalidPassword.code(),
                AuthErrorCode::InvalidPassword.message(),
            ),
            AuthThrottle::delay_for(failures),
        )
    }

    /// Checks the protected token (`params.Token`, compat header, agreement).
    fn check_token(
        &self,
        params: &serde_json::Map<String, serde_json::Value>,
        header_token: Option<&str>,
        now_ms: u64,
    ) -> Result<(), (i64, &'static str)> {
        let param_token = match params.get("Token") {
            None => None,
            Some(serde_json::Value::String(token)) => Some(token.as_str()),
            Some(_) => {
                return Err((
                    JsonRpcErrorCode::InvalidParams.code(),
                    JsonRpcErrorCode::InvalidParams.message(),
                ));
            }
        };
        let effective = match (param_token, header_token) {
            (None, None) => {
                return Err((
                    AuthErrorCode::MissingToken.code(),
                    AuthErrorCode::MissingToken.message(),
                ));
            }
            (Some(param), None) => param,
            (None, Some(header)) => header,
            (Some(param), Some(header)) => {
                if param != header {
                    return Err((
                        JsonRpcErrorCode::InvalidParams.code(),
                        JsonRpcErrorCode::InvalidParams.message(),
                    ));
                }
                param
            }
        };
        if effective.is_empty() {
            return Err((
                AuthErrorCode::MissingToken.code(),
                AuthErrorCode::MissingToken.message(),
            ));
        }
        if effective.len() > MAX_PRESENTED_TOKEN_LEN {
            return Err((
                AuthErrorCode::InvalidToken.code(),
                AuthErrorCode::InvalidToken.message(),
            ));
        }
        let mut tokens = self.tokens.lock().map_err(|_| {
            (
                JsonRpcErrorCode::InternalError.code(),
                JsonRpcErrorCode::InternalError.message(),
            )
        })?;
        match tokens.validate(effective, now_ms) {
            TokenOutcome::Valid => Ok(()),
            TokenOutcome::Unknown => Err((
                AuthErrorCode::InvalidToken.code(),
                AuthErrorCode::InvalidToken.message(),
            )),
            TokenOutcome::Expired => Err((
                AuthErrorCode::ExpiredToken.code(),
                AuthErrorCode::ExpiredToken.message(),
            )),
        }
    }
}

/// Outcome of dispatching one HTTP body.
pub(crate) struct DispatchOutcome {
    /// Response disposition.
    pub dispatch: HttpDispatch,
    /// Bounded post-processing delay (failed-auth throttle).
    pub post_delay: Duration,
}

/// Response disposition for one HTTP body.
pub(crate) enum HttpDispatch {
    /// JSON body with HTTP 200.
    Json(Vec<u8>),
    /// No response body (HTTP 204).
    NoContent,
}

impl DispatchOutcome {
    /// JSON outcome with an explicit post delay.
    fn json(value: serde_json::Value, post_delay: Duration) -> Self {
        let body = serde_json::to_vec(&value).unwrap_or_else(|_| b"{}".to_vec());
        Self {
            dispatch: HttpDispatch::Json(body),
            post_delay,
        }
    }

    /// Content-free outcome with an explicit post delay.
    fn no_content(post_delay: Duration) -> Self {
        Self {
            dispatch: HttpDispatch::NoContent,
            post_delay,
        }
    }
}

/// Builds the TLS server configuration before any listener bind.
///
/// Explicit operator-owned PEM material is used when configured (loopback
/// or not). Otherwise the bind must be loopback (enforced again here as
/// defense in depth) and an in-memory ephemeral self-signed certificate
/// covering `localhost`, `127.0.0.1`, and `::1` is generated. No
/// filesystem mutation ever occurs on this path.
fn build_tls_config(
    config: &I2pControlConfig,
) -> Result<Arc<rustls::ServerConfig>, I2pControlServiceError> {
    if let (Some(cert_path), Some(key_path)) = (&config.certificate, &config.private_key) {
        let cert_data = std::fs::read(cert_path).map_err(|_| {
            I2pControlServiceError::InvalidConfig("cannot read I2PControl certificate".to_owned())
        })?;
        let key_data = std::fs::read(key_path).map_err(|_| {
            I2pControlServiceError::InvalidConfig("cannot read I2PControl private key".to_owned())
        })?;
        use rustls_pki_types::pem::PemObject;
        let certs: Vec<rustls::pki_types::CertificateDer<'static>> =
            rustls::pki_types::CertificateDer::pem_slice_iter(&cert_data)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| {
                    I2pControlServiceError::InvalidConfig(
                        "cannot parse I2PControl certificate PEM".to_owned(),
                    )
                })?;
        if certs.is_empty() {
            return Err(I2pControlServiceError::InvalidConfig(
                "I2PControl certificate PEM holds no certificate".to_owned(),
            ));
        }
        let key = rustls::pki_types::PrivateKeyDer::from_pem_slice(&key_data).map_err(|_| {
            I2pControlServiceError::InvalidConfig(
                "cannot parse I2PControl private-key PEM".to_owned(),
            )
        })?;
        let server = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .map_err(|_| {
                I2pControlServiceError::InvalidConfig(
                    "I2PControl certificate and key do not combine".to_owned(),
                )
            })?;
        return Ok(Arc::new(server));
    }
    if !config.bind_address.is_loopback() {
        return Err(I2pControlServiceError::InvalidConfig(
            "non-loopback bind requires explicit certificate and private_key".to_owned(),
        ));
    }
    let certified = rcgen::generate_simple_self_signed(vec![
        "localhost".to_owned(),
        "127.0.0.1".to_owned(),
        "::1".to_owned(),
    ])
    .map_err(|_| {
        I2pControlServiceError::InvalidConfig("managed certificate generation failed".to_owned())
    })?;
    let cert_der = certified.cert.der().to_vec();
    let key_der = certified.key_pair.serialize_der();
    let server = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(
            vec![rustls::pki_types::CertificateDer::from(cert_der)],
            rustls::pki_types::PrivateKeyDer::Pkcs8(rustls::pki_types::PrivatePkcs8KeyDer::from(
                key_der,
            )),
        )
        .map_err(|_| {
            I2pControlServiceError::InvalidConfig(
                "managed certificate installation failed".to_owned(),
            )
        })?;
    Ok(Arc::new(server))
}

/// Builds the `POST` response bytes for one dispatch outcome.
fn http_response(outcome: &DispatchOutcome) -> Vec<u8> {
    match &outcome.dispatch {
        HttpDispatch::Json(body) => {
            let mut response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .into_bytes();
            response.extend_from_slice(body);
            response
        }
        HttpDispatch::NoContent => {
            b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec()
        }
    }
}

/// Writes one response and closes the TLS session gracefully.
///
/// `shutdown` emits `close_notify` so well-behaved clients observe a
/// clean end of stream instead of a truncation. Every failure is
/// bounded by `deadline` and ignored: the connection closes either way
/// and permits release on drop.
async fn write_and_close(
    stream: &mut tokio_rustls::server::TlsStream<TcpStream>,
    bytes: &[u8],
    deadline: Duration,
) {
    let _ = tokio::time::timeout(deadline, stream.write_all(bytes)).await;
    let _ = tokio::time::timeout(deadline, stream.shutdown()).await;
}

/// Minimal HTTP framing error response.
fn http_status(status: u16, reason: &str) -> Vec<u8> {
    format!("HTTP/1.1 {status} {reason}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
        .into_bytes()
}

/// Reads one `POST` request head and bounded body.
///
/// Returns the compatibility-header token (when present) and the exact
/// body bytes. Framing violations map to an HTTP status; the caller
/// writes it and closes. Forwarding headers are never consulted: the
/// source address always comes from the real socket.
async fn read_http_request(
    stream: &mut tokio_rustls::server::TlsStream<TcpStream>,
    max_body_bytes: usize,
) -> Result<(Option<String>, Vec<u8>), Vec<u8>> {
    let mut head: Vec<u8> = Vec::new();
    let mut chunk = [0_u8; 1_024];
    loop {
        let count = stream
            .read(&mut chunk)
            .await
            .map_err(|_| http_status(400, "Bad Request"))?;
        if count == 0 {
            return Err(http_status(400, "Bad Request"));
        }
        head.extend_from_slice(&chunk[..count]);
        if head.len() > MAX_HTTP_HEAD_BYTES {
            return Err(http_status(431, "Request Header Fields Too Large"));
        }
        if head.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
    }
    let head_end = head
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|index| index + 4)
        .unwrap_or(head.len());
    // A single read commonly carries the head and part (or all) of the
    // body: bytes past the head separator are already-buffered body, not
    // a second request (one request travels per connection).
    let buffered_body = head[head_end..].to_vec();
    let head_text =
        std::str::from_utf8(&head[..head_end]).map_err(|_| http_status(400, "Bad Request"))?;
    let mut lines = head_text.split("\r\n");
    let request_line = lines.next().unwrap_or("");
    let mut parts = request_line.split(' ');
    let method = parts.next().unwrap_or("");
    let _target = parts.next().unwrap_or("");
    let version = parts.next().unwrap_or("");
    if parts.next().is_some() || method.is_empty() || version.is_empty() {
        return Err(http_status(400, "Bad Request"));
    }
    if method != "POST" {
        return Err(http_status(405, "Method Not Allowed"));
    }
    if version != "HTTP/1.0" && version != "HTTP/1.1" {
        return Err(http_status(400, "Bad Request"));
    }
    let mut content_length: Option<usize> = None;
    let mut header_token: Option<String> = None;
    for line in lines {
        if line.is_empty() {
            continue;
        }
        let (name, value) = line
            .split_once(':')
            .ok_or_else(|| http_status(400, "Bad Request"))?;
        let name = name.trim().to_ascii_lowercase();
        let value = value.trim();
        match name.as_str() {
            "content-length" => {
                let parsed: usize = value.parse().map_err(|_| http_status(400, "Bad Request"))?;
                if parsed > max_body_bytes {
                    return Err(http_status(413, "Content Too Large"));
                }
                match content_length {
                    Some(previous) if previous != parsed => {
                        return Err(http_status(400, "Bad Request"));
                    }
                    Some(_) => {}
                    None => content_length = Some(parsed),
                }
            }
            "transfer-encoding" => {
                return Err(http_status(400, "Bad Request"));
            }
            name if name == TOKEN_HEADER.to_ascii_lowercase() => {
                if value.len() > MAX_HEADER_TOKEN_BYTES {
                    return Err(http_status(400, "Bad Request"));
                }
                header_token = Some(value.to_owned());
            }
            _ => {}
        }
    }
    let content_length = content_length.ok_or_else(|| http_status(400, "Bad Request"))?;
    if buffered_body.len() > content_length {
        return Err(http_status(400, "Bad Request"));
    }
    let mut body = Vec::with_capacity(content_length);
    body.extend_from_slice(&buffered_body);
    if body.len() < content_length {
        let remaining = content_length - body.len();
        let mut rest = vec![0_u8; remaining];
        stream
            .read_exact(&mut rest)
            .await
            .map_err(|_| http_status(400, "Bad Request"))?;
        body.extend_from_slice(&rest);
    }
    Ok((header_token, body))
}

/// Serves one accepted connection: TLS accept, one bounded request body,
/// sequential dispatch, one bounded response, close.
///
/// Every early return drops the connection and in-flight permits: permits
/// release on every path including TLS failures, slow bodies, client
/// disconnects, and cancellation.
async fn handle_connection(
    state: Arc<I2pControlServiceState>,
    _connection_id: u64,
    stream: TcpStream,
    peer_ip: IpAddr,
    _connection_permit: OwnedSemaphorePermit,
    task_cancellation: CancellationToken,
) {
    let acceptor = tokio_rustls::TlsAcceptor::from(Arc::clone(&state.tls));
    let mut stream =
        match tokio::time::timeout(state.config.request_deadline, acceptor.accept(stream)).await {
            Ok(Ok(stream)) => stream,
            _ => return,
        };
    let _inflight = match state.inflight_permits.clone().try_acquire_owned() {
        Ok(permit) => permit,
        Err(_) => {
            write_and_close(
                &mut stream,
                &http_status(503, "Service Unavailable"),
                state.config.request_deadline,
            )
            .await;
            return;
        }
    };
    let max_body_bytes = state.config.max_body_bytes;
    let (header_token, body) = match tokio::time::timeout(
        state.config.request_deadline,
        read_http_request(&mut stream, max_body_bytes),
    )
    .await
    {
        Ok(Ok(request)) => request,
        Ok(Err(status)) => {
            write_and_close(&mut stream, &status, state.config.request_deadline).await;
            return;
        }
        Err(_) => return,
    };
    let now_ms = state.now_ms();
    let outcome = state
        .dispatch_body(&body, header_token.as_deref(), peer_ip, now_ms)
        .await;
    if !outcome.post_delay.is_zero() {
        tokio::select! {
            biased;
            _ = task_cancellation.cancelled() => return,
            _ = tokio::time::sleep(outcome.post_delay) => {}
        }
    }
    let response = http_response(&outcome);
    let write = write_and_close(&mut stream, &response, state.config.request_deadline);
    tokio::select! {
        biased;
        _ = task_cancellation.cancelled() => {},
        _ = write => {},
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    /// Loopback test password (never logged; redaction asserted below).
    const TEST_PASSWORD: &str = "correct-horse-battery-staple-i2pcontrol";

    /// Builds an enabled loopback config with an ephemeral port.
    fn test_config(password: &str) -> I2pControlConfig {
        let text = format!(
            "schema_version = 1\n[router]\ndata_dir = \"state\"\n[i2pcontrol]\nenabled = true\nbind_address = \"127.0.0.1\"\nport = 0\npassword = {password:?}\n"
        );
        let config = Config::parse(&text).expect("test config parses");
        assert!(config.i2pcontrol.enabled);
        config.i2pcontrol
    }

    /// Builds a service directly (same crate, no sockets).
    fn test_state(password: &str) -> I2pControlServiceState {
        I2pControlServiceState::new(test_config(password)).expect("test state builds")
    }

    /// Dispatches a JSON body at a manual instant.
    fn dispatch(
        state: &I2pControlServiceState,
        body: &serde_json::Value,
        header_token: Option<&str>,
        now_ms: u64,
    ) -> DispatchOutcome {
        let bytes = serde_json::to_vec(body).expect("body serializes");
        dispatch_raw(
            state,
            &bytes,
            header_token,
            IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1)),
            now_ms,
        )
    }

    /// Dispatches raw bytes at a manual instant.
    fn dispatch_raw(
        state: &I2pControlServiceState,
        bytes: &[u8],
        header_token: Option<&str>,
        peer_ip: IpAddr,
        now_ms: u64,
    ) -> DispatchOutcome {
        // The dispatch chain is async (Plan 289 reconcile awaits); unit
        // tests drive it through a throwaway current-thread runtime so
        // every existing sync test stays sync.
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime")
            .block_on(state.dispatch_body(bytes, header_token, peer_ip, now_ms))
    }

    /// Extracts the JSON response (panics for content-free outcomes).
    fn json_of(outcome: &DispatchOutcome) -> serde_json::Value {
        match &outcome.dispatch {
            HttpDispatch::Json(bytes) => serde_json::from_slice(bytes).expect("response parses"),
            HttpDispatch::NoContent => panic!("expected a JSON response"),
        }
    }

    /// Runs `Authenticate` and returns the minted token.
    fn authenticate(state: &I2pControlServiceState, now_ms: u64) -> String {
        let outcome = dispatch(
            state,
            &serde_json::json!({
                "jsonrpc": "2.0",
                "method": "Authenticate",
                "params": {"API": 1, "Password": TEST_PASSWORD},
                "id": 1,
            }),
            None,
            now_ms,
        );
        let response = json_of(&outcome);
        assert_eq!(response["id"], serde_json::json!(1));
        response["result"]["Token"]
            .as_str()
            .expect("token minted")
            .to_owned()
    }

    #[test]
    fn authenticate_positive_mints_opaque_token() {
        let state = test_state(TEST_PASSWORD);
        let token = authenticate(&state, 0);
        assert_eq!(token.len(), TOKEN_BYTES * 2);
        assert_eq!(state.live_token_count(), 1);
        assert!(token.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn all_six_auth_errors_are_exact() {
        let state = test_state(TEST_PASSWORD);
        // -32005 missing API version.
        let outcome = dispatch(
            &state,
            &serde_json::json!({"jsonrpc": "2.0", "method": "Authenticate", "params": {"Password": TEST_PASSWORD}, "id": 1}),
            None,
            0,
        );
        assert_eq!(
            json_of(&outcome)["error"]["code"],
            serde_json::json!(-32_005)
        );
        // -32006 unsupported API version.
        let outcome = dispatch(
            &state,
            &serde_json::json!({"jsonrpc": "2.0", "method": "Authenticate", "params": {"API": 2, "Password": TEST_PASSWORD}, "id": 1}),
            None,
            0,
        );
        assert_eq!(
            json_of(&outcome)["error"]["code"],
            serde_json::json!(-32_006)
        );
        // -32001 invalid password (wrong value).
        let outcome = dispatch(
            &state,
            &serde_json::json!({"jsonrpc": "2.0", "method": "Authenticate", "params": {"API": 1, "Password": "wrong"}, "id": 1}),
            None,
            0,
        );
        assert_eq!(
            json_of(&outcome)["error"]["code"],
            serde_json::json!(-32_001)
        );
        // -32001 invalid password (missing password param).
        let outcome = dispatch(
            &state,
            &serde_json::json!({"jsonrpc": "2.0", "method": "Authenticate", "params": {"API": 1}, "id": 1}),
            None,
            0,
        );
        assert_eq!(
            json_of(&outcome)["error"]["code"],
            serde_json::json!(-32_001)
        );
        // -32002 missing token on a protected method.
        let outcome = dispatch(
            &state,
            &serde_json::json!({"jsonrpc": "2.0", "method": "RouterInfo", "params": {}, "id": 2}),
            None,
            0,
        );
        assert_eq!(
            json_of(&outcome)["error"]["code"],
            serde_json::json!(-32_002)
        );
        // -32003 invalid/unknown token.
        let outcome = dispatch(
            &state,
            &serde_json::json!({"jsonrpc": "2.0", "method": "RouterInfo", "params": {"Token": "nope"}, "id": 2}),
            None,
            0,
        );
        assert_eq!(
            json_of(&outcome)["error"]["code"],
            serde_json::json!(-32_003)
        );
        // -32004 first use after expiry, then removal.
        let token = authenticate(&state, 0);
        let request = serde_json::json!({"jsonrpc": "2.0", "method": "RouterInfo", "params": {"Token": token}, "id": 3});
        let outcome = dispatch(&state, &request, None, TOKEN_LIFETIME_MS);
        assert_eq!(
            json_of(&outcome)["error"]["code"],
            serde_json::json!(-32_004)
        );
        let outcome = dispatch(&state, &request, None, TOKEN_LIFETIME_MS + 1);
        assert_eq!(
            json_of(&outcome)["error"]["code"],
            serde_json::json!(-32_003)
        );
    }

    #[test]
    fn protected_known_method_reaches_typed_dispatch() {
        let state = test_state(TEST_PASSWORD);
        let token = authenticate(&state, 0);
        // Plan 288: RouterInfo and ClientServicesInfo answer the select
        // form; an empty selection returns an empty result object.
        for method in ["RouterInfo", "ClientServicesInfo"] {
            let outcome = dispatch(
                &state,
                &serde_json::json!({"jsonrpc": "2.0", "method": method, "params": {"Token": token}, "id": 1}),
                None,
                0,
            );
            let response = json_of(&outcome);
            assert_eq!(
                response["result"],
                serde_json::json!({}),
                "{method} empty selection must succeed empty"
            );
        }
        let (method, marker) = ("AddressBook", "Plan 294");
        {
            let outcome = dispatch(
                &state,
                &serde_json::json!({"jsonrpc": "2.0", "method": method, "params": {"Token": token}, "id": 1}),
                None,
                0,
            );
            let response = json_of(&outcome);
            assert_eq!(response["error"]["code"], serde_json::json!(-32_603));
            assert!(
                response["error"]["message"]
                    .as_str()
                    .expect("message")
                    .contains(marker),
                "missing {marker} marker"
            );
        }
        // Plan 289: TunnelManager without an action is an envelope
        // rejection; a valid envelope without installed control state
        // fails explicitly with the Plan 289 marker.
        let outcome = dispatch(
            &state,
            &serde_json::json!({"jsonrpc": "2.0", "method": "TunnelManager", "params": {"Token": token}, "id": 1}),
            None,
            0,
        );
        assert_eq!(
            json_of(&outcome)["error"]["code"],
            serde_json::json!(-32_602)
        );
        let outcome = dispatch(
            &state,
            &serde_json::json!({"jsonrpc": "2.0", "method": "TunnelManager", "params": {"Token": token, "action": "get"}, "id": 1}),
            None,
            0,
        );
        let response = json_of(&outcome);
        assert_eq!(response["error"]["code"], serde_json::json!(-32_603));
        assert!(
            response["error"]["message"]
                .as_str()
                .expect("message")
                .contains("Plan 289"),
            "missing Plan 289 marker"
        );
        // Unknown methods answer method-not-found without fabricating state.
        let outcome = dispatch(
            &state,
            &serde_json::json!({"jsonrpc": "2.0", "method": "GetRate", "params": {"Token": token}, "id": 1}),
            None,
            0,
        );
        assert_eq!(
            json_of(&outcome)["error"]["code"],
            serde_json::json!(-32_601)
        );
    }

    #[test]
    fn token_table_ceiling_evicts_deterministically() {
        let state = test_state(TEST_PASSWORD);
        let first = authenticate(&state, 0);
        for _ in 1..=MAX_LIVE_TOKENS {
            authenticate(&state, 0);
        }
        assert_eq!(state.live_token_count(), MAX_LIVE_TOKENS);
        // The oldest token was evicted by bounded FIFO.
        let outcome = dispatch(
            &state,
            &serde_json::json!({"jsonrpc": "2.0", "method": "RouterInfo", "params": {"Token": first}, "id": 1}),
            None,
            0,
        );
        assert_eq!(
            json_of(&outcome)["error"]["code"],
            serde_json::json!(-32_003)
        );
    }

    #[test]
    fn oversized_password_token_and_batch_rejected() {
        let state = test_state(TEST_PASSWORD);
        // Oversized password (+1) fails without minting.
        let big_password = "p".repeat(limits::MAX_PASSWORD_LEN + 1);
        let outcome = dispatch(
            &state,
            &serde_json::json!({"jsonrpc": "2.0", "method": "Authenticate", "params": {"API": 1, "Password": big_password}, "id": 1}),
            None,
            0,
        );
        assert_eq!(
            json_of(&outcome)["error"]["code"],
            serde_json::json!(-32_001)
        );
        assert_eq!(state.live_token_count(), 0);
        // Oversized presented token (+1) is an invalid token.
        let big_token = "t".repeat(MAX_PRESENTED_TOKEN_LEN + 1);
        let outcome = dispatch(
            &state,
            &serde_json::json!({"jsonrpc": "2.0", "method": "RouterInfo", "params": {"Token": big_token}, "id": 1}),
            None,
            0,
        );
        assert_eq!(
            json_of(&outcome)["error"]["code"],
            serde_json::json!(-32_003)
        );
        // Oversized batch (+1 element) is one body-level invalid request.
        let element = serde_json::json!({"jsonrpc": "2.0", "method": "Authenticate", "params": {"API": 1, "Password": TEST_PASSWORD}, "id": 1});
        let batch = serde_json::Value::Array(vec![element; MAX_BATCH_ELEMENTS + 1]);
        let outcome = dispatch(&state, &batch, None, 0);
        let response = json_of(&outcome);
        assert_eq!(response["error"]["code"], serde_json::json!(-32_600));
        assert_eq!(state.live_token_count(), 0);
    }

    #[test]
    fn json_envelope_errors_are_exact() {
        let state = test_state(TEST_PASSWORD);
        let loopback = IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1));
        // Malformed JSON is a parse error.
        let outcome = dispatch_raw(&state, b"{not json", None, loopback, 0);
        assert_eq!(
            json_of(&outcome)["error"]["code"],
            serde_json::json!(-32_700)
        );
        // Valid JSON that is not an object or array is an invalid request.
        let outcome = dispatch_raw(&state, b"42", None, loopback, 0);
        assert_eq!(
            json_of(&outcome)["error"]["code"],
            serde_json::json!(-32_600)
        );
        // Empty batch is one invalid-request response.
        let outcome = dispatch(&state, &serde_json::json!([]), None, 0);
        assert_eq!(
            json_of(&outcome)["error"]["code"],
            serde_json::json!(-32_600)
        );
        // Wrong JSON-RPC version is an invalid request.
        let outcome = dispatch(
            &state,
            &serde_json::json!({"jsonrpc": "1.0", "method": "Authenticate", "params": {"API": 1, "Password": TEST_PASSWORD}, "id": 1}),
            None,
            0,
        );
        assert_eq!(
            json_of(&outcome)["error"]["code"],
            serde_json::json!(-32_600)
        );
        // Positional params are rejected.
        let outcome = dispatch(
            &state,
            &serde_json::json!({"jsonrpc": "2.0", "method": "Authenticate", "params": [1, TEST_PASSWORD], "id": 1}),
            None,
            0,
        );
        assert_eq!(
            json_of(&outcome)["error"]["code"],
            serde_json::json!(-32_600)
        );
        // Unknown params keys are rejected on every method.
        let outcome = dispatch(
            &state,
            &serde_json::json!({"jsonrpc": "2.0", "method": "Authenticate", "params": {"API": 1, "Password": TEST_PASSWORD, "Extra": 1}, "id": 1}),
            None,
            0,
        );
        assert_eq!(
            json_of(&outcome)["error"]["code"],
            serde_json::json!(-32_602)
        );
    }

    #[test]
    fn notification_suppresses_response_but_executes() {
        let state = test_state(TEST_PASSWORD);
        // Notification Authenticate mints (side effect) with no response.
        let outcome = dispatch(
            &state,
            &serde_json::json!({"jsonrpc": "2.0", "method": "Authenticate", "params": {"API": 1, "Password": TEST_PASSWORD}}),
            None,
            0,
        );
        assert!(matches!(outcome.dispatch, HttpDispatch::NoContent));
        assert_eq!(state.live_token_count(), 1);
        // All-notification valid batch yields no response body.
        let outcome = dispatch(
            &state,
            &serde_json::json!([
                {"jsonrpc": "2.0", "method": "Authenticate", "params": {"API": 1, "Password": TEST_PASSWORD}},
                {"jsonrpc": "2.0", "method": "GetRate", "params": {}},
            ]),
            None,
            0,
        );
        assert!(matches!(outcome.dispatch, HttpDispatch::NoContent));
        // Explicit null id remains a request id with a null-id response.
        let outcome = dispatch(
            &state,
            &serde_json::json!({"jsonrpc": "2.0", "method": "GetRate", "params": {}, "id": null}),
            None,
            0,
        );
        let response = json_of(&outcome);
        assert_eq!(response["id"], serde_json::Value::Null);
        assert_eq!(response["error"]["code"], serde_json::json!(-32_601));
    }

    #[test]
    fn mixed_batches_preserve_order_and_isolate_invalid() {
        let state = test_state(TEST_PASSWORD);
        let token = authenticate(&state, 0);
        let outcome = dispatch(
            &state,
            &serde_json::json!([
                {"jsonrpc": "2.0", "method": "RouterInfo", "params": {"Token": token}, "id": "a"},
                {"jsonrpc": "2.0", "method": "Nope", "params": {}, "id": "b"},
                {"not": "a request"},
                {"jsonrpc": "2.0", "method": "RouterInfo", "params": {"Token": "bad"}, "id": "c"},
            ]),
            None,
            0,
        );
        let response = json_of(&outcome);
        let elements = response.as_array().expect("batch array");
        assert_eq!(elements.len(), 4);
        assert_eq!(elements[0]["id"], serde_json::json!("a"));
        // Plan 288: an empty RouterInfo selection succeeds with an empty
        // result object instead of the old dispatch-floor error.
        assert_eq!(elements[0]["result"], serde_json::json!({}));
        assert_eq!(elements[1]["id"], serde_json::json!("b"));
        assert_eq!(elements[1]["error"]["code"], serde_json::json!(-32_601));
        assert_eq!(elements[2]["error"]["code"], serde_json::json!(-32_600));
        assert_eq!(elements[3]["id"], serde_json::json!("c"));
        assert_eq!(elements[3]["error"]["code"], serde_json::json!(-32_003));
    }

    #[test]
    fn batch_authenticate_does_not_propagate_to_siblings() {
        let state = test_state(TEST_PASSWORD);
        // The sibling presents no token of its own: even though the first
        // element mints one, the mint stays invisible until batch end.
        let outcome = dispatch(
            &state,
            &serde_json::json!([
                {"jsonrpc": "2.0", "method": "Authenticate", "params": {"API": 1, "Password": TEST_PASSWORD}, "id": 1},
                {"jsonrpc": "2.0", "method": "RouterInfo", "params": {}, "id": 2},
            ]),
            None,
            0,
        );
        let response = json_of(&outcome);
        let elements = response.as_array().expect("batch array");
        assert_eq!(elements.len(), 2);
        assert!(elements[0]["result"]["Token"].is_string());
        assert_eq!(elements[1]["error"]["code"], serde_json::json!(-32_002));
        // The minted token works after the batch committed.
        let token = elements[0]["result"]["Token"]
            .as_str()
            .expect("token")
            .to_owned();
        let outcome = dispatch(
            &state,
            &serde_json::json!({"jsonrpc": "2.0", "method": "RouterInfo", "params": {"Token": token}, "id": 3}),
            None,
            0,
        );
        // Plan 288: the minted token authenticates an empty selection.
        assert_eq!(json_of(&outcome)["result"], serde_json::json!({}));
    }

    #[test]
    fn compatibility_header_token_is_accepted_and_must_agree() {
        let state = test_state(TEST_PASSWORD);
        let token = authenticate(&state, 0);
        // Header-only token authenticates an empty selection.
        let outcome = dispatch(
            &state,
            &serde_json::json!({"jsonrpc": "2.0", "method": "RouterInfo", "params": {}, "id": 1}),
            Some(&token),
            0,
        );
        assert_eq!(json_of(&outcome)["result"], serde_json::json!({}));
        // Agreeing duplicates authenticate.
        let outcome = dispatch(
            &state,
            &serde_json::json!({"jsonrpc": "2.0", "method": "RouterInfo", "params": {"Token": token}, "id": 1}),
            Some(&token),
            0,
        );
        assert_eq!(json_of(&outcome)["result"], serde_json::json!({}));
        // Disagreeing duplicates are invalid params.
        let outcome = dispatch(
            &state,
            &serde_json::json!({"jsonrpc": "2.0", "method": "RouterInfo", "params": {"Token": token}, "id": 1}),
            Some("different"),
            0,
        );
        assert_eq!(
            json_of(&outcome)["error"]["code"],
            serde_json::json!(-32_602)
        );
    }

    #[test]
    fn simultaneous_failed_auth_reserves_throttle_atomically() {
        use std::thread;
        let state = Arc::new(test_state(TEST_PASSWORD));
        let ip = IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1));
        let attempts = 24;
        let mut handles = Vec::new();
        for _ in 0..attempts {
            let state = Arc::clone(&state);
            handles.push(thread::spawn(move || {
                let body = serde_json::json!({
                    "jsonrpc": "2.0",
                    "method": "Authenticate",
                    "params": {"API": 1, "Password": "wrong"},
                    "id": 1,
                });
                let bytes = serde_json::to_vec(&body).expect("body");
                // Each worker drives the async chain on its own
                // current-thread runtime (no shared runtime needed).
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("test runtime")
                    .block_on(state.dispatch_body(&bytes, None, ip, 0))
            }));
        }
        for handle in handles {
            let outcome = handle.join().expect("worker joins");
            assert_eq!(
                json_of(&outcome)["error"]["code"],
                serde_json::json!(-32_001)
            );
        }
        // No reservation was lost: the atomic count names every attempt.
        let failures = state
            .throttle
            .lock()
            .expect("throttle")
            .failure_count(ip, 0);
        assert_eq!(failures, attempts);
        // The bounded delay saturates at the hard maximum.
        assert_eq!(
            AuthThrottle::delay_for(1_000_000),
            Duration::from_millis(THROTTLE_DELAY_MAX_MS)
        );
        assert_eq!(AuthThrottle::delay_for(4), Duration::ZERO);
    }

    #[test]
    fn secrets_never_appear_in_debug_or_errors() {
        let config = test_config(TEST_PASSWORD);
        let rendered = format!("{config:?}");
        assert!(
            !rendered.contains(TEST_PASSWORD),
            "config Debug leaks password"
        );
        // The service state intentionally implements no `Debug` at all, so
        // formatting it cannot compile: the snapshot below is the only
        // printable surface and carries counters only.
        let state = test_state(TEST_PASSWORD);
        let snapshot = state.snapshot();
        let rendered = format!("{snapshot:?}");
        assert!(!rendered.contains(TEST_PASSWORD), "snapshot leaks password");
        let outcome = dispatch(
            &state,
            &serde_json::json!({"jsonrpc": "2.0", "method": "Authenticate", "params": {"API": 1, "Password": TEST_PASSWORD}, "id": 1}),
            None,
            0,
        );
        let rendered = serde_json::to_string(&json_of(&outcome)).expect("renders");
        assert!(
            !rendered.contains(TEST_PASSWORD),
            "response echoes password"
        );
        let error = I2pControlServiceError::InvalidConfig("probe".to_owned());
        assert!(!format!("{error:?}").contains(TEST_PASSWORD));
    }

    #[test]
    fn disabled_config_allocates_nothing() {
        let text = "schema_version = 1\n[router]\ndata_dir = \"state\"\n";
        let config = Config::parse(text).expect("defaults parse");
        assert!(!config.i2pcontrol.enabled);
        assert!(I2pControlServiceState::new(config.i2pcontrol).is_err());
    }

    #[test]
    fn config_validation_fails_before_bind() {
        // Enabled with an empty password.
        let text =
            "schema_version = 1\n[router]\ndata_dir = \"state\"\n[i2pcontrol]\nenabled = true\n";
        assert!(Config::parse(text).is_err());
        // Non-loopback bind without explicit TLS material.
        let text = "schema_version = 1\n[router]\ndata_dir = \"state\"\n[i2pcontrol]\nenabled = true\nbind_address = \"0.0.0.0\"\npassword = \"secret\"\n";
        assert!(Config::parse(text).is_err());
        // Half-configured TLS material.
        let text = "schema_version = 1\n[router]\ndata_dir = \"state\"\n[i2pcontrol]\nenabled = true\npassword = \"secret\"\ncertificate = \"/tmp/cert.pem\"\n";
        assert!(Config::parse(text).is_err());
        // Invalid bind literal.
        let text = "schema_version = 1\n[router]\ndata_dir = \"state\"\n[i2pcontrol]\nbind_address = \"not-an-ip\"\n";
        assert!(Config::parse(text).is_err());
        // IPv6 loopback is explicitly supported.
        let text = "schema_version = 1\n[router]\ndata_dir = \"state\"\n[i2pcontrol]\nenabled = true\nbind_address = \"::1\"\npassword = \"secret\"\n";
        let config = Config::parse(text).expect("::1 parses");
        assert!(config.i2pcontrol.is_loopback_bind());
        assert!(!config.i2pcontrol.has_explicit_tls());
    }

    #[test]
    fn bad_explicit_tls_fails_before_side_effects() {
        let directory = tempfile::tempdir().expect("temp directory");
        let cert = directory.path().join("cert.pem");
        let key = directory.path().join("key.pem");
        std::fs::write(&cert, "not a certificate").expect("write cert");
        std::fs::write(&key, "not a key").expect("write key");
        let text = format!(
            "schema_version = 1\n[router]\ndata_dir = \"state\"\n[i2pcontrol]\nenabled = true\npassword = \"secret\"\ncertificate = {:?}\nprivate_key = {:?}\n",
            cert.to_string_lossy(),
            key.to_string_lossy()
        );
        let config = Config::parse(&text).expect("shape parses; material loads later");
        assert!(config.i2pcontrol.has_explicit_tls());
        assert!(I2pControlServiceState::new(config.i2pcontrol).is_err());
    }

    #[test]
    fn managed_tls_covers_loopback_identities() {
        let state = test_state(TEST_PASSWORD);
        let acceptor = tokio_rustls::TlsAcceptor::from(Arc::clone(&state.tls));
        let _ = acceptor;
        // Construction (managed cert generation) succeeds for loopback
        // without touching the filesystem: tested at the socket layer by
        // the black-box suite asserting a clean data directory.
    }

    #[test]
    fn plan294_addressbook_method_drives_the_canonical_owner() {
        use crate::addressbook::{AddressBookManager, AddressBookSubsystemConfig};
        let directory = tempfile::tempdir().expect("temp directory");
        let manager = Arc::new(AddressBookManager::activate(AddressBookSubsystemConfig {
            enabled: true,
            state_dir: directory.path().join("addressbook"),
        }));
        assert!(manager.is_active());
        let state = test_state(TEST_PASSWORD);
        state.set_addressbook_manager(Arc::clone(&manager));
        state.inspection().publish_addressbook(manager.shared());
        let token = authenticate(&state, 0);
        let call = |params: serde_json::Value| {
            json_of(&dispatch(
                &state,
                &serde_json::json!({"jsonrpc": "2.0", "method": "AddressBook", "params": params, "id": 1}),
                None,
                0,
            ))
        };
        let mut bytes = vec![0u8; 384];
        bytes.extend_from_slice(&[5u8, 0, 4, 0, 7, 0, 4]);
        let destination = i2pr_api::sam::base64::encode(&bytes);
        // Entry lifecycle with exact result shapes.
        let response = call(
            serde_json::json!({"Token": token, "Type": "private", "Hostname": "wire.i2p", "Destination": destination}),
        );
        assert_eq!(
            response["result"],
            serde_json::json!({"success": true, "message": "entry created"})
        );
        let response = call(
            serde_json::json!({"Token": token, "Type": "private", "Hostname": "wire.i2p", "Destination": destination}),
        );
        assert_eq!(
            response["result"]["message"],
            serde_json::json!("entry updated")
        );
        // Proposal getters are recognized as canonical fields, but their
        // exact list/object projections are owned by the next plan. No
        // normalized alias or partial response is emitted.
        let response = json_of(&dispatch(
            &state,
            &serde_json::json!({"jsonrpc": "2.0", "method": "RouterInfo", "params": {"Token": token, "i2p.router.addressbook.private.list": null, "i2p.router.addressbook.subscriptions": null, "i2p.router.addressbook.config": null}, "id": 2}),
            None,
            0,
        ));
        assert_eq!(response["error"]["code"], serde_json::json!(-32_603));
        assert!(
            response.get("result").is_none(),
            "no partial selector results"
        );
        // Delete presence selects deletion even with a false value.
        let response = call(
            serde_json::json!({"Token": token, "Type": "private", "Hostname": "wire.i2p", "Delete": false}),
        );
        assert_eq!(
            response["result"],
            serde_json::json!({"success": true, "message": "entry deleted"})
        );
        let response = json_of(&dispatch(
            &state,
            &serde_json::json!({"jsonrpc": "2.0", "method": "RouterInfo", "params": {"Token": token, "i2p.router.addressbook.private.list": null}, "id": 3}),
            None,
            0,
        ));
        assert_eq!(response["error"]["code"], serde_json::json!(-32_603));
        // Shape violations fail whole with no partial effect.
        for params in [
            serde_json::json!({"Token": token, "Type": "private", "Hostname": "gone.i2p", "Delete": true}),
            serde_json::json!({"Token": token, "Type": "private", "Hostname": "mix.i2p", "Destination": destination, "Delete": true}),
            serde_json::json!({"Token": token, "Type": "private", "Hostname": "mix.i2p", "SetSubscriptions": []}),
            serde_json::json!({"Token": token, "Type": "nope", "Hostname": "mix.i2p"}),
            serde_json::json!({"Token": token, "Type": "private"}),
            serde_json::json!({"Token": token, "Bogus": 1}),
            serde_json::json!({"Token": token}),
        ] {
            let response = call(params);
            assert!(
                response.get("error").is_some(),
                "shape must fail: {response}"
            );
        }
        // Unknown hostnames delete deterministically; values stay valid.
        let untouched = manager.revision().expect("revision");
        let response = call(
            serde_json::json!({"Token": token, "Type": "local", "Hostname": "ghost.i2p", "Destination": "nope"}),
        );
        assert!(response.get("error").is_some());
        assert_eq!(manager.revision(), Some(untouched));
        // Subscriptions and config replacements with exact messages.
        let response = call(
            serde_json::json!({"Token": token, "SetSubscriptions": ["http://example.i2p/hosts.txt"]}),
        );
        assert_eq!(
            response["result"],
            serde_json::json!({"success": true, "message": "subscriptions replaced"})
        );
        let response = call(
            serde_json::json!({"Token": token, "SetSubscriptions": ["http://example.i2p/hosts.txt"]}),
        );
        assert_eq!(
            response["result"]["message"],
            serde_json::json!("subscriptions unchanged")
        );
        let response =
            call(serde_json::json!({"Token": token, "SetConfig": {"theme": "midnight"}}));
        assert_eq!(
            response["result"],
            serde_json::json!({"success": true, "message": "config applied"})
        );
        let response =
            call(serde_json::json!({"Token": token, "SetConfig": {"theme": "midnight"}}));
        assert_eq!(
            response["result"]["message"],
            serde_json::json!("config unchanged")
        );
        let response = call(serde_json::json!({"Token": token, "SetConfig": {"theme": 7}}));
        assert!(response.get("error").is_some());
        for key in ["should_publish", "etags", "last_modified"] {
            let response = call(serde_json::json!({
                "Token": token,
                "SetConfig": {(key): "proposal-value"},
            }));
            assert_eq!(response["error"]["code"], serde_json::json!(-32_603));
            assert!(
                response["error"]["message"]
                    .as_str()
                    .unwrap_or_default()
                    .contains("Plan 321")
            );
        }
        // Error messages never echo request values.
        let response = call(
            serde_json::json!({"Token": token, "Type": "private", "Hostname": "secret-host.i2p", "Destination": "hunter2-material"}),
        );
        let rendered = serde_json::to_string(&response["error"]).expect("render");
        assert!(!rendered.contains("secret-host"));
        assert!(!rendered.contains("hunter2"));
    }

    #[test]
    fn plan294_addressbook_without_owner_fails_with_plan_marker() {
        let state = test_state(TEST_PASSWORD);
        let token = authenticate(&state, 0);
        // Standalone construction: the Plan 294 marker from the
        // existing typed floor is preserved.
        let response = json_of(&dispatch(
            &state,
            &serde_json::json!({"jsonrpc": "2.0", "method": "AddressBook", "params": {"Token": token}, "id": 1}),
            None,
            0,
        ));
        assert_eq!(response["error"]["code"], serde_json::json!(-32_603));
        assert!(
            response["error"]["message"]
                .as_str()
                .expect("message")
                .contains("Plan 294"),
            "missing Plan 294 marker"
        );
        // Installed but inactive (disabled subsystem): explicit
        // not-active failure, still no fabrication.
        let directory = tempfile::tempdir().expect("temp directory");
        let idle = Arc::new(crate::addressbook::AddressBookManager::activate(
            crate::addressbook::AddressBookSubsystemConfig {
                enabled: false,
                state_dir: directory.path().join("addressbook"),
            },
        ));
        state.set_addressbook_manager(idle);
        let response = json_of(&dispatch(
            &state,
            &serde_json::json!({"jsonrpc": "2.0", "method": "AddressBook", "params": {"Token": token, "Type": "private", "Hostname": "a.i2p"}, "id": 2}),
            None,
            0,
        ));
        assert_eq!(response["error"]["code"], serde_json::json!(-32_603));
        assert!(
            response["error"]["message"]
                .as_str()
                .expect("message")
                .contains("not active"),
            "missing not-active marker"
        );
    }
}
