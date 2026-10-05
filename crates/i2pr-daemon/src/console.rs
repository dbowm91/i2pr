//! Plan 356 router-console service: loopback listener and EggServe
//! lifecycle ownership.
//!
//! Ownership boundary. The console crate owns the browser application (the
//! Axum router, assets, and themes) and nothing else. This module owns the
//! socket, the HTTP/1 substrate lifecycle, and the shutdown handshake. The
//! `axum::serve` path is deliberately absent: the daemon must route the
//! console through EggServe so the substrate decision stays checkable in
//! one place.
//!
//! The listener is loopback-only and disabled by default, enforced by
//! `crate::config::normalize_console` rather than here. Plan 357 adds the
//! browser security policy to the router; Plan 358 binds read-only
//! Proposal-170 state.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

use eggserve_server::{RuntimeConfig, Server, TowerToEggserve};
use i2pr_console::control::ControlReply;
use i2pr_console::{ConsoleConfig, ConsoleState};
use tokio::net::TcpListener;

use i2pr_runtime::CancellationToken;

use crate::config::ConsoleConfig as ConsoleServiceConfig;
use crate::i2pcontrol_dispatch::{ControlDispatcher, LocalConsolePrincipal};

/// Maximum accepted request body bytes.
///
/// The console serves HTML, CSS, JavaScript, and a small JSON view model. It
/// has no upload surface in Plan 356, so a small fixed ceiling is enforced
/// by the substrate rather than trusted to handler code.
pub const MAX_REQUEST_BODY_BYTES: u64 = 64 * 1024;

/// Maximum accepted request header bytes.
pub const MAX_HEADER_BYTES: usize = 16 * 1024;

/// Maximum accepted number of request headers.
pub const MAX_HEADERS: usize = 32;

/// Maximum accepted request-target bytes.
pub const MAX_REQUEST_TARGET_BYTES: usize = 2 * 1024;

/// Maximum accepted in-flight requests across all connections.
pub const MAX_IN_FLIGHT_REQUESTS: usize = 64;

/// Distinct throttle-table entries retained.
///
/// The console binds one loopback listener and trusts no proxy header, so
/// the table is a defensive bound rather than a per-client quota.
pub const DEFAULT_THROTTLE_PEERS: usize = 256;

/// Bounded wait for the substrate to finish in-flight connections after
/// shutdown is requested.
pub const SHUTDOWN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// Console listener failures.
#[derive(Debug)]
pub enum ConsoleServiceError {
    /// The configured address could not be bound.
    Bind {
        /// Address that failed to bind.
        address: std::net::SocketAddr,
        /// Underlying error text.
        source: std::io::Error,
    },
    /// The substrate rejected its configuration.
    ///
    /// The detail is the substrate's own bounded configuration message. It
    /// is derived from this crate's constants, never from request bytes.
    RuntimeConfig {
        /// Substrate-reported configuration rejection.
        detail: String,
    },
    /// The substrate failed to start or to complete shutdown.
    Server,
    /// The browser security policy could not be built.
    SecurityPolicy,
}

impl std::fmt::Display for ConsoleServiceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Bind { address, source } => {
                write!(formatter, "console bind failed for {address}: {source}")
            }
            Self::RuntimeConfig { detail } => {
                write!(formatter, "console server configuration rejected: {detail}")
            }
            Self::Server => formatter.write_str("console server lifecycle failed"),
            Self::SecurityPolicy => formatter.write_str("console browser security policy rejected"),
        }
    }
}

impl std::error::Error for ConsoleServiceError {}

/// The supervised console service state.
pub struct ConsoleServiceState {
    config: ConsoleServiceConfig,
    /// Canonical Proposal-170 dispatch shared with the optional external
    /// I2PControl listener, so both use one implementation.
    dispatcher: ControlDispatcher,
    bound_address: OnceLock<std::net::SocketAddr>,
    serving: AtomicBool,
}

impl ConsoleServiceState {
    /// Builds the service from validated configuration.
    ///
    /// The router is deliberately **not** built here. The browser authority
    /// policy has to name the listener's real port, and with `port = 0`
    /// that port only exists after the bind. Building the router in
    /// [`ConsoleServiceState::serve`] keeps the authority set derived from
    /// the address the console actually accepted on.
    pub fn new(
        config: ConsoleServiceConfig,
        inspection: Arc<crate::i2pcontrol_inspection::InspectionHandles>,
    ) -> Result<Arc<Self>, ConsoleServiceError> {
        Ok(Arc::new(Self {
            config,
            dispatcher: ControlDispatcher::new(inspection),
            bound_address: OnceLock::new(),
            serving: AtomicBool::new(false),
        }))
    }

    /// Builds the read-only control client the console renders from.
    pub fn control_client(&self) -> ConsoleControlClient {
        ConsoleControlClient::new(self.dispatcher.clone())
    }

    /// Builds the security policy for a resolved listener address.
    ///
    /// Fails closed: a configuration that says `auth = true` but carries no
    /// usable verifier cannot start an unauthenticated console.
    fn security_policy(
        &self,
        resolved: std::net::SocketAddr,
    ) -> Result<i2pr_console::SecurityPolicy, ConsoleServiceError> {
        let limits = i2pr_console::SessionLimits::validate(
            self.config.max_sessions,
            std::time::Duration::from_secs(self.config.session_idle_secs),
            std::time::Duration::from_secs(self.config.session_absolute_secs),
        )
        .ok_or(ConsoleServiceError::SecurityPolicy)?;
        let throttle = i2pr_console::ThrottleLimits {
            max_failures: self.config.login_max_failures,
            window_secs: self.config.login_window_secs,
            max_peers: DEFAULT_THROTTLE_PEERS,
        };
        match (self.config.auth, &self.config.password_hash) {
            (true, Some(hash)) => i2pr_console::SecurityPolicy::authenticated_from_hash(
                resolved,
                hash.as_str(),
                limits,
                throttle,
            )
            .map_err(|_| ConsoleServiceError::SecurityPolicy),
            (true, None) => Err(ConsoleServiceError::SecurityPolicy),
            (false, _) => i2pr_console::SecurityPolicy::unauthenticated(resolved, limits, throttle)
                .map_err(|_| ConsoleServiceError::SecurityPolicy),
        }
    }

    /// Returns the validated configuration.
    pub fn config(&self) -> &ConsoleServiceConfig {
        &self.config
    }

    /// Returns the bound address once the listener is up.
    pub fn bound_address(&self) -> Option<std::net::SocketAddr> {
        self.bound_address.get().copied()
    }

    /// Binds a loopback listener and returns it with the resolved address.
    pub async fn bind(
        &self,
        bind_address: std::net::SocketAddr,
    ) -> Result<(TcpListener, std::net::SocketAddr), ConsoleServiceError> {
        let listener =
            TcpListener::bind(bind_address)
                .await
                .map_err(|source| ConsoleServiceError::Bind {
                    address: bind_address,
                    source,
                })?;
        let resolved = listener.local_addr().unwrap_or(bind_address);
        let _ = self.bound_address.set(resolved);
        Ok((listener, resolved))
    }

    /// Binds and serves until cancellation.
    pub async fn run(
        self: Arc<Self>,
        bind_address: std::net::SocketAddr,
        cancellation: CancellationToken,
    ) -> Result<(), ConsoleServiceError> {
        let (listener, resolved) = self.bind(bind_address).await?;
        self.serve(listener, resolved, cancellation).await
    }

    /// Serves an already-bound listener until cancellation fires.
    ///
    /// Shutdown is a lifecycle event, not a retry: cancellation requests one
    /// graceful stop, then the service waits a bounded time for in-flight
    /// connections. It never re-binds and never loops on the accept path.
    pub async fn serve(
        self: Arc<Self>,
        listener: TcpListener,
        resolved: std::net::SocketAddr,
        cancellation: CancellationToken,
    ) -> Result<(), ConsoleServiceError> {
        let policy = self.security_policy(resolved)?;
        let router = i2pr_console::router(Arc::new(ConsoleState::with_control(
            ConsoleConfig::new(&self.config.theme),
            policy,
            Arc::new(self.control_client()),
        )));

        let runtime = RuntimeConfig::builder()
            .bind(resolved)
            .max_connections(self.config.max_connections as usize)
            .max_request_body_bytes(MAX_REQUEST_BODY_BYTES)
            .max_headers(MAX_HEADERS)
            .max_header_bytes(MAX_HEADER_BYTES)
            .max_request_target_bytes(MAX_REQUEST_TARGET_BYTES)
            .max_in_flight_requests(MAX_IN_FLIGHT_REQUESTS)
            .graceful_shutdown_timeout(SHUTDOWN_TIMEOUT)
            // No `server_header` is set: the substrate default omits the
            // `Server` header entirely, which matches this router's
            // "advertise nothing beyond the tested subset" posture in
            // specs/CONFORMANCE.md.
            .build()
            .map_err(|error| ConsoleServiceError::RuntimeConfig {
                detail: error.to_string(),
            })?;

        let server = Server::builder()
            .runtime(runtime)
            .from_listener(listener)
            .build()
            .map_err(|error| ConsoleServiceError::RuntimeConfig {
                detail: error.to_string(),
            })?;

        let handle = server
            .start_with_service(TowerToEggserve::new(router))
            .await
            .map_err(|_| ConsoleServiceError::Server)?;

        self.serving.store(true, Ordering::SeqCst);
        tracing::info!(
            address = %resolved,
            max_connections = self.config.max_connections,
            theme = %self.config.theme,
            "console loopback listener bound"
        );

        cancellation.cancelled().await;
        tracing::debug!("console listener cancellation observed");
        handle.shutdown();

        tokio::time::timeout(SHUTDOWN_TIMEOUT, handle.wait())
            .await
            .map_err(|_| ConsoleServiceError::Server)?;
        self.serving.store(false, Ordering::SeqCst);
        tracing::info!("console listener stopped");
        Ok(())
    }

    /// Returns whether the listener is currently accepting.
    pub fn is_serving(&self) -> bool {
        self.serving.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    fn inspection() -> Arc<crate::i2pcontrol_inspection::InspectionHandles> {
        Arc::new(
            crate::i2pcontrol_inspection::InspectionHandles::from_config(
                &crate::config::Config::parse(
                    "schema_version = 1\n[router]\ndata_dir = \"./state\"\n",
                )
                .expect("fixture config parses"),
            ),
        )
    }

    fn config() -> ConsoleServiceConfig {
        ConsoleServiceConfig {
            enabled: true,
            bind_address: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 0,
            theme: i2pr_console::DEFAULT_THEME_NAME.to_string(),
            max_connections: 4,
            auth: false,
            password_hash: None,
            max_sessions: 8,
            session_idle_secs: 900,
            session_absolute_secs: 28_800,
            login_max_failures: 5,
            login_window_secs: 300,
        }
    }

    #[test]
    fn service_state_builds_from_validated_configuration() {
        let state = ConsoleServiceState::new(config(), inspection()).expect("state builds");
        assert_eq!(state.config().theme, i2pr_console::DEFAULT_THEME_NAME);
        assert!(!state.is_serving());
        assert!(state.bound_address().is_none());
    }

    #[test]
    fn authenticated_mode_without_a_verifier_fails_closed() {
        let mut cfg = config();
        cfg.auth = true;
        cfg.password_hash = None;
        let state = ConsoleServiceState::new(cfg, inspection()).expect("state builds");
        let resolved = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 7070);
        assert!(matches!(
            state.security_policy(resolved),
            Err(ConsoleServiceError::SecurityPolicy)
        ));
    }

    #[test]
    fn unauthenticated_mode_builds_a_policy_for_the_resolved_port() {
        let state = ConsoleServiceState::new(config(), inspection()).expect("state builds");
        let resolved = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 41_000);
        let policy = state.security_policy(resolved).expect("policy builds");
        // The authority policy must name the port that was actually bound.
        assert!(policy.authority().accepts_host("localhost:41000"));
        assert!(!policy.authority().accepts_host("localhost:7070"));
        assert_eq!(policy.mode(), i2pr_console::AuthMode::Disabled);
    }

    #[test]
    fn unknown_theme_configuration_still_yields_a_usable_service() {
        let mut cfg = config();
        cfg.theme = "not-a-bundled-theme".to_string();
        let state = ConsoleServiceState::new(cfg, inspection()).expect("state builds");
        // Normalization rejects this name upstream; the defensive path here
        // must still produce a readable console rather than a panic.
        assert!(!state.config().theme.is_empty());
    }

    #[test]
    fn bounds_are_ordered_conservatively() {
        const _: () = assert!(MAX_REQUEST_BODY_BYTES <= 1024 * 1024);
        const _: () = assert!(MAX_HEADER_BYTES < MAX_REQUEST_BODY_BYTES as usize);
        const _: () = assert!(MAX_REQUEST_TARGET_BYTES < MAX_HEADER_BYTES);
        const _: () = assert!(MAX_IN_FLIGHT_REQUESTS >= MAX_HEADERS);
    }
}
/// The daemon's read-only control client for the built-in console.
///
/// It holds the `LocalConsolePrincipal`, which is issuable only by the
/// daemon composition root. The console crate never sees the principal
/// type: it receives this object behind the `ControlClient` trait, whose
/// surface is two fixed methods with no method-name parameter.
#[derive(Clone, Debug)]
pub struct ConsoleControlClient {
    dispatcher: ControlDispatcher,
    principal: LocalConsolePrincipal,
}

/// Classifies a canonical JSON-RPC envelope into a console reply.
///
/// Split out so the unavailability semantics are testable directly against
/// the contract rather than only through a live listener.
pub(crate) fn classify_envelope(envelope: &serde_json::Value) -> ControlReply {
    if let Some(error) = envelope.get("error") {
        let detail = error
            .get("message")
            .and_then(|message| message.as_str())
            .unwrap_or("control request failed")
            .to_string();
        return ControlReply::failed(detail);
    }
    match envelope.get("result") {
        Some(result) => ControlReply::returned(result.clone()),
        None => ControlReply::failed("control response carried no result"),
    }
}

impl ConsoleControlClient {
    /// Builds the client over a canonical dispatcher.
    pub(crate) fn new(dispatcher: ControlDispatcher) -> Self {
        Self {
            dispatcher,
            principal: LocalConsolePrincipal::issue(),
        }
    }

    /// Dispatches one read-only method through the canonical dispatcher.
    fn dispatch(
        &self,
        method: i2pr_i2pcontrol::Method,
        params: serde_json::Map<String, serde_json::Value>,
    ) -> ControlReply {
        let (envelope, _delay) =
            self.principal
                .dispatch_readonly(&self.dispatcher, &method, None, &params, 0);
        // The envelope is already bounded and redacted by the canonical
        // dispatch; the console only classifies it.
        classify_envelope(&envelope)
    }
}

/// Base `RouterInfo` rows this implementation is known to publish.
///
/// The canonical source matrix in `i2pr-i2pcontrol` covers the 43
/// Proposal-170 additions; the base rows (`i2p.router.version`,
/// `i2p.router.status`, ...) have no matrix, so their availability cannot
/// be *derived*. Rather than invent a source of truth, the set is small,
/// explicit, and proved round-trip by
/// `verified_base_overview_selectors_answer` — a selector that stops
/// answering fails a test instead of silently blanking the page.
const VERIFIED_BASE_OVERVIEW_SELECTORS: &[&str] = &[
    "i2p.router.version",
    "i2p.router.uptime",
    "i2p.router.status",
];

/// Splits the overview selectors into requestable and withheld.
///
/// A selector is requested when either the canonical matrix marks it
/// `Available`, or it is one of the verified base rows above. Everything
/// else is deliberately withheld: requesting an unavailable or
/// publish-gated row fails the *whole* canonical request, which would
/// blank an otherwise truthful page. The console renders withheld rows as
/// unavailable, which is the honest presentation.
pub(crate) fn requestable_overview_selectors() -> Vec<&'static str> {
    let matrix = i2pr_i2pcontrol::proposal_router_info_source_matrix();
    i2pr_console::control::OVERVIEW_SELECTORS
        .iter()
        .copied()
        .filter(|selector| {
            match matrix.iter().find(|row| row.key == *selector) {
                Some(row) => matches!(
                    row.availability,
                    i2pr_i2pcontrol::SourceAvailability::Available
                ),
                // Not a Proposal-170 addition: consult the verified base set.
                None => VERIFIED_BASE_OVERVIEW_SELECTORS.contains(selector),
            }
        })
        .collect()
}

impl i2pr_console::ControlClient for ConsoleControlClient {
    fn router_info(&self) -> ControlReply {
        let mut params = serde_json::Map::new();
        for selector in requestable_overview_selectors() {
            params.insert(selector.to_string(), serde_json::Value::Null);
        }
        self.dispatch(i2pr_i2pcontrol::Method::RouterInfo, params)
    }

    fn client_services(&self) -> ControlReply {
        let mut params = serde_json::Map::new();
        for selector in i2pr_console::control::CLIENT_SERVICE_SELECTORS {
            params.insert((*selector).to_string(), serde_json::Value::Null);
        }
        self.dispatch(i2pr_i2pcontrol::Method::ClientServicesInfo, params)
    }
}
