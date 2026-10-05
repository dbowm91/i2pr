//! Plan 177 Milestone 10 SOCKS5 `.i2p` CONNECT proxy executor.
//!
//! The Plan 177 SOCKS5 client proxy is the second M10 application
//! profile that owns a bounded protocol parser. The runtime-neutral
//! policy (parser bounds, negotiation, request validation, reply
//! generation) lives in `i2pr-service-tunnels::socks5`; this module
//! owns the socket and task orchestration.
//!
//! ```text
//! loopback TCP accept
//!   -> bounded greeting read (deadline)
//!   -> runtime-neutral greeting negotiation
//!   -> bounded CONNECT request read (deadline)
//!   -> runtime-neutral request parser + target validator
//!   -> Manager destination resolution (Base32 / alias / local)
//!   -> I2P Streaming connect (CSPRNG, OS)
//!   -> on Streaming Established: send SOCKS5 success reply
//!   -> run shared socket <-> Streaming pump in opaque tunnel mode
//! ```
//!
//! The proxy is loopback-only, disabled by default, and uses the
//! existing Plan 149 local destination product path plus the Plan
//! 174 shared socket <-> Streaming pump. No new Garlic/I2NP/Streaming
//! implementation is introduced.

#![forbid(unsafe_code)]

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use i2pr_client::streaming::connection::{ConnectionId, ConnectionState};
use i2pr_client::streaming::manager::{
    ConnectOutcome, DEFAULT_ADVERTISED_MAX_PAYLOAD, RemoteDestination,
};
use i2pr_crypto::OsRng;
use i2pr_runtime::CancellationToken;
use i2pr_service_tunnels::{
    ConnectDestination, GreetingOutcome, GreetingParser, ProxyCredentials, RequestOutcome,
    RequestParser, Socks4aOutcome, Socks4aRequestParser, Socks5ClientOptions, Socks5Error,
    Socks5ErrorKind, Socks5Limits, build_socks4a_reply, build_socks5_reply,
    build_socks5_reply_from_code,
};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;
use tracing::{debug, warn};

use crate::destination_streaming::{PumpConfig, StreamPumpEndpoint, run_stream_pump};
use crate::outproxy_route::open_client_route;
use crate::service_tunnels::{
    ClientTarget, DestinationFailure, ServicePumpEndpoint, ServiceRuntime, ServiceTunnelManager,
    service_streaming_now_ms,
};
use i2pr_service_tunnels::outproxy::{ClientTargetClass, classify_client_target};

/// Default ceiling for reading the SOCKS5 greeting section.
const GREETING_READ_DEADLINE: Duration = Duration::from_secs(30);
/// Default ceiling for reading the SOCKS5 CONNECT request section.
const REQUEST_READ_DEADLINE: Duration = Duration::from_secs(30);
/// Default read chunk size for the SOCKS5 byte pump.
const BODY_CHUNK_BYTES: usize = 32 * 1024;

/// Outcome of one SOCKS5 connection attempt (typed for tests).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Socks5ConnectionOutcome {
    /// CONNECT completed and the opaque byte pump ran to EOF.
    TunnelClosed,
    /// Greeting was malformed or unsupported and the socket was
    /// closed after the bounded reply was flushed.
    BadGreeting,
    /// CONNECT request was malformed or unsupported and the
    /// bounded SOCKS5 reply was emitted before close.
    BadRequest,
    /// CONNECT target was rejected by the proxy policy (clearnet,
    /// local, IP, mixed-suffix, etc).
    Forbidden,
    /// The configured destination was unknown and no I2P connect
    /// was attempted.
    BadGateway,
    /// Proxy authentication failed (or was required but missing)
    /// and the socket was closed after the bounded reply.
    AuthFailed,
    /// The header read or connect deadline expired.
    TimedOut,
}

/// Reads the complete SOCKS5 greeting section from the supplied
/// stream into a bounded buffer, dispatching into the runtime-
/// neutral greeting parser. `initial` carries already-consumed
/// bytes (the version peek) that belong to the greeting section.
/// Returns the bytes after the greeting (which belong to the
/// request section) on success.
async fn read_greeting<S>(
    stream: &mut S,
    initial: Vec<u8>,
    limits: Socks5Limits,
    deadline: Duration,
    auth_required: bool,
) -> Result<(GreetingOutcome, Vec<u8>), Socks5Error>
where
    S: AsyncRead + Unpin,
{
    let mut parser = GreetingParser::new();
    let advanced = if auth_required {
        parser.advance_auth(&initial, limits)
    } else {
        parser.advance(&initial, limits)
    };
    if !initial.is_empty()
        && let Some((outcome, consumed)) = advanced?
    {
        let remainder = if consumed < initial.len() {
            initial[consumed..].to_vec()
        } else {
            Vec::new()
        };
        return Ok((outcome, remainder));
    }
    let mut chunk = [0_u8; 256];
    let started = Instant::now();
    loop {
        let elapsed = started.elapsed();
        if elapsed >= deadline {
            return Err(Socks5Error::new(
                Socks5ErrorKind::GreetingCeiling,
                "greeting read deadline exceeded",
            ));
        }
        let remaining = deadline.saturating_sub(elapsed);
        let read = match timeout(remaining, stream.read(&mut chunk)).await {
            Ok(Ok(0)) => {
                return Err(Socks5Error::new(
                    Socks5ErrorKind::GreetingCeiling,
                    "EOF before greeting section terminator",
                ));
            }
            Ok(Ok(n)) => n,
            Ok(Err(_error)) => {
                return Err(Socks5Error::new(
                    Socks5ErrorKind::GreetingCeiling,
                    "greeting read io error",
                ));
            }
            Err(_) => {
                return Err(Socks5Error::new(
                    Socks5ErrorKind::GreetingCeiling,
                    "greeting read deadline exceeded",
                ));
            }
        };
        let advanced = if auth_required {
            parser.advance_auth(&chunk[..read], limits)?
        } else {
            parser.advance(&chunk[..read], limits)?
        };
        match advanced {
            Some((outcome, consumed)) => {
                // Bytes past the greeting terminator belong to the
                // CONNECT request; forward them as the initial
                // request buffer. After truncation the parser
                // retained only the greeting bytes, so `consumed`
                // is the offset into the chunk where the request
                // section begins.
                let remainder = if consumed < read {
                    chunk[consumed..read].to_vec()
                } else {
                    Vec::new()
                };
                return Ok((outcome, remainder));
            }
            None => continue,
        }
    }
}

/// Reads the complete SOCKS5 CONNECT request section from the
/// supplied stream into a bounded buffer, dispatching into the
/// runtime-neutral request parser. Returns the parsed
/// [`RequestOutcome`] on success.
async fn read_request<S>(
    stream: &mut S,
    initial: Vec<u8>,
    limits: Socks5Limits,
    deadline: Duration,
) -> Result<RequestOutcome, Socks5Error>
where
    S: AsyncRead + Unpin,
{
    let mut parser = RequestParser::new();
    if !initial.is_empty()
        && let Some(outcome) = parser.advance(&initial, limits)?
    {
        return Ok(outcome);
    }
    let mut chunk = [0_u8; 1024];
    let started = Instant::now();
    loop {
        let elapsed = started.elapsed();
        if elapsed >= deadline {
            return Err(Socks5Error::new(
                Socks5ErrorKind::BufferCeilingExceeded,
                "request read deadline exceeded",
            ));
        }
        let remaining = deadline.saturating_sub(elapsed);
        let read = match timeout(remaining, stream.read(&mut chunk)).await {
            Ok(Ok(0)) => {
                return Err(Socks5Error::new(
                    Socks5ErrorKind::BufferCeilingExceeded,
                    "EOF before request section terminator",
                ));
            }
            Ok(Ok(n)) => n,
            Ok(Err(_error)) => {
                return Err(Socks5Error::new(
                    Socks5ErrorKind::BufferCeilingExceeded,
                    "request read io error",
                ));
            }
            Err(_) => {
                return Err(Socks5Error::new(
                    Socks5ErrorKind::BufferCeilingExceeded,
                    "request read deadline exceeded",
                ));
            }
        };
        match parser.advance(&chunk[..read], limits)? {
            Some(outcome) => return Ok(outcome),
            None => continue,
        }
    }
}

/// Reads one complete SOCKS4a CONNECT request section (the
/// version byte is already consumed into `initial`), dispatching
/// into the runtime-neutral 4a parser. Returns the parsed
/// [`Socks4aOutcome`] on success.
async fn read_socks4a_request<S>(
    stream: &mut S,
    initial: Vec<u8>,
    limits: Socks5Limits,
    deadline: Duration,
) -> Result<Socks4aOutcome, Socks5Error>
where
    S: AsyncRead + Unpin,
{
    let mut parser = Socks4aRequestParser::new();
    if !initial.is_empty()
        && let Some(outcome) = parser.advance(&initial, limits)?
    {
        return Ok(outcome);
    }
    let mut chunk = [0_u8; 1024];
    let started = Instant::now();
    loop {
        let elapsed = started.elapsed();
        if elapsed >= deadline {
            return Err(Socks5Error::new(
                Socks5ErrorKind::BufferCeilingExceeded,
                "4a request read deadline exceeded",
            ));
        }
        let remaining = deadline.saturating_sub(elapsed);
        let read = match timeout(remaining, stream.read(&mut chunk)).await {
            Ok(Ok(0)) => {
                return Err(Socks5Error::new(
                    Socks5ErrorKind::BufferCeilingExceeded,
                    "EOF before 4a request terminator",
                ));
            }
            Ok(Ok(n)) => n,
            Ok(Err(_error)) => {
                return Err(Socks5Error::new(
                    Socks5ErrorKind::BufferCeilingExceeded,
                    "4a request read io error",
                ));
            }
            Err(_) => {
                return Err(Socks5Error::new(
                    Socks5ErrorKind::BufferCeilingExceeded,
                    "4a request read deadline exceeded",
                ));
            }
        };
        match parser.advance(&chunk[..read], limits)? {
            Some(outcome) => return Ok(outcome),
            None => continue,
        }
    }
}

/// One negotiated SOCKS CONNECT target shared by the `socks` and
/// `socks-irc` backends. Both protocol versions converge here;
/// only the success/reject reply framing differs downstream.
pub struct SocksNegotiation {
    /// Parsed CONNECT destination (`.i2p`-only, validated).
    pub destination: ConnectDestination,
    /// Same-read bytes after the CONNECT request (first tunnel
    /// bytes for opaque profiles, first filter bytes for
    /// `socks-irc`).
    pub leftover: Vec<u8>,
    /// Whether the peer negotiated SOCKS4a (8-byte reply framing)
    /// rather than SOCKS5 (10-byte reply framing).
    pub via_socks4a: bool,
}

/// Negotiates one SOCKS CONNECT (SOCKS5 greeting + request, or
/// bare SOCKS4a request after a `0x04` version peek) and emits the
/// version-appropriate greeting/reject replies inline. Returns the
/// negotiated target, or the connection outcome after flushing
/// the terminal reply and shutting the socket down.
///
/// Plan 290: shared by the ordinary `socks` backend (pinned
/// SOCKS 4/4a/5 parity) and the `socks-irc` composition. The
/// success reply is NOT emitted here: it goes out only after real
/// Streaming establishment, per profile.
pub async fn negotiate_socks_destination(
    stream: &mut TcpStream,
    limits: Socks5Limits,
    auth: Option<&ProxyCredentials>,
) -> Result<SocksNegotiation, Socks5ConnectionOutcome> {
    // Version peek: SOCKS4a has no greeting, so the first byte
    // decides the negotiation path.
    let mut version = [0_u8; 1];
    let peeked = timeout(GREETING_READ_DEADLINE, stream.read_exact(&mut version)).await;
    if peeked.is_err() {
        let _ = stream.shutdown().await;
        return Err(Socks5ConnectionOutcome::BadGreeting);
    }
    if version[0] == i2pr_service_tunnels::socks5::socks4a::SOCKS4A_VERSION {
        // SOCKS4a carries no authentication: an auth-guarded
        // listener rejects it instead of downgrading.
        if auth.is_some() {
            let _ = stream.shutdown().await;
            return Err(Socks5ConnectionOutcome::AuthFailed);
        }
        return negotiate_socks4a(stream, limits).await;
    }
    if version[0] != i2pr_service_tunnels::socks5::config::SOCKS_VERSION {
        let _ = stream.shutdown().await;
        return Err(Socks5ConnectionOutcome::BadGreeting);
    }
    // Step 1: greeting read + negotiation (version byte re-fed).
    // With credentials configured only `0x02` is acceptable.
    let (greeting_outcome, request_initial) = match read_greeting(
        stream,
        vec![version[0]],
        limits,
        GREETING_READ_DEADLINE,
        auth.is_some(),
    )
    .await
    {
        Ok(value) => value,
        Err(_error) => {
            let _ = stream.shutdown().await;
            return Err(Socks5ConnectionOutcome::BadGreeting);
        }
    };
    match greeting_outcome {
        GreetingOutcome::UsernamePassword => {
            let credentials = match auth {
                Some(credentials) => credentials,
                None => {
                    let no_accept = GreetingParser::no_acceptable_method_reply();
                    let _ = stream.write_all(&no_accept).await;
                    let _ = stream.shutdown().await;
                    return Err(Socks5ConnectionOutcome::BadGreeting);
                }
            };
            // Select username/password, then run the RFC 1929
            // subnegotiation before the request stage.
            if let Err(error) = stream.write_all(&[0x05, 0x02]).await {
                debug!(error = %error, "method selection write failed");
                let _ = stream.shutdown().await;
                return Err(Socks5ConnectionOutcome::BadGreeting);
            }
            negotiate_user_pass(stream, credentials).await?;
        }
        GreetingOutcome::NoAuthentication => {
            if auth.is_some() {
                // Unreachable through `advance_auth`, but a
                // no-auth selection on a guarded listener must
                // never proceed.
                let no_accept = GreetingParser::no_acceptable_method_reply();
                let _ = stream.write_all(&no_accept).await;
                let _ = stream.shutdown().await;
                return Err(Socks5ConnectionOutcome::AuthFailed);
            }
            let reply = GreetingParser::no_auth_reply();
            if let Err(error) = stream.write_all(&reply).await {
                debug!(error = %error, "greeting reply write failed");
                let _ = stream.shutdown().await;
                return Err(Socks5ConnectionOutcome::BadGreeting);
            }
        }
        GreetingOutcome::NoAcceptableMethod => {
            let no_accept = GreetingParser::no_acceptable_method_reply();
            if let Err(error) = stream.write_all(&no_accept).await {
                debug!(error = %error, "no-acceptable-method write failed");
            }
            let _ = stream.shutdown().await;
            return Err(Socks5ConnectionOutcome::BadGreeting);
        }
    }
    // Step 2: CONNECT request read + parse.
    let request_outcome =
        match read_request(stream, request_initial, limits, REQUEST_READ_DEADLINE).await {
            Ok(value) => value,
            Err(_error) => {
                let _ = stream.shutdown().await;
                return Err(Socks5ConnectionOutcome::BadRequest);
            }
        };
    let (destination, leftover) = match request_outcome {
        RequestOutcome::ReadyToConnect {
            destination,
            leftover,
        } => (destination, leftover),
        RequestOutcome::Rejected { reply_code } => {
            let reply = build_socks5_reply_from_code(reply_code);
            let _ = write_reply(stream, reply).await;
            let _ = stream.shutdown().await;
            return Err(Socks5ConnectionOutcome::Forbidden);
        }
    };
    Ok(SocksNegotiation {
        destination,
        leftover,
        via_socks4a: false,
    })
}

/// Runs one RFC 1929 username/password subnegotiation and verifies
/// it against the tunnel credentials (Plan 292).
///
/// Replies `01 00` and proceeds on verified credentials, `01 01`
/// and closes when the complete section parses but verification
/// fails. Framing violations close silently: the stream position is
/// unrecoverable, so no status oracle is emitted. All reads share
/// the greeting deadline; the section is bounded to 513 bytes by
/// the protocol's own length octets.
async fn negotiate_user_pass(
    stream: &mut TcpStream,
    credentials: &ProxyCredentials,
) -> Result<(), Socks5ConnectionOutcome> {
    let mut header = [0_u8; 2];
    if timeout(GREETING_READ_DEADLINE, stream.read_exact(&mut header))
        .await
        .is_err()
    {
        return Err(auth_failed_closed(stream).await);
    }
    if header[0] != 0x01 {
        return Err(auth_failed_closed(stream).await);
    }
    let username_len = header[1] as usize;
    if username_len == 0 || username_len > i2pr_service_tunnels::MAX_PROXY_USERNAME_LEN {
        return Err(auth_failed_closed(stream).await);
    }
    let mut username = vec![0_u8; username_len];
    if timeout(GREETING_READ_DEADLINE, stream.read_exact(&mut username))
        .await
        .is_err()
    {
        return Err(auth_failed_closed(stream).await);
    }
    let mut length = [0_u8; 1];
    if timeout(GREETING_READ_DEADLINE, stream.read_exact(&mut length))
        .await
        .is_err()
    {
        return Err(auth_failed_closed(stream).await);
    }
    let password_len = length[0] as usize;
    if password_len == 0 || password_len > i2pr_service_tunnels::MAX_PROXY_PASSWORD_LEN {
        return Err(auth_failed_closed(stream).await);
    }
    let mut password = vec![0_u8; password_len];
    if timeout(GREETING_READ_DEADLINE, stream.read_exact(&mut password))
        .await
        .is_err()
    {
        return Err(auth_failed_closed(stream).await);
    }
    let verified = match (String::from_utf8(username), String::from_utf8(password)) {
        (Ok(user), Ok(pass)) => credentials.verify(&user, &pass),
        _ => false,
    };
    let reply = if verified { [0x01, 0x00] } else { [0x01, 0x01] };
    if stream.write_all(&reply).await.is_err() {
        return Err(auth_failed_closed(stream).await);
    }
    if !verified {
        return Err(auth_failed_closed(stream).await);
    }
    Ok(())
}

/// Shuts the socket and reports authentication failure (framing
/// violations close silently: the stream position is
/// unrecoverable, so no status oracle is emitted).
async fn auth_failed_closed(stream: &mut TcpStream) -> Socks5ConnectionOutcome {
    let _ = stream.shutdown().await;
    Socks5ConnectionOutcome::AuthFailed
}

/// Negotiates the SOCKS4a bare-request path (no greeting).
async fn negotiate_socks4a(
    stream: &mut TcpStream,
    limits: Socks5Limits,
) -> Result<SocksNegotiation, Socks5ConnectionOutcome> {
    let outcome =
        match read_socks4a_request(stream, vec![0x04], limits, REQUEST_READ_DEADLINE).await {
            Ok(value) => value,
            Err(_error) => {
                let _ = stream.shutdown().await;
                return Err(Socks5ConnectionOutcome::BadRequest);
            }
        };
    let (destination, leftover) = match outcome {
        Socks4aOutcome::ReadyToConnect {
            destination,
            leftover,
        } => (destination, leftover),
        Socks4aOutcome::Rejected => {
            let _ = stream.write_all(&build_socks4a_reply(false)).await;
            let _ = stream.shutdown().await;
            return Err(Socks5ConnectionOutcome::Forbidden);
        }
    };
    Ok(SocksNegotiation {
        destination,
        leftover,
        via_socks4a: true,
    })
}

/// Writes a bounded SOCKS5 reply and (optionally) shuts down the
/// write half of the socket.
async fn write_reply<S>(stream: &mut S, reply: [u8; 10]) -> std::io::Result<()>
where
    S: AsyncWrite + Unpin,
{
    if let Err(error) = stream.write_all(&reply).await {
        debug!(error = %error, "socks5 reply write failed");
    }
    Ok(())
}

/// Writes one bounded SOCKS4a reply (8 bytes, never echoes
/// request state).
async fn write_socks4a_reply(stream: &mut TcpStream, granted: bool) -> std::io::Result<()> {
    if let Err(error) = stream.write_all(&build_socks4a_reply(granted)).await {
        debug!(error = %error, "socks4a reply write failed");
    }
    Ok(())
}

/// Emits one version-appropriate terminal reject reply (SOCKS4a
/// 8-byte 91 framing or SOCKS5 10-byte RFC 1928 framing) and shuts
/// the socket down.
///
/// Plan 290: shared with the `socks-irc` composition.
pub async fn reject_socks_request(
    stream: &mut TcpStream,
    via_socks4a: bool,
    code: i2pr_service_tunnels::Socks5ReplyCode,
) {
    if via_socks4a {
        let _ = write_socks4a_reply(stream, false).await;
    } else {
        let reply = build_socks5_reply(code);
        let _ = write_reply(stream, reply).await;
    }
    let _ = stream.shutdown().await;
}

/// Resolves a parsed CONNECT destination to a [`ClientTarget`]. The
/// proxy only forwards requests whose host matches the configured
/// service destination (Base32) or an entry in the alias table.
///
/// Plan 290: shared with the `socks-irc` composition (identical
/// resolution policy after either SOCKS version negotiates).
pub fn resolve_target_for_service(
    manager: &ServiceTunnelManager,
    service_destination: i2pr_client::DestinationId,
    destination: &ConnectDestination,
) -> Result<ClientTarget, Socks5Error> {
    let reference =
        i2pr_service_tunnels::DestinationRef::parse(&destination.host).map_err(|_| {
            Socks5Error::new(
                Socks5ErrorKind::NonI2pTarget,
                "destination not resolvable by SOCKS5 proxy",
            )
        })?;
    match manager.resolve_reference(&reference) {
        Ok(client) => Ok(client),
        // Plan 214 — non-local references resolve through the
        // requesting runtime's installed router-backed remote
        // LeaseSet2 mirror. No local fallback.
        Err(DestinationFailure::LookupRequired { hash, .. }) => manager
            .resolve_remote_client_target(service_destination, &hash)
            .ok_or_else(|| {
                Socks5Error::new(
                    Socks5ErrorKind::NonI2pTarget,
                    "destination not resolvable by SOCKS5 proxy",
                )
            }),
        Err(_) => Err(Socks5Error::new(
            Socks5ErrorKind::NonI2pTarget,
            "destination not resolvable by SOCKS5 proxy",
        )),
    }
}

/// Opens a Streaming connection to the supplied remote destination
/// and waits until `Established` (or returns a typed error).
///
/// Plan 290: shared with the `socks-irc` composition.
pub async fn open_streaming(
    manager: &ServiceTunnelManager,
    spec_id: &str,
    destination_id: i2pr_client::DestinationId,
    remote: &RemoteDestination,
    timeout_ms: u64,
    cancellation: &CancellationToken,
) -> Result<ConnectionId, Socks5Error> {
    manager
        .ensure_destination_active(spec_id, cancellation, timeout_ms)
        .await
        .map_err(|_| {
            Socks5Error::new(
                Socks5ErrorKind::ConnectFailure,
                "destination activation failed",
            )
        })?;
    let identity_arc = manager
        .with_destination_bridge(destination_id, |bridge| bridge.identity())
        .ok_or_else(|| Socks5Error::new(Socks5ErrorKind::ConnectFailure, "missing identity"))?;
    let connect_outcome = {
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
    let connect_outcome = match connect_outcome {
        Some(value) => value,
        None => {
            return Err(Socks5Error::new(
                Socks5ErrorKind::ConnectFailure,
                "client connect produced no outcome",
            ));
        }
    };
    let connection_id = match connect_outcome {
        Ok(ConnectOutcome::SynSent { connection_id, .. }) => connection_id,
        Ok(_) => {
            return Err(Socks5Error::new(
                Socks5ErrorKind::ConnectFailure,
                "client connect produced no SYN",
            ));
        }
        Err(_error) => {
            return Err(Socks5Error::new(
                Socks5ErrorKind::ConnectFailure,
                "client connect error",
            ));
        }
    };
    // Plan 182: kick the delivery driver so the queued SYN is
    // routed immediately instead of waiting for the fallback tick.
    manager.notify_outbound_signal(destination_id);
    wait_for_established(manager, destination_id, connection_id, timeout_ms)
        .await
        .map_err(|_| Socks5Error::new(Socks5ErrorKind::ConnectFailure, "deadline reached"))?;
    Ok(connection_id)
}

/// Bounded wait for one Streaming connection to reach
/// `Established`.
///
/// Plan 290: shared with the `socks-irc` composition.
pub async fn wait_for_established(
    manager: &ServiceTunnelManager,
    destination_id: i2pr_client::DestinationId,
    connection_id: ConnectionId,
    timeout_ms: u64,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    loop {
        let established = manager.with_destination_bridge(destination_id, |bridge| {
            bridge
                .streaming()
                .get_connection(connection_id)
                .is_some_and(|conn| matches!(conn.state(), ConnectionState::Established))
        });
        if established.unwrap_or(false) {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(Box::new(std::io::Error::other("deadline reached")));
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

/// Closes the Streaming connection after the per-connection pump
/// exits. Emits a CLOSE packet on success and a RESET on error.
///
/// Plan 290: shared with the `socks-irc` composition.
pub fn terminate_streaming(
    manager: &ServiceTunnelManager,
    destination_id: i2pr_client::DestinationId,
    connection_id: ConnectionId,
    remote: &RemoteDestination,
    reset: bool,
) {
    let now_ms = service_streaming_now_ms();
    let identity_arc = manager.with_destination_bridge(destination_id, |bridge| bridge.identity());
    let Some(identity_arc) = identity_arc else {
        return;
    };
    manager.with_destination_bridge(destination_id, |bridge| {
        let conn = bridge.streaming().get_connection(connection_id);
        let Some(conn) = conn else {
            return;
        };
        let local_port = conn.local_port();
        let remote_port = conn.remote_port();
        let request = if reset {
            bridge.streaming_mut().send_reset(
                connection_id,
                identity_arc.as_ref(),
                remote,
                local_port,
                remote_port,
                now_ms,
            )
        } else {
            bridge.streaming_mut().send_close(
                connection_id,
                identity_arc.as_ref(),
                remote,
                local_port,
                remote_port,
                now_ms,
            )
        };
        let _ = request;
    });
    manager.with_destination_bridge(destination_id, |bridge| {
        let _ = bridge.streaming_mut().remove_connection(connection_id);
    });
}

/// Per-connection SOCKS proxy entry. Negotiates SOCKS5 (greeting
/// + CONNECT) or bare SOCKS4a CONNECT through the shared Plan 290
///   negotiator, opens Streaming, sends the version-appropriate
///   success reply only after `Established`, and runs the shared
///   Plan 174 byte pump.
///
/// Plan 342: answers a SOCKS client with success and pumps the outproxy
/// route.
///
/// Split out of `run_socks5_connection` because the success reply must be
/// written only after the route exists, and because the pump endpoint has to
/// be built with the handshake prefix and the **outproxy's** remote rather
/// than the tunnel's. Getting either wrong silently corrupts the first
/// exchange on every pipelining outproxy.
///
/// 1. The success reply is written only after the route exists.
/// 2. The pump endpoint carries the outproxy's remote, not the tunnel's.
/// 3. The handshake prefix is seeded into the pump's first read.
async fn pump_via_outproxy(
    mut stream: TcpStream,
    manager: Arc<ServiceTunnelManager>,
    runtime: Arc<ServiceRuntime>,
    cancellation: CancellationToken,
    session: crate::outproxy_route::OutproxySession,
    leftover: Vec<u8>,
    via_socks4a: bool,
) -> Socks5ConnectionOutcome {
    let success: [u8; 10] = if via_socks4a {
        [0x00, 0x5a, 0, 0, 0, 0, 0, 0, 0, 0]
    } else {
        [0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0]
    };
    if stream.write_all(&success).await.is_err() {
        terminate_streaming(
            &manager,
            runtime.destination_id,
            session.connection_id,
            &session.remote,
            true,
        );
        return Socks5ConnectionOutcome::BadRequest;
    }
    if stream.flush().await.is_err() {
        terminate_streaming(
            &manager,
            runtime.destination_id,
            session.connection_id,
            &session.remote,
            true,
        );
        return Socks5ConnectionOutcome::BadRequest;
    }
    let endpoint: Arc<dyn StreamPumpEndpoint> =
        Arc::new(ServicePumpEndpoint::new_client_with_prefix(
            Arc::clone(&manager),
            runtime.destination_id,
            session.connection_id,
            session.remote.clone(),
            session.tunnel_prefix,
        ));
    let config = PumpConfig::defaults(BODY_CHUNK_BYTES);
    let result = run_stream_pump(stream, leftover, endpoint, config, cancellation).await;
    if let Err(error) = result {
        debug!(error = %error, "socks5 outproxy pump failed");
        terminate_streaming(
            &manager,
            runtime.destination_id,
            session.connection_id,
            &session.remote,
            true,
        );
        return Socks5ConnectionOutcome::BadGateway;
    }
    terminate_streaming(
        &manager,
        runtime.destination_id,
        session.connection_id,
        &session.remote,
        false,
    );
    Socks5ConnectionOutcome::TunnelClosed
}

pub async fn run_socks5_connection(
    manager: Arc<ServiceTunnelManager>,
    runtime: Arc<ServiceRuntime>,
    stream: TcpStream,
    cancellation: CancellationToken,
    options: Socks5ClientOptions,
) -> Socks5ConnectionOutcome {
    let limits = Socks5Limits::defaults();
    let mut stream = stream;
    // Steps 1-2: version dispatch + negotiation (replies for
    // rejections are emitted inside the negotiator).
    let negotiation =
        match negotiate_socks_destination(&mut stream, limits, options.proxy_auth.as_ref()).await {
            Ok(value) => value,
            Err(outcome) => return outcome,
        };
    let destination = negotiation.destination;
    let leftover = negotiation.leftover;
    let via_socks4a = negotiation.via_socks4a;
    // Version-appropriate terminal replies: SOCKS4a peers receive
    // the 8-byte 91 framing, SOCKS5 peers the 10-byte RFC 1928
    // framing.
    if !options.port_policy.allows_connect_port(destination.port) {
        reject_socks_request(
            &mut stream,
            via_socks4a,
            i2pr_service_tunnels::Socks5ReplyCode::ConnectionNotAllowed,
        )
        .await;
        return Socks5ConnectionOutcome::Forbidden;
    }
    // Plan 342: the same classification the HTTP and CONNECT paths use.
    //
    // The order is the guarantee. An `.i2p` destination goes straight down
    // and no provider is consulted; a clearnet name goes to the configured
    // outproxy; a clearnet name with no outproxy is answered
    // `HostUnreachable` and no socket is opened. There is no fourth branch,
    // and in particular nothing that "tries the tunnel's own destination
    // instead".
    let outproxy_provider = manager.outproxy_provider(&runtime.spec_id);
    let counters = manager.outproxy_counters();
    let class = match classify_client_target(
        outproxy_provider
            .as_deref()
            .map(i2pr_service_tunnels::OutproxyProvider::config),
        &destination.host,
        destination.port,
    ) {
        Ok(value) => value,
        Err(_) => {
            reject_socks_request(
                &mut stream,
                via_socks4a,
                i2pr_service_tunnels::Socks5ReplyCode::HostUnreachable,
            )
            .await;
            return Socks5ConnectionOutcome::BadGateway;
        }
    };
    // An exhaustive `match`, not two `if let`s: anything that is neither
    // `ViaOutproxy` nor `Refused` must not fall through to the direct path,
    // or a fourth outcome added later would silently take it.
    // `scripts/check-outproxy-request-path.sh` asserts this shape.
    match class {
        ClientTargetClass::ViaOutproxy(target) => {
            let provider = outproxy_provider
                .as_deref()
                .expect("a ViaOutproxy classification implies a provider");
            let route = match open_client_route(
                &manager,
                &runtime.spec_id,
                runtime.destination_id,
                Some(provider),
                target.host(),
                target.port(),
                true,
                &cancellation,
                counters,
            )
            .await
            {
                Ok(crate::outproxy_route::ClientRoute::ViaOutproxy(session)) => session,
                Ok(other) => {
                    debug!(outcome = ?other, "socks5 outproxy route was not open");
                    reject_socks_request(
                        &mut stream,
                        via_socks4a,
                        i2pr_service_tunnels::Socks5ReplyCode::HostUnreachable,
                    )
                    .await;
                    return Socks5ConnectionOutcome::BadGateway;
                }
                Err(failure) => {
                    debug!(
                        failure = failure.as_str(),
                        "socks5 outproxy route open failed"
                    );
                    reject_socks_request(
                        &mut stream,
                        via_socks4a,
                        i2pr_service_tunnels::Socks5ReplyCode::HostUnreachable,
                    )
                    .await;
                    return Socks5ConnectionOutcome::BadGateway;
                }
            };
            return pump_via_outproxy(
                stream,
                manager,
                runtime,
                cancellation,
                route,
                leftover,
                via_socks4a,
            )
            .await;
        }
        ClientTargetClass::Refused(failure) => {
            debug!(
                failure = failure.as_str(),
                "socks5 target refused before any route"
            );
            reject_socks_request(
                &mut stream,
                via_socks4a,
                i2pr_service_tunnels::Socks5ReplyCode::HostUnreachable,
            )
            .await;
            return Socks5ConnectionOutcome::BadGateway;
        }
        ClientTargetClass::Direct(_) => {}
    }
    let target = match resolve_target_for_service(&manager, runtime.destination_id, &destination) {
        Ok(value) => value,
        Err(_error) => {
            reject_socks_request(
                &mut stream,
                via_socks4a,
                i2pr_service_tunnels::Socks5ReplyCode::HostUnreachable,
            )
            .await;
            return Socks5ConnectionOutcome::BadGateway;
        }
    };
    let connect_timeout_ms = lookup_connect_timeout(&manager, &runtime.spec_id);
    let connection_id = match open_streaming(
        &manager,
        &runtime.spec_id,
        runtime.destination_id,
        &target.remote,
        connect_timeout_ms,
        &cancellation,
    )
    .await
    {
        Ok(id) => id,
        Err(error) => {
            let code = error.reply_code();
            reject_socks_request(&mut stream, via_socks4a, code).await;
            return Socks5ConnectionOutcome::BadGateway;
        }
    };
    if via_socks4a {
        if let Err(error) = write_socks4a_reply(&mut stream, true).await {
            debug!(error = %error, "socks4a success reply write failed");
            terminate_streaming(
                &manager,
                runtime.destination_id,
                connection_id,
                &target.remote,
                true,
            );
            return Socks5ConnectionOutcome::BadRequest;
        }
    } else {
        let success = build_socks5_reply(i2pr_service_tunnels::Socks5ReplyCode::Success);
        if let Err(error) = stream.write_all(&success).await {
            debug!(error = %error, "socks5 success reply write failed");
            terminate_streaming(
                &manager,
                runtime.destination_id,
                connection_id,
                &target.remote,
                true,
            );
            return Socks5ConnectionOutcome::BadRequest;
        }
    }
    if let Err(error) = stream.flush().await {
        debug!(error = %error, "socks5 success reply flush failed");
        terminate_streaming(
            &manager,
            runtime.destination_id,
            connection_id,
            &target.remote,
            true,
        );
        return Socks5ConnectionOutcome::BadRequest;
    }
    let endpoint: Arc<dyn StreamPumpEndpoint> = Arc::new(ServicePumpEndpoint::new_client(
        Arc::clone(&manager),
        runtime.destination_id,
        connection_id,
        target.remote.clone(),
    ));
    let config = PumpConfig::defaults(BODY_CHUNK_BYTES);
    let result = run_stream_pump(stream, leftover, endpoint, config, cancellation).await;
    if let Err(error) = result {
        debug!(error = %error, "socks5 pump failed");
        terminate_streaming(
            &manager,
            runtime.destination_id,
            connection_id,
            &target.remote,
            true,
        );
        return Socks5ConnectionOutcome::BadGateway;
    }
    terminate_streaming(
        &manager,
        runtime.destination_id,
        connection_id,
        &target.remote,
        false,
    );
    Socks5ConnectionOutcome::TunnelClosed
}

/// Looks up the configured connect deadline for one service.
///
/// Plan 290: shared with the `socks-irc` composition.
pub fn lookup_connect_timeout(manager: &ServiceTunnelManager, spec_id: &str) -> u64 {
    manager
        .config()
        .specs
        .tunnels
        .iter()
        .find(|spec| spec.id.as_str() == spec_id)
        .map(|spec| spec.timeouts.connect_timeout_ms)
        .unwrap_or(10_000)
}

/// Runs the per-service supervisor loop for a SOCKS5 client tunnel.
pub async fn run_socks5_client_loop(
    manager: &Arc<ServiceTunnelManager>,
    runtime: &Arc<ServiceRuntime>,
    spec: &i2pr_service_tunnels::ServiceTunnelSpec,
    cancellation: &CancellationToken,
) -> Result<(), crate::service_tunnels::ServiceTunnelError> {
    let listener = runtime.client_listener.as_ref().ok_or_else(|| {
        crate::service_tunnels::ServiceTunnelError::InvalidConfig(
            "socks5 client tunnel missing loopback listener".to_owned(),
        )
    })?;
    let local_addr = listener
        .local_addr()
        .map_err(|error| crate::service_tunnels::ServiceTunnelError::Bind(error.to_string()))?;
    tracing::info!(
        service = %spec.id.as_str(),
        bind = %local_addr,
        "socks5 client tunnel bound loopback listener"
    );
    let options = spec.socks5_options.clone().unwrap_or_default();
    let drain_cancel = runtime.admission_cancellation_token();
    loop {
        let accept = tokio::select! {
            biased;
            _ = cancellation.cancelled() => break,
            // Plan 289: drained runtimes stop accepting promptly (see
            // `run_client_loop`).
            _ = drain_cancel.cancelled() => break,
            accept = listener.accept() => accept,
        };
        let (stream, _peer) = match accept {
            Ok(value) => value,
            Err(error) => {
                warn!(
                    service = %spec.id.as_str(),
                    error = %error,
                    "socks5 client listener accept failed"
                );
                continue;
            }
        };
        let aggregate_permit: Option<tokio::sync::OwnedSemaphorePermit> =
            manager.aggregate_permit().try_acquire_owned().ok();
        let Some(permit_for_task) = aggregate_permit else {
            warn!(
                service = %runtime.spec_id,
                "socks5 client tunnel aggregate ceiling reached; rejecting connection"
            );
            runtime.failed_connects.fetch_add(1, Ordering::Relaxed);
            drop(stream);
            continue;
        };
        runtime.active_connections.fetch_add(1, Ordering::Relaxed);
        let manager_for_task = Arc::clone(manager);
        let runtime_for_task = Arc::clone(runtime);
        let options_for_task = options.clone();
        let cancellation_for_task = cancellation.clone();
        let spec_id_for_log = runtime.spec_id.clone();
        tokio::spawn(async move {
            // Plan 182: hold the aggregate slot for the connection
            // lifetime; the previous let-else binding released it
            // at spawn time so the ceiling never engaged.
            let _permit_for_task = permit_for_task;
            let outcome = run_socks5_connection(
                manager_for_task,
                runtime_for_task.clone(),
                stream,
                cancellation_for_task,
                options_for_task,
            )
            .await;
            if matches!(
                outcome,
                Socks5ConnectionOutcome::BadGateway | Socks5ConnectionOutcome::Forbidden
            ) {
                runtime_for_task
                    .failed_connects
                    .fetch_add(1, Ordering::Relaxed);
            }
            runtime_for_task.connection_finished_now();
            debug!(
                service = %spec_id_for_log,
                ?outcome,
                "socks5 client connection finished"
            );
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_chunk_size_is_bounded() {
        // Static-only check: ensure the read chunk constants stay
        // within the runtime-neutral limits.
        const { assert!(BODY_CHUNK_BYTES <= 32 * 1024) };
        assert!(GREETING_READ_DEADLINE.as_secs() >= 5);
        assert!(REQUEST_READ_DEADLINE.as_secs() >= 5);
    }
}
