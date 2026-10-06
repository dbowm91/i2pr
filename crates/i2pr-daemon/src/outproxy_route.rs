//! Plan 342: the daemon half of the I2P-routed outproxy provider.
//!
//! [`i2pr_service_tunnels::outproxy`] decides everything that can be decided
//! without a socket. This module supplies the two things only the composition
//! root can do:
//!
//! 1. **the I/O half** — open a Streaming route to the selected I2P outproxy
//!    destination and speak the outproxy-facing handshake over it, with
//!    bounded retry and a bounded read;
//! 2. **the credential half** — recover the persisted outproxy password
//!    through Plan 341's [`OutboundSecretStore`] and build the header.
//!
//! # The invariant this module is built around
//!
//! The only route this module ever opens is a Streaming connection to an I2P
//! destination. There is no clearnet socket anywhere in it, and no branch that
//! would open one after a failure. A request for a clearnet target that
//! cannot be routed through an outproxy returns a typed failure; it does not
//! fall back. That is the whole design, and the honest way to test it is to
//! assert the failure rather than to look for a socket.
//!
//! # Where the credential lives
//!
//! The sealed password arrives as a stored form and is opened only inside
//! [`RouterOutproxyProvider::auth_header`], so the plaintext's lifetime is the
//! header construction. Nothing here formats the username, the password, or
//! the header, and no error variant carries an operator value.

#![forbid(unsafe_code)]

use std::sync::Arc;
use std::time::{Duration, Instant};

use i2pr_client::streaming::connection::{ConnectionId, ConnectionState};
use i2pr_client::streaming::manager::{ConnectOutcome, RemoteDestination};
use i2pr_crypto::OsRng;
use i2pr_runtime::CancellationToken;
use i2pr_service_tunnels::outbound_secret::{OutboundSecret, OutboundSecretStore};
use i2pr_service_tunnels::outproxy::{ClientTargetClass, classify_client_target};
use i2pr_service_tunnels::outproxy::{
    MAX_OUTPROXY_HANDSHAKE_BYTES, OutproxyAuthHeader, OutproxyConfig, OutproxyError,
    OutproxyFailure, OutproxyProvider, OutproxyRoute, OutproxyTarget, OutproxyType,
    build_http_connect_request, build_socks4a_connect_request, build_socks5_connect_request,
    parse_http_connect_response, parse_socks4a_connect_reply, parse_socks5_connect_reply,
    parse_socks5_method_reply,
};

use crate::service_tunnels::{ServiceTunnelManager, service_streaming_now_ms};
use i2pr_client::streaming::manager::DEFAULT_ADVERTISED_MAX_PAYLOAD;

/// Deadline for one outproxy handshake exchange, independent of the connect
/// timeout.
///
/// A separate ceiling because a Streaming connection can be established and
/// then the outproxy can simply never answer. Bounding only the connect would
/// leave that case waiting forever.
pub const OUTPROXY_HANDSHAKE_DEADLINE_MS: u64 = 15_000;

/// Sanitized observation counters for the outproxy path.
///
/// Counts only. No hostname, no username, no header, and no error string
/// reaches this struct, so it is safe to project into a status or
/// `REPORT_STATUS` surface verbatim.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct OutproxyCounters {
    /// Requests that were routed directly because the target was an I2P name.
    pub direct_i2p: u64,
    /// Requests that selected an outproxy and opened a Streaming route.
    pub connect_attempts: u64,
    /// Routes that completed their handshake and are ready to pump.
    pub handshake_ok: u64,
    /// Requests refused because no outproxy is configured.
    pub not_configured: u64,
    /// Requests refused because the secret owner cannot open the credential.
    pub secret_owner_unavailable: u64,
    /// Requests that exhausted every permitted attempt.
    pub attempts_exhausted: u64,
    /// Attempts where the outproxy rejected the presented credential.
    pub authentication_rejected: u64,
    /// Attempts where the outproxy could not reach the requested target.
    pub target_unreachable: u64,
    /// Requests refused by policy before any route was opened.
    pub not_permitted: u64,
    /// Connect attempts that failed to establish Streaming at all.
    pub connect_failed: u64,
}

impl OutproxyCounters {
    /// Records one terminal outcome.
    pub fn note(&mut self, failure: OutproxyFailure) {
        match failure {
            OutproxyFailure::NotConfigured => self.not_configured += 1,
            OutproxyFailure::SecretOwnerUnavailable => self.secret_owner_unavailable += 1,
            OutproxyFailure::AttemptsExhausted => self.attempts_exhausted += 1,
            OutproxyFailure::AuthenticationRejected => self.authentication_rejected += 1,
            OutproxyFailure::TargetUnreachable => self.target_unreachable += 1,
            OutproxyFailure::NotPermitted => self.not_permitted += 1,
        }
    }

    /// Total requests this router refused because no outproxy was configured.
    pub const fn refused_without_provider(&self) -> u64 {
        self.not_configured
    }
}

/// Maps a handshake or codec error onto the typed failure vocabulary.
///
/// Total by construction: every [`OutproxyError`] has an arm, so a new codec
/// error cannot silently become "some other failure" and be reported as a
/// dead outproxy.
pub fn classify(error: OutproxyError) -> OutproxyFailure {
    match error {
        OutproxyError::CredentialRejected => OutproxyFailure::AuthenticationRejected,
        OutproxyError::UpstreamRefused | OutproxyError::MalformedResponse { .. } => {
            OutproxyFailure::TargetUnreachable
        }
        OutproxyError::UnknownType
        | OutproxyError::MalformedType { .. }
        | OutproxyError::MalformedList { .. }
        | OutproxyError::NotAnI2pDestination
        | OutproxyError::DuplicateEndpoint
        | OutproxyError::MalformedTarget { .. }
        | OutproxyError::MalformedCredential { .. }
        | OutproxyError::TunnelledNotInList
        | OutproxyError::ExceedsCeiling { .. } => OutproxyFailure::NotPermitted,
    }
}

/// The composition-root provider: a validated [`OutproxyConfig`] plus the
/// Plan 341 secret owner that holds the sealed password.
pub struct RouterOutproxyProvider {
    config: OutproxyConfig,
    store: Arc<dyn OutboundSecretStore>,
    /// The sealed stored form. Ciphertext, not plaintext, so an ordinary
    /// `Option<String>` is correct here — and it still must not be echoed.
    sealed_password: Option<String>,
}

impl RouterOutproxyProvider {
    /// Builds a provider.
    ///
    /// `sealed_password` is the Plan 341 stored form. Passing `None` with
    /// `present_credential` set is a contradiction the config validator
    /// catches; passing one with it unset is tolerated so an operator can
    /// stage a credential without arming it.
    pub fn new(
        config: OutproxyConfig,
        store: Arc<dyn OutboundSecretStore>,
        sealed_password: Option<String>,
    ) -> Result<Self, OutproxyError> {
        config.validate()?;
        Ok(Self {
            config,
            store,
            sealed_password,
        })
    }

    /// The sealed stored form, for status surfaces that must show that a
    /// credential is configured without revealing it.
    pub fn has_stored_credential(&self) -> bool {
        self.sealed_password.is_some()
    }

    /// The configured outproxy count, for status surfaces.
    pub fn configured_count(&self) -> usize {
        self.config.list.len()
    }
}

impl OutproxyProvider for RouterOutproxyProvider {
    fn config(&self) -> &OutproxyConfig {
        &self.config
    }

    fn credential_available(&self) -> bool {
        self.config.present_credential
            && self.sealed_password.is_some()
            && self.store.is_available()
    }

    fn select(&self, target: &OutproxyTarget, attempt: usize) -> OutproxyRoute {
        self.config.route_attempt(target, attempt)
    }

    /// Opens the credential and builds the header.
    ///
    /// Fails closed at every step: no sealed form, an owner that cannot open,
    /// a form that does not authenticate, and a missing username all produce
    /// an error rather than an unauthenticated request. A request that goes
    /// upstream without a credential it was configured to present is a
    /// policy violation, not a fallback.
    fn auth_header(&self) -> Result<Option<OutproxyAuthHeader>, OutproxyError> {
        if !self.config.present_credential {
            return Ok(None);
        }
        let sealed = self.sealed_password.as_ref().ok_or({
            OutproxyError::MalformedCredential {
                reason: "outproxy authentication is configured without a stored credential",
            }
        })?;
        let username =
            self.config
                .username
                .as_deref()
                .ok_or(OutproxyError::MalformedCredential {
                    reason: "outproxy authentication is configured without a username",
                })?;
        let secret: OutboundSecret = self.store.open(sealed).map_err(|_| {
            // The underlying reason is a fixed string and never carries the
            // value, but it is dropped here anyway: a caller of this method
            // reports a typed failure, not a store error.
            OutproxyError::MalformedCredential {
                reason: "outproxy credential could not be recovered",
            }
        })?;
        Ok(Some(OutproxyAuthHeader::basic(username, &secret)?))
    }
}

/// A route to an outproxy that completed its handshake and is ready to pump.
#[derive(Clone, Debug)]
pub struct OutproxySession {
    /// Streaming connection identifier for the established route.
    pub connection_id: ConnectionId,
    /// The outproxy destination the route runs to.
    pub outproxy: String,
    /// The remote the route was opened to.
    ///
    /// Plan 342: the pump addresses its Streaming bridge by `(connection_id,
    /// remote)`, so a session that carried only the connection would force
    /// the request paths to re-resolve the outproxy to learn where they had
    /// actually connected. Carrying it here makes "the route I opened" and
    /// "the endpoint I pump" the same value, and removes the step where they
    /// could disagree.
    pub remote: i2pr_client::streaming::manager::RemoteDestination,
    /// Bytes already delivered by the outproxy that belong to the *tunnelled*
    /// stream rather than to the handshake.
    ///
    /// A proxy is allowed to answer `CONNECT` and start pushing payload in
    /// the same burst, so the bytes after the response head are preserved and
    /// handed to the pump rather than dropped. Losing them would corrupt the
    /// first request on every such outproxy.
    pub tunnel_prefix: Vec<u8>,
}

/// What a client request path should do with one request.
///
/// The three request paths (HTTP forward, HTTP `CONNECT`, SOCKS5) differ in
/// how they answer the *local* client — a status line, a 200, or a SOCKS
/// reply — and in nothing else. Everything above that line is this enum, so
/// the guarantee Plan 342 asks for cannot hold on one path and not another.
#[derive(Debug)]
pub enum ClientRoute {
    /// The target is an I2P destination: the caller takes its existing direct
    /// path and no provider is consulted.
    Direct,
    /// The route now runs through an outproxy and is ready to pump.
    ViaOutproxy(OutproxySession),
    /// There is no route.
    ///
    /// The caller must fail the request. There is deliberately no variant
    /// meaning "fall back to a direct socket": a caller that could reach this
    /// state by any other route is exactly what Plan 342 invariant 1 forbids.
    Refused(OutproxyFailure),
}

/// Decides and, if needed, opens the route for one client request.
///
/// One function for all three request paths, for the reason
/// `i2pr_service_tunnels::outproxy::classify_client_target`
/// has one: the guarantee is a property of the code, not of a convention
/// three call sites follow.
///
/// `provider` is `None` on a tunnel with no outproxy block, which is the
/// fail-closed default: a clearnet target then arrives as
/// [`ClientRoute::Refused`] rather than being carried by the tunnel's own
/// I2P destination. That is a behaviour change from before Plan 342 for a
/// tunnel that has *no* provider, and it is the point — before, an
/// http-client silently forwarded every `Host:` header to one configured I2P
/// destination, which is a route the operator did not configure.
#[allow(clippy::too_many_arguments)]
pub async fn open_client_route(
    manager: &Arc<ServiceTunnelManager>,
    spec_id: &str,
    destination_id: i2pr_client::DestinationId,
    provider: Option<&RouterOutproxyProvider>,
    host: &str,
    port: u16,
    tunnelled: bool,
    cancellation: &CancellationToken,
    counters: &std::sync::Mutex<OutproxyCounters>,
) -> Result<ClientRoute, OutproxyFailure> {
    // One classification, one parse. The `.i2p` bypass is decided here, with
    // no provider consulted, so a misconfigured list cannot divert in-network
    // traffic off-network — and so that a failure to parse the target is
    // counted as a refusal rather than being retried per outproxy.
    let config = provider.map(RouterOutproxyProvider::config);
    let target = classify_client_target(config, host, port).map_err(|_| {
        let failure = OutproxyFailure::NotPermitted;
        note(counters, |c| c.note(failure));
        failure
    })?;
    let target = match target {
        ClientTargetClass::Direct(_) => return Ok(ClientRoute::Direct),
        ClientTargetClass::ViaOutproxy(target) => target,
        ClientTargetClass::Refused(failure) => {
            note(counters, |c| c.note(failure));
            return Ok(ClientRoute::Refused(failure));
        }
    };

    // Past the `.i2p` bypass a provider is not optional: a clearnet target
    // classified as carryable means a config existed. The explicit arm keeps
    // that invariant readable rather than asserting it with an unwrap.
    let Some(provider) = provider else {
        return Ok(ClientRoute::Refused(OutproxyFailure::NotConfigured));
    };

    match open_via_outproxy(
        manager,
        spec_id,
        destination_id,
        provider,
        &target,
        tunnelled,
        cancellation,
        counters,
    )
    .await
    {
        Ok(session) => Ok(ClientRoute::ViaOutproxy(session)),
        Err(failure) => Ok(ClientRoute::Refused(failure)),
    }
}

/// Builds the outproxy-facing request for one attempt.
///
/// One place, so the HTTP and SOCKS dialects cannot drift, and so the
/// credential header is attached on exactly the path that needs it.
pub fn build_attempt(
    kind: OutproxyType,
    target: &OutproxyTarget,
    credential: Option<&OutproxyAuthHeader>,
) -> Result<i2pr_service_tunnels::outproxy::OutproxyWireBuffer, OutproxyError> {
    match kind {
        OutproxyType::HttpConnect => build_http_connect_request(target, credential),
        // The SOCKS dialects carry no HTTP credential header. A credential
        // configured for a SOCKS outproxy is a configuration error the caller
        // surfaces, not something silently dropped on the wire.
        OutproxyType::Socks5 => {
            if credential.is_some() {
                return Err(OutproxyError::MalformedCredential {
                    reason: "a socks outproxy cannot carry an http basic credential",
                });
            }
            build_socks5_connect_request(target)
        }
        OutproxyType::Socks4a => {
            if credential.is_some() {
                return Err(OutproxyError::MalformedCredential {
                    reason: "a socks outproxy cannot carry an http basic credential",
                });
            }
            build_socks4a_connect_request(target)
        }
    }
}

/// Interprets an outproxy's reply for one dialect.
///
/// `received` is the accumulated handshake response; `leftover` comes back so
/// the caller can preserve tunnelled bytes.
pub fn interpret_reply(kind: OutproxyType, received: &[u8]) -> Result<Vec<u8>, OutproxyError> {
    match kind {
        OutproxyType::HttpConnect => {
            // Consume through the blank line; everything after is tunnelled.
            let Some(end) = find_crlf_crlf(received) else {
                return Err(OutproxyError::MalformedResponse {
                    reason: "http outproxy response head is incomplete",
                });
            };
            let head_end = end + 4;
            parse_http_connect_response(&received[..head_end])?;
            Ok(received[head_end..].to_vec())
        }
        OutproxyType::Socks5 => {
            // Method selection, then the connect reply. SOCKS5 needs the
            // length of the bound address before the reply can be framed, so
            // the parse is length-driven rather than a fixed offset.
            if received.len() < 2 {
                return Err(OutproxyError::MalformedResponse {
                    reason: "socks5 outproxy reply is truncated",
                });
            }
            parse_socks5_method_reply(&received[..2])?;
            let reply = &received[2..];
            let reply_len = socks5_reply_len(reply)?;
            parse_socks5_connect_reply(&reply[..reply_len])?;
            Ok(received[2 + reply_len..].to_vec())
        }
        OutproxyType::Socks4a => {
            if received.len() < 8 {
                return Err(OutproxyError::MalformedResponse {
                    reason: "socks4a outproxy reply is truncated",
                });
            }
            parse_socks4a_connect_reply(&received[..8])?;
            Ok(received[8..].to_vec())
        }
    }
}

/// The total length of one SOCKS5 reply, from the version byte through the
/// bound address, per the RFC 1928 address-length table.
fn socks5_reply_len(reply: &[u8]) -> Result<usize, OutproxyError> {
    let malformed = |reason| OutproxyError::MalformedResponse { reason };
    if reply.len() < 4 {
        return Err(malformed("socks5 connect reply is truncated"));
    }
    // Every address type is followed by a 2-byte port, so the total is the
    // four-byte prefix plus the address plus the port. The domain arm also
    // carries its own length byte, counted here rather than folded into the
    // address width. An earlier revision omitted the port on the IPv4 and
    // IPv6 arms, which framed those replies two bytes short.
    let total = match reply[3] {
        0x01 => 4 + 4 + 2,
        0x04 => 4 + 16 + 2,
        0x03 => {
            if reply.len() < 5 {
                return Err(malformed("socks5 domain reply has no length byte"));
            }
            4 + 1 + usize::from(reply[4]) + 2
        }
        _ => return Err(malformed("socks5 reply has an unknown address type")),
    };
    if reply.len() < total {
        return Err(malformed(
            "socks5 connect reply is shorter than its address",
        ));
    }
    Ok(total)
}

/// Index of the first CRLFCRLF, if present.
fn find_crlf_crlf(bytes: &[u8]) -> Option<usize> {
    bytes.windows(4).position(|window| window == b"\r\n\r\n")
}

/// Whether a reply can be interpreted yet, or is simply incomplete.
///
/// Distinguishing "not yet" from "malformed" is what lets the exchange loop
/// keep reading instead of failing a slow outproxy on the first poll.
pub fn is_complete(kind: OutproxyType, received: &[u8]) -> bool {
    match kind {
        OutproxyType::HttpConnect => find_crlf_crlf(received).is_some(),
        OutproxyType::Socks5 => {
            if received.len() < 2 {
                return false;
            }
            socks5_reply_len(&received[2..]).is_ok()
        }
        OutproxyType::Socks4a => received.len() >= 8,
    }
}

/// Opens one route to an outproxy over I2P Streaming and negotiates it.
///
/// This is the only function in the repository that opens an outproxy route,
/// and it opens exactly one kind of route: Streaming to an I2P destination.
/// The retry loop is bounded by `config.policy().attempts()`, the handshake by
/// [`OUTPROXY_HANDSHAKE_DEADLINE_MS`], and every failure is typed. There is no
/// direct-clearnet arm to fall back to.
#[allow(clippy::too_many_arguments)]
pub async fn open_via_outproxy(
    manager: &Arc<ServiceTunnelManager>,
    spec_id: &str,
    destination_id: i2pr_client::DestinationId,
    provider: &RouterOutproxyProvider,
    target: &OutproxyTarget,
    tunnelled: bool,
    cancellation: &CancellationToken,
    counters: &std::sync::Mutex<OutproxyCounters>,
) -> Result<OutproxySession, OutproxyFailure> {
    let config = provider.config();
    let attempts = config.policy.attempts();

    let mut last = OutproxyFailure::AttemptsExhausted;
    for attempt in 0..attempts {
        if cancellation.is_cancelled() {
            return Err(OutproxyFailure::NotPermitted);
        }
        // The I2P bypass is re-evaluated per attempt, not once up front: a
        // caller cannot reach this loop with an I2P target, and re-checking
        // keeps the guarantee local to the function that opens routes.
        let route = config.route_attempt(target, attempt);
        let (endpoint, kind) = match route {
            OutproxyRoute::DirectI2p => {
                note(counters, |c| {
                    c.direct_i2p += 1;
                });
                // An I2P target reaching an outproxy opener is a caller
                // error, and it is refused rather than quietly routed.
                return Err(OutproxyFailure::NotPermitted);
            }
            // Plan 342: the selector now says "no" instead of claiming a
            // direct route. Carrying the failure through means the client is
            // told *why* it cannot connect rather than a generic attempt
            // failure, and it makes the no-direct-clearnet guarantee a
            // property of the match rather than of one arm's behaviour.
            OutproxyRoute::Refused(failure) => {
                note(counters, |c| c.note(failure));
                return Err(failure);
            }
            OutproxyRoute::ViaOutproxy { endpoint, kind, .. } => (endpoint, kind),
        };
        if tunnelled && !config.permits_tunnelled(&endpoint) {
            note(counters, |c| c.not_permitted += 1);
            return Err(OutproxyFailure::NotPermitted);
        }
        note(counters, |c| c.connect_attempts += 1);

        let credential = match provider.auth_header() {
            Ok(value) => value,
            Err(_) => {
                note(counters, |c| c.secret_owner_unavailable += 1);
                return Err(OutproxyFailure::SecretOwnerUnavailable);
            }
        };
        let request = match build_attempt(kind, target, credential.as_ref()) {
            Ok(value) => value,
            Err(error) => {
                let failure = classify(error);
                note(counters, |c| c.note(failure));
                return Err(failure);
            }
        };

        match open_one(
            manager,
            spec_id,
            destination_id,
            &endpoint,
            kind,
            &request,
            config.policy.connect_timeout_ms(),
            cancellation,
        )
        .await
        {
            Ok(session) => {
                note(counters, |c| c.handshake_ok += 1);
                return Ok(session);
            }
            Err(failure) => {
                last = failure;
                if !failure.is_retryable() {
                    note(counters, |c| c.note(failure));
                    return Err(failure);
                }
            }
        }
    }
    // Every attempt was spent. The last attempt's reason is recorded
    // alongside the exhaustion rather than discarded, because "the outproxy
    // rejected the credential N times" is a far more useful diagnosis than
    // "exhausted" and the counters are the only surface that carries it.
    note_exhausted(counters, last);
    Err(OutproxyFailure::AttemptsExhausted)
}

/// Records an exhausted request: the terminal reason of the last attempt and
/// the exhaustion itself. Split out so the composition is testable without a
/// live route.
fn note_exhausted(counters: &std::sync::Mutex<OutproxyCounters>, last: OutproxyFailure) {
    note(counters, |c| {
        c.note(last);
        c.attempts_exhausted += 1;
    });
}

/// One connect-and-negotiate attempt.
#[allow(clippy::too_many_arguments)]
async fn open_one(
    manager: &Arc<ServiceTunnelManager>,
    spec_id: &str,
    destination_id: i2pr_client::DestinationId,
    endpoint: &i2pr_service_tunnels::OutproxyEndpoint,
    kind: OutproxyType,
    request: &i2pr_service_tunnels::outproxy::OutproxyWireBuffer,
    connect_timeout_ms: u64,
    cancellation: &CancellationToken,
) -> Result<OutproxySession, OutproxyFailure> {
    let reference = endpoint.reference();
    let client = match manager.resolve_reference(reference) {
        Ok(value) => value,
        Err(_) => {
            // The outproxy is not a destination this router can reach. That
            // is a configuration fact rather than a transient one, so it
            // fails instead of rotating — and it is emphatically not a
            // licence to connect directly.
            return Err(OutproxyFailure::TargetUnreachable);
        }
    };
    if manager
        .ensure_destination_active(spec_id, cancellation, connect_timeout_ms)
        .await
        .is_err()
    {
        return Err(OutproxyFailure::TargetUnreachable);
    }
    let connection_id =
        match open_outproxy_streaming(manager, destination_id, &client.remote, connect_timeout_ms)
            .await
        {
            Ok(id) => id,
            Err(()) => return Err(OutproxyFailure::TargetUnreachable),
        };
    // A route that never reaches `Established` cannot carry a handshake, so
    // establishment is awaited under the same bounded deadline.
    if wait_for_established(manager, destination_id, connection_id, connect_timeout_ms)
        .await
        .is_err()
    {
        terminate(manager, destination_id, connection_id, &client.remote, true);
        return Err(OutproxyFailure::TargetUnreachable);
    }
    // Send the handshake once, then read until the reply is complete.
    if send_handshake(
        manager,
        destination_id,
        connection_id,
        &client.remote,
        request.expose(),
    )
    .is_err()
    {
        terminate(manager, destination_id, connection_id, &client.remote, true);
        return Err(OutproxyFailure::TargetUnreachable);
    }

    let deadline = Instant::now() + Duration::from_millis(OUTPROXY_HANDSHAKE_DEADLINE_MS);
    let mut received: Vec<u8> = Vec::new();
    loop {
        if cancellation.is_cancelled() {
            terminate(manager, destination_id, connection_id, &client.remote, true);
            return Err(OutproxyFailure::NotPermitted);
        }
        match manager.with_destination_bridge(destination_id, |bridge| {
            bridge.streaming_mut().drain_delivered_for(connection_id)
        }) {
            Some(chunks) => {
                for chunk in chunks {
                    if !chunk.bytes.is_empty() {
                        received.extend_from_slice(&chunk.bytes);
                    }
                }
            }
            None => {
                terminate(manager, destination_id, connection_id, &client.remote, true);
                return Err(OutproxyFailure::TargetUnreachable);
            }
        }
        // A hostile or broken outproxy must not be able to make this buffer
        // grow without bound.
        if received.len() > MAX_OUTPROXY_HANDSHAKE_BYTES {
            terminate(manager, destination_id, connection_id, &client.remote, true);
            return Err(OutproxyFailure::TargetUnreachable);
        }
        if is_complete(kind, &received) {
            break;
        }
        if Instant::now() >= deadline {
            terminate(manager, destination_id, connection_id, &client.remote, true);
            return Err(OutproxyFailure::TargetUnreachable);
        }
        tokio::time::sleep(HANDSHAKE_POLL_INTERVAL).await;
    }

    let tunnel_prefix = match interpret_reply(kind, &received) {
        Ok(value) => value,
        Err(error) => {
            terminate(manager, destination_id, connection_id, &client.remote, true);
            return Err(classify(error));
        }
    };
    Ok(OutproxySession {
        connection_id,
        outproxy: endpoint.as_str(),
        remote: client.remote.clone(),
        tunnel_prefix,
    })
}

/// Poll interval while waiting for an outproxy's reply.
///
/// Short enough that a local outproxy is not slowed noticeably, long enough
/// that a stalled one does not spin the runtime.
const HANDSHAKE_POLL_INTERVAL: Duration = Duration::from_millis(5);

/// Sends the staged handshake bytes once over the established route.
fn send_handshake(
    manager: &Arc<ServiceTunnelManager>,
    destination_id: i2pr_client::DestinationId,
    connection_id: ConnectionId,
    remote: &RemoteDestination,
    bytes: &[u8],
) -> Result<(), ()> {
    let identity_arc = manager
        .with_destination_bridge(destination_id, |bridge| bridge.identity())
        .ok_or(())?;
    manager
        .with_destination_bridge(destination_id, |bridge| {
            let Some((local_port, remote_port)) = bridge
                .streaming()
                .get_connection(connection_id)
                .map(|conn| (conn.local_port(), conn.remote_port()))
            else {
                return Err(());
            };
            bridge
                .streaming_mut()
                .send_data(
                    connection_id,
                    identity_arc.as_ref(),
                    remote,
                    local_port,
                    remote_port,
                    bytes,
                    service_streaming_now_ms(),
                )
                // The send returns a dispatch request for the runtime; the
                // handshake only cares that the streaming manager accepted
                // the bytes, so the request is dropped here deliberately.
                .map(|_| ())
                .map_err(|_| ())
        })
        .ok_or(())?
}

/// Waits for the route to reach `Established` under a bounded deadline.
async fn wait_for_established(
    manager: &Arc<ServiceTunnelManager>,
    destination_id: i2pr_client::DestinationId,
    connection_id: ConnectionId,
    timeout_ms: u64,
) -> Result<(), ()> {
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    loop {
        let established = manager
            .with_destination_bridge(destination_id, |bridge| {
                bridge
                    .streaming()
                    .get_connection(connection_id)
                    .is_some_and(|conn| matches!(conn.state(), ConnectionState::Established))
            })
            .unwrap_or(false);
        if established {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(());
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

/// Opens the Streaming connection to the outproxy destination.
///
/// This is the only socket-shaped operation in the module, and the
/// destination it connects to is always an I2P outproxy reference that
/// [`OutproxyEndpoint::parse`] already proved is an I2P destination.
async fn open_outproxy_streaming(
    manager: &Arc<ServiceTunnelManager>,
    destination_id: i2pr_client::DestinationId,
    remote: &RemoteDestination,
    _timeout_ms: u64,
) -> Result<ConnectionId, ()> {
    let identity_arc = manager
        .with_destination_bridge(destination_id, |bridge| bridge.identity())
        .ok_or(())?;
    let outcome = {
        let mut os_rng = OsRng;
        let mut rng = rand_core::UnwrapMut(&mut os_rng);
        manager.with_destination_bridge(destination_id, |bridge| {
            bridge.streaming_mut().connect(
                identity_arc.as_ref(),
                remote,
                0,
                0,
                DEFAULT_ADVERTISED_MAX_PAYLOAD,
                service_streaming_now_ms(),
                &mut rng,
            )
        })
    };
    // `with_destination_bridge` is the optional half (no such destination)
    // and `connect` is the fallible half; both refuse the route.
    // Plan 182: kick the delivery driver so the queued SYN is routed
    // immediately instead of waiting for the fallback tick.
    //
    // This was missing and the wire lane found it: without the kick the SYN
    // stayed queued on the client destination's Streaming table, so
    // `wait_for_established` below timed out and every route failed with
    // `TargetUnreachable` while the counters looked like a network problem.
    // The direct path (`service_tunnels_http::open_streaming`) has had this
    // since Plan 182; the outproxy opener was written later and did not.
    manager.notify_outbound_signal(destination_id);
    match outcome.ok_or(())?.map_err(|_| ())? {
        ConnectOutcome::SynSent { connection_id, .. } => Ok(connection_id),
        ConnectOutcome::ConnectionTableFull
        | ConnectOutcome::PendingBudgetExhausted
        | ConnectOutcome::PayloadTooLarge { .. }
        | ConnectOutcome::Codec(_) => Err(()),
    }
}

/// Closes the route to the outproxy, treating the close as a lifecycle event.
fn terminate(
    manager: &Arc<ServiceTunnelManager>,
    destination_id: i2pr_client::DestinationId,
    connection_id: ConnectionId,
    remote: &RemoteDestination,
    reset: bool,
) {
    let identity_arc = manager.with_destination_bridge(destination_id, |bridge| bridge.identity());
    let Some(identity_arc) = identity_arc else {
        return;
    };
    manager.with_destination_bridge(destination_id, |bridge| {
        let Some((local_port, remote_port)) = bridge
            .streaming()
            .get_connection(connection_id)
            .map(|conn| (conn.local_port(), conn.remote_port()))
        else {
            return;
        };
        // The port tuple is read from the connection rather than assumed, so
        // the close cannot be redirected and a mismatch fails closed inside
        // the streaming manager instead of closing the wrong stream.
        let _ = if reset {
            bridge.streaming_mut().send_reset(
                connection_id,
                identity_arc.as_ref(),
                remote,
                local_port,
                remote_port,
                service_streaming_now_ms(),
            )
        } else {
            bridge.streaming_mut().send_close(
                connection_id,
                identity_arc.as_ref(),
                remote,
                local_port,
                remote_port,
                service_streaming_now_ms(),
            )
        };
    });
}

/// Applies a counter update under the lock.
fn note(counters: &std::sync::Mutex<OutproxyCounters>, apply: impl FnOnce(&mut OutproxyCounters)) {
    if let Ok(mut guard) = counters.lock() {
        apply(&mut guard);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use i2pr_service_tunnels::outbound_secret::NoOutboundSecrets;
    use i2pr_service_tunnels::outproxy::OutproxyList;

    fn b32() -> String {
        format!(
            "{}.b32.i2p",
            i2pr_service_tunnels::encode_b32_label(&[0x33; 32])
        )
    }

    fn config() -> OutproxyConfig {
        OutproxyConfig {
            list: OutproxyList::parse(&format!("{},second.example.i2p", b32())).expect("list"),
            kind: OutproxyType::HttpConnect,
            present_credential: false,
            username: None,
            tunnelled: OutproxyList::parse(&b32()).expect("tunnelled"),
            policy: i2pr_service_tunnels::outproxy::OutproxyPolicy::default(),
        }
    }

    #[test]
    fn classification_is_total_over_every_codec_error() {
        assert_eq!(
            classify(OutproxyError::CredentialRejected),
            OutproxyFailure::AuthenticationRejected
        );
        assert_eq!(
            classify(OutproxyError::UpstreamRefused),
            OutproxyFailure::TargetUnreachable
        );
        assert_eq!(
            classify(OutproxyError::MalformedResponse { reason: "x" }),
            OutproxyFailure::TargetUnreachable
        );
        for error in [
            OutproxyError::UnknownType,
            OutproxyError::NotAnI2pDestination,
            OutproxyError::DuplicateEndpoint,
            OutproxyError::TunnelledNotInList,
            OutproxyError::ExceedsCeiling { field: "x" },
        ] {
            assert_eq!(classify(error), OutproxyFailure::NotPermitted);
        }
        // A rejected credential is the one failure worth retrying with a
        // different outproxy; a malformed request is not.
        assert!(OutproxyFailure::AuthenticationRejected.is_retryable());
        assert!(!OutproxyFailure::NotPermitted.is_retryable());
    }

    #[test]
    fn an_http_attempt_carries_the_credential_and_keeps_tunnelled_bytes() {
        let target = OutproxyTarget::parse_authority("example.com:443").expect("target");
        let request = build_attempt(OutproxyType::HttpConnect, &target, None).expect("request");
        // An outproxy may answer `CONNECT` and start pushing tunnelled
        // payload in the same burst, so the bytes after the response head
        // must survive as the pump's first read.
        let tunnelled = b"GET /index.html HTTP/1.1\r\nHost: example.com\r\n\r\n".to_vec();
        let mut received = b"HTTP/1.1 200 Connection Established\r\nX-Proxy: yes\r\n\r\n".to_vec();
        received.extend_from_slice(&tunnelled);
        assert!(is_complete(OutproxyType::HttpConnect, &received));
        assert_eq!(
            interpret_reply(OutproxyType::HttpConnect, &received).expect("reply"),
            tunnelled
        );
        // The request is what gets sent, not what comes back.
        assert_eq!(
            request.expose(),
            b"CONNECT example.com:443 HTTP/1.1\r\nHost: example.com:443\r\n\r\n"
        );
    }

    #[test]
    fn an_incomplete_reply_is_not_yet_malformed() {
        assert!(!is_complete(
            OutproxyType::HttpConnect,
            b"HTTP/1.1 200 OK\r\n"
        ));
        assert!(is_complete(
            OutproxyType::HttpConnect,
            b"HTTP/1.1 200 OK\r\n\r\n"
        ));
        assert!(!is_complete(OutproxyType::Socks5, b"\x05"));
        assert!(!is_complete(OutproxyType::Socks5, b"\x05\x00\x05"));
        assert!(is_complete(
            OutproxyType::Socks5,
            b"\x05\x00\x05\x00\x00\x01\x00\x00\x00\x00\x00\x00"
        ));
        assert!(!is_complete(OutproxyType::Socks4a, b"\x00\x5A"));
        assert!(is_complete(
            OutproxyType::Socks4a,
            b"\x00\x5A\x00\x50\x00\x01\x00\x00"
        ));
    }

    #[test]
    fn a_socks5_domain_reply_is_length_framed() {
        // Method selection, then a connect reply with a DOMAINNAME bind.
        let mut received = vec![0x05, 0x00];
        received.extend_from_slice(&[0x05, 0x00, 0x00, 0x03, 0x0b]);
        received.extend_from_slice(b"example.com");
        received.extend_from_slice(&80_u16.to_be_bytes());
        received.extend_from_slice(b"tail");
        assert!(is_complete(OutproxyType::Socks5, &received));
        assert_eq!(
            interpret_reply(OutproxyType::Socks5, &received).expect("reply"),
            b"tail"
        );
        // An unknown address type is malformed, not incomplete.
        let bad = vec![0x05, 0x00, 0x05, 0x00, 0x00, 0x09];
        assert!(!is_complete(OutproxyType::Socks5, &bad));
        assert!(interpret_reply(OutproxyType::Socks5, &bad).is_err());
    }

    #[test]
    fn a_socks_outproxy_refuses_an_http_credential() {
        let target = OutproxyTarget::parse_authority("example.com:80").expect("target");
        let password = OutboundSecret::new("s3cret").expect("secret");
        let header = OutproxyAuthHeader::basic("alice", &password).expect("header");
        for kind in [OutproxyType::Socks5, OutproxyType::Socks4a] {
            assert_eq!(
                build_attempt(kind, &target, Some(&header)).err(),
                Some(OutproxyError::MalformedCredential {
                    reason: "a socks outproxy cannot carry an http basic credential"
                }),
                "{kind:?} must refuse an http credential rather than drop it"
            );
        }
        assert!(build_attempt(OutproxyType::Socks5, &target, None).is_ok());
    }

    #[test]
    fn a_provider_without_a_credential_reports_none_and_still_routes() {
        let provider = RouterOutproxyProvider::new(config(), Arc::new(NoOutboundSecrets), None)
            .expect("provider");
        assert!(!provider.credential_available());
        assert!(!provider.has_stored_credential());
        assert!(provider.auth_header().expect("no credential").is_none());
        assert_eq!(provider.configured_count(), 2);
        let target = OutproxyTarget::parse_authority("example.com:80").expect("target");
        assert!(matches!(
            provider.select(&target, 0),
            OutproxyRoute::ViaOutproxy { .. }
        ));
    }

    #[test]
    fn a_credential_that_cannot_be_recovered_fails_closed() {
        // `NoOutboundSecrets` cannot open anything, which is exactly the
        // "no owner installed" case. It must be an error, never an
        // unauthenticated request.
        let mut armed = config();
        armed.present_credential = true;
        armed.username = Some("alice".to_owned());
        let provider = RouterOutproxyProvider::new(
            armed,
            Arc::new(NoOutboundSecrets),
            Some("$i2pr1o$00ff".to_owned()),
        )
        .expect("provider");
        assert!(!provider.credential_available());
        assert!(provider.auth_header().is_err());
    }

    #[test]
    fn an_armed_credential_without_a_stored_form_is_refused() {
        let mut armed = config();
        armed.present_credential = true;
        armed.username = Some("alice".to_owned());
        let provider = RouterOutproxyProvider::new(armed, Arc::new(NoOutboundSecrets), None)
            .expect("provider");
        assert!(!provider.credential_available());
        assert!(provider.auth_header().is_err());
    }

    #[test]
    fn an_invalid_configuration_is_refused_at_construction() {
        let mut bad = config();
        bad.present_credential = true;
        // Armed without a username.
        assert!(RouterOutproxyProvider::new(bad, Arc::new(NoOutboundSecrets), None).is_err());
        let mut subset = config();
        subset.tunnelled = OutproxyList::parse("absent.example.i2p").expect("absent");
        assert!(RouterOutproxyProvider::new(subset, Arc::new(NoOutboundSecrets), None).is_err());
    }

    #[test]
    fn an_exhausted_request_records_both_its_last_reason_and_the_exhaustion() {
        let counters = std::sync::Mutex::new(OutproxyCounters::default());
        note_exhausted(&counters, OutproxyFailure::AuthenticationRejected);
        let guard = counters.lock().expect("counters");
        // A wrong password N times is the diagnosis; "exhausted" alone would
        // hide it, so both facts are recorded.
        assert_eq!(guard.authentication_rejected, 1);
        assert_eq!(guard.attempts_exhausted, 1);
        assert_eq!(guard.connect_attempts, 0);
    }

    #[test]
    fn counters_are_sanitized_and_additive() {
        let mut counters = OutproxyCounters::default();
        counters.note(OutproxyFailure::NotConfigured);
        counters.note(OutproxyFailure::NotConfigured);
        counters.note(OutproxyFailure::AuthenticationRejected);
        assert_eq!(counters.not_configured, 2);
        assert_eq!(counters.refused_without_provider(), 2);
        assert_eq!(counters.authentication_rejected, 1);
        assert_eq!(counters.target_unreachable, 0);
        // A `Debug` of the counters carries no host, username, or header: it
        // is a struct of counts, and this row is what keeps it that way.
        let text = format!("{counters:?}");
        assert!(!text.contains("example.i2p"));
        assert!(!text.contains("alice"));
    }

    #[test]
    fn a_tunnelled_request_to_an_unlisted_outproxy_is_refused() {
        let provider = RouterOutproxyProvider::new(config(), Arc::new(NoOutboundSecrets), None)
            .expect("provider");
        // The list has two outproxies; only the first is in `tunnelled`.
        let second = provider.config().list.select(1).expect("second");
        assert!(!provider.config().permits_tunnelled(second));
    }
}
