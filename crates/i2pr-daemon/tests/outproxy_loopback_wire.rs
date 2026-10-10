//! Plan 342: the self-composed in-tree loopback outproxy wire lane.
//!
//! # What this is, and what it is not
//!
//! This is **loopback evidence, not interoperability**, decided with Plan 339
//! and matching the `sam_stream_self_composed` precedent. Nothing here is
//! vendored, patched, or borrowed from an external router. Every socket binds
//! `127.0.0.1:0`.
//!
//! It is the runtime half of Plan 342 that
//! `scripts/check-outproxy-request-path.sh` cannot supply. That guard pins the
//! *shape* of the request paths — that each classifies through the one
//! classifier, that a refusal returns, that the handshake prefix reaches the
//! pump's inbound direction. A shape can be right while the path is dead, and
//! that is exactly what step 3 shipped: every request-target grammar in the
//! tree refused a clearnet authority at the parser, so
//! `ClientTargetClass::ViaOutproxy` was unreachable in production while its
//! unit rows passed. This lane is what makes "a clearnet target actually
//! arrives at an outproxy and its bytes come back" an executed claim.
//!
//! # The composition
//!
//! ```text
//! local test client
//!   -> the client tunnel's own loopback listener            (127.0.0.1:0)
//!   -> bounded HTTP/1.1 or SOCKS5 head read
//!   -> the tunnel's TargetPolicy + classify_client_target
//!   -> StreamRoutingProvider: Streaming to the outproxy destination
//!   -> `outproxy-svc`, a service *server* tunnel whose
//!      ServerTarget is LoopbackTcp(the outproxy fixture)    (127.0.0.1:0)
//!   -> the outproxy fixture, which speaks the server side of an I2P outproxy
//!   -> a loopback origin fixture
//! ```
//!
//! The outproxy is reached through **Streaming**, not through a loopback
//! shortcut: it is an ordinary local service destination, and `resolve_reference`
//! finds it through `lookup_local_service_destination` exactly as it would find
//! any I2P outproxy destination. That is the property under test — the route is
//! opened the way production opens it.
//!
//! # Why the fixture never opens a clearnet socket
//!
//! The outproxy fixture maps the authority it was asked for onto a **loopback**
//! origin it was given at construction. It never resolves a name and never
//! connects to anything but `127.0.0.1`. So the lane can assert "a clearnet
//! request succeeded through an outproxy" without the test process itself ever
//! holding a clearnet capability — which would contradict Plan 342 invariant 1
//! at exactly the layer that is supposed to be enforcing it.
//!
//! # What each row proves
//!
//! # Plan 376: live failover and a request across a restart
//!
//! The rows below close the two absences Plan 342 deliberately left. They
//! extend the composition above rather than replacing it: the same
//! real-Streaming route, the same service *server* tunnel standing in for the
//! I2P outproxy destination, the same classifier, the same route owner. No
//! socket here is a clearnet socket, the fixtures never resolve a name, and
//! the targets stay opaque labels handed to the outproxy.
//!
//! ## Ordered failover, not load rotation
//!
//! The provider contract is **ordered failover within one request**, not
//! round-robin between requests. `OutproxyList::select(attempt)` maps an
//! attempt index to an endpoint by `attempt % len`, and `open_via_outproxy`
//! walks attempts from zero until one succeeds, so two requests against the
//! same tunnel both begin at the operator's first entry. That is deliberate:
//! the operator's order is intent. Between-request load rotation is **not**
//! claimed, because no provider contract claims it, and inventing it to satisfy
//! historical wording would have been the wrong fix.
//!
//! ## What Plan 376 changed to make these rows reachable
//!
//! Three defects were found while writing them, all in production code and all
//! invisible to the Plan 342 rows because each row used exactly one endpoint:
//!
//! 1. **The declared backoff was never applied.** `OutproxyPolicy::backoff_ms`
//!    had existed since Plan 342 and the route owner never called it, so a
//!    retryable failure was retried immediately and a whole list of dead
//!    outproxies was hammered in one burst.
//! 2. **The attempt budget was hardcoded to 2** regardless of `ProxyList`
//!    length. An operator configuring four outproxies got two attempts, and
//!    entries three and four were silently never tried — the control surface
//!    accepted the list, echoed it back, and ignored its tail. The budget is now
//!    the list length clamped to `[1, MAX_OUTPROXY_ATTEMPTS]`.
//! 3. **A retryable failure that a later attempt went on to succeed was never
//!    counted.** Counters recorded only a *terminal* reason, so an operator
//!    whose first outproxy was permanently dead saw `target_unreachable: 0`
//!    forever while every request quietly succeeded through the second. The
//!    counters are the only surface that carries this diagnosis.
//!
//! ## The restart boundary, precisely
//!
//! A generation owns a manager, a control state, a supervisor scope, and a
//! cancellation token. A restart drops the whole generation and builds another
//! over the **same data directory**; nothing else crosses the boundary. The
//! second generation is brought up by `TunnelControlState::startup` — the
//! product's own boot path, which reloads the published generation, revalidates
//! every stored definition, reconciles the shared manager, and reinstalls the
//! outproxy provider registry. The test never re-applies the operator's
//! `create`, so a provider that exists after recovery can only have come from
//! disk plus the router-bound secret owner.
//!
//! That is deliberately stronger than the serialization round trip Plan 342
//! proved, which only wrote and read a stored definition. It is **not** a
//! cross-process `exec`: this lane has no child process and no config file, so
//! a cross-process restart is not claimed.
//!
//! # What this lane does **not** prove
//!
//! Stated here rather than only in a closure record, because a reader who runs
//! this lane must not have to open another file to learn its reach.
//!
//! - **Interoperability with a real outproxy.** Nothing here ran against Java
//!   I2P or i2pd. This is loopback evidence; the security property under test is
//!   that every clearnet request is carried only through an I2P Streaming route.
//! - **A cross-process restart.** See above: every in-memory owner is torn down
//!   and rebuilt from persisted state, in one process.
//! - **Between-request load rotation.** Deliberately not implemented and not
//!   claimed; see "Ordered failover" above.
//! - **An operator-independence guarantee for the credential.** A configured
//!   `ProxyList` of *n* outproxies is offered the same credential *n* times,
//!   because a 407 is retryable under the frozen taxonomy. That is the current
//!   explicit policy and
//!   `plan376_authentication_rejection_is_retried_at_the_next_endpoint_by_the_current_policy`
//!   pins it, but an operator with two outproxies may not expect it. It is a
//!   consequence of the operator's own list rather than of the router, and it
//!   is recorded as a finding rather than silently accepted.
//!
//! | row | claim |
//! |---|---|
//! | `plan342_i2p_authority_bypasses_the_outproxy` | an `.i2p` target is routed to the tunnel's own destination and **no outproxy is contacted** |
//! | `plan342_clearnet_target_succeeds_through_the_loopback_outproxy` | a clearnet target reaches the outproxy and its bytes come back, prefix included |
//! | `plan342_clearnet_target_without_a_provider_is_refused_and_opens_nothing` | no provider means refusal, not a direct socket |
//! | `plan342_outproxy_auth_is_presented_and_never_echoed` | the credential is presented as a header and never appears to the client |
//! | `plan342_socks5_request_is_carried_by_the_outproxy` | the SOCKS family reaches the same route |
//! | `plan342_http_forward_request_is_carried_by_the_outproxy` | the forward path carries the clearnet authority in `Host:` |
//! | `plan342_malformed_block_is_refused_before_any_listener` | `ProxyList` / `OutproxyType` grammar holds at the boundary |
//! | `plan342_unreachable_outproxy_fails_typed_and_bounded` | an unreachable outproxy fails typed, bounded, and counted |
//! | `plan376_http_connect_fails_over_to_the_second_outproxy` | one HTTP CONNECT survives a dead first endpoint; two attempts, one handshake |
//! | `plan376_socks5_fails_over_to_the_second_outproxy` | the same failover over SOCKS5, so it is the route owner's property and not the HTTP grammar's |
//! | `plan376_an_upstream_refusal_is_retried_at_the_second_outproxy` | a *reachable* outproxy answering 502 is `TargetUnreachable`, which is retryable |
//! | `plan376_authentication_rejection_is_retried_at_the_next_endpoint_by_the_current_policy` | a 407 is retried at the next endpoint, and the credential is offered to both — pinned, not hidden |
//! | `plan376_the_attempt_budget_is_the_list_length_and_exhaustion_is_typed` | three endpoints means three attempts, exhaustion is typed, and nothing is routed directly |
//! | `plan376_a_routed_request_survives_a_product_restart` | the block, the sealed password, the provider registry, and application data all survive a restart |
//! | `plan376_after_restart_i2p_traffic_bypasses_and_removing_the_provider_fails_closed` | `.i2p` still bypasses after a restart; removing the provider fails closed |
//! | `plan376_a_copied_config_without_the_router_secret_cannot_recover_the_credential` | a copied config without the matching router secret cannot present a credential |

#![forbid(unsafe_code)]
#![allow(clippy::too_many_lines)]

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use i2pr_i2pcontrol::tunnel::{TunnelAction, TunnelType};
use i2pr_i2pcontrol::tunnel_request::TunnelManagerRequest;
use i2pr_runtime::{CancellationToken, ChildFailurePolicy, ChildScope};
use i2pr_service_tunnels::outbound_secret::RouterSecretOwner;
use i2pr_service_tunnels::{
    DestinationRef, ServerTarget, ServiceTimeouts, ServiceTunnelId, ServiceTunnelKind,
    ServiceTunnelSet, ServiceTunnelSpec, StaticAliasTable,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use i2pr_daemon::i2pcontrol_tunnels::{ControlStore, TunnelControlState};
use i2pr_daemon::service_tunnels::{ServiceTunnelManager, ServiceTunnelManagerConfig};

// ---------------------------------------------------------------------------
// The loopback outproxy fixture
// ---------------------------------------------------------------------------

/// What the fixture observed for one tunnelled request.
#[derive(Clone, Debug, Default)]
struct Observed {
    /// Every `CONNECT authority:port` the fixture was asked to reach.
    authorities: Vec<String>,
    /// Whether any request carried a `Proxy-Authorization` header.
    saw_authorization: bool,
}

/// A loopback origin: echoes every byte back with a stable prefix.
struct LoopbackOrigin {
    address: SocketAddr,
}

async fn start_origin() -> LoopbackOrigin {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("origin bind");
    let address = listener.local_addr().expect("origin addr");
    tokio::spawn(async move {
        // Serve a bounded number of connections; a leaked task would outlive the
        // test, and every row asserts on bytes so one connection per row is
        // enough.
        while let Ok((mut stream, _)) = listener.accept().await {
            tokio::spawn(async move {
                let mut chunk = [0_u8; 4096];
                loop {
                    match tokio::time::timeout(Duration::from_secs(10), stream.read(&mut chunk))
                        .await
                    {
                        Ok(Ok(0)) | Ok(Err(_)) | Err(_) => break,
                        Ok(Ok(n)) => {
                            if stream.write_all(&chunk[..n]).await.is_err() {
                                break;
                            }
                        }
                    }
                }
            });
        }
    });
    LoopbackOrigin { address }
}

/// How the fixture should answer.
#[derive(Clone, Copy)]
struct FixtureBehaviour {
    /// Require a `Proxy-Authorization` header, and require this exact value.
    required_authorization: Option<&'static str>,
    /// Bytes to pipeline into the handshake response, before the tunnelled
    /// stream. Exercises the pump's inbound prefix.
    pipeline_prefix: &'static [u8],
    /// Close the connection immediately instead of answering, to drive the
    /// failover row.
    refuse_immediately: bool,
    /// Answer `502 Bad Gateway` after a complete, well-formed request head.
    ///
    /// This is the *upstream refusal* stage: the outproxy was reached, the
    /// handshake bytes arrived, and the outproxy declined to reach the target.
    /// It is a different failure from `refuse_immediately` (which never gets
    /// that far) and it maps to `TargetUnreachable`, which is retryable.
    refuse_upstream: bool,
}

impl Default for FixtureBehaviour {
    fn default() -> Self {
        Self {
            required_authorization: None,
            pipeline_prefix: b"",
            refuse_immediately: false,
            refuse_upstream: false,
        }
    }
}

/// A loopback outproxy speaking the **server** side of an I2P outproxy's
/// HTTP CONNECT dialect.
///
/// It answers `CONNECT authority:port`, optionally pipelines bytes into its own
/// handshake response, and then relays to a loopback origin. It never resolves a
/// name and never opens a non-loopback socket — see the module header.
struct LoopbackOutproxy {
    address: SocketAddr,
    observed: Arc<Mutex<Observed>>,
}

async fn start_outproxy(origin: SocketAddr, behaviour: FixtureBehaviour) -> LoopbackOutproxy {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("outproxy bind");
    let address = listener.local_addr().expect("outproxy addr");
    let observed = Arc::new(Mutex::new(Observed::default()));
    let observed_for_task = Arc::clone(&observed);
    tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            let observed = Arc::clone(&observed_for_task);
            tokio::spawn(async move {
                serve_outproxy_connection(&mut stream, origin, behaviour, observed).await;
            });
        }
    });
    LoopbackOutproxy { address, observed }
}

async fn serve_outproxy_connection(
    stream: &mut TcpStream,
    origin: SocketAddr,
    behaviour: FixtureBehaviour,
    observed: Arc<Mutex<Observed>>,
) {
    if behaviour.refuse_immediately {
        // A TCP-level close: the daemon sees an unreachable outproxy and, if it
        // has another, rotates to it.
        return;
    }
    let head = match read_head_bounded(stream).await {
        Some(value) => value,
        None => return,
    };
    let text = String::from_utf8_lossy(&head);
    let mut lines = text.split("\r\n");
    let request_line = lines.next().unwrap_or_default();
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let authority = parts.next().unwrap_or_default();
    if method != "CONNECT" {
        let _ = stream
            .write_all(b"HTTP/1.1 405 Method Not Allowed\r\n\r\n")
            .await;
        return;
    }
    let authorization = lines.find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.trim()
            .eq_ignore_ascii_case("proxy-authorization")
            .then(|| value.trim().to_owned())
    });
    let authorized = match behaviour.required_authorization {
        None => true,
        Some(expected) => authorization.as_deref() == Some(expected),
    };
    {
        let mut guard = observed.lock().expect("observed lock");
        guard.authorities.push(authority.to_owned());
        guard.saw_authorization |= authorization.is_some();
    }
    if !authorized {
        let _ = stream
            .write_all(b"HTTP/1.1 407 Proxy Authentication Required\r\n\r\n")
            .await;
        return;
    }
    if behaviour.refuse_upstream {
        // A well-formed refusal from an outproxy that was reached and
        // understood. The route owner reads this as `TargetUnreachable`.
        let _ = stream.write_all(b"HTTP/1.1 502 Bad Gateway\r\n\r\n").await;
        return;
    }
    // The 200 and the pipelined prefix go out in one write, so the daemon's
    // handshake read can legitimately receive both in the same burst.
    let mut response = Vec::from(&b"HTTP/1.1 200 Connection Established\r\n\r\n"[..]);
    response.extend_from_slice(behaviour.pipeline_prefix);
    if stream.write_all(&response).await.is_err() {
        return;
    }
    let _ = stream.flush().await;
    // The tunnelled stream is a byte pipe to the origin. The origin is
    // loopback; the fixture never resolves the authority it was asked for.
    let Ok(mut upstream) = TcpStream::connect(origin).await else {
        return;
    };
    let mut downstream = stream;
    let _ = tokio::io::copy_bidirectional(&mut downstream, &mut upstream).await;
}

/// Reads exactly `expected` bytes or fails the row on a deadline.
async fn read_exact_bounded(stream: &mut TcpStream, expected: usize) -> Vec<u8> {
    let mut out = vec![0_u8; expected];
    let mut filled = 0_usize;
    let deadline = Instant::now() + Duration::from_secs(15);
    while filled < expected {
        assert!(
            Instant::now() < deadline,
            "timed out reading {expected} bytes (got {filled})"
        );
        match tokio::time::timeout(Duration::from_secs(5), stream.read(&mut out[filled..])).await {
            Ok(Ok(0)) => panic!("EOF after {filled}/{expected} bytes"),
            Ok(Ok(n)) => filled += n,
            Ok(Err(error)) => panic!("read error after {filled}/{expected}: {error}"),
            Err(_) => panic!("read timed out after {filled}/{expected} bytes"),
        }
    }
    out
}

/// Reads one bounded HTTP head (up to the blank line) or gives up.
async fn read_head_bounded(stream: &mut TcpStream) -> Option<Vec<u8>> {
    let mut buffer = Vec::with_capacity(2048);
    let mut chunk = [0_u8; 1024];
    loop {
        match tokio::time::timeout(Duration::from_secs(10), stream.read(&mut chunk)).await {
            Ok(Ok(0)) | Ok(Err(_)) | Err(_) => return None,
            Ok(Ok(n)) => buffer.extend_from_slice(&chunk[..n]),
        }
        if buffer.windows(4).any(|w| w == b"\r\n\r\n") {
            return Some(buffer);
        }
        // Bounded: a fixture that accumulates forever is itself a leak.
        if buffer.len() > 64 * 1024 {
            return None;
        }
    }
}

// ---------------------------------------------------------------------------
// Service tunnel composition
// ---------------------------------------------------------------------------

fn temp_data_dir(name: &str) -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix(name)
        .tempdir()
        .expect("tempdir")
}

/// A server tunnel whose Streaming destination delivers to `target`.
fn server_spec(id: &str, target: SocketAddr) -> ServiceTunnelSpec {
    ServiceTunnelSpec {
        id: ServiceTunnelId::parse(id).expect("id"),
        kind: ServiceTunnelKind::GenericServer,
        enabled: true,
        listener: None,
        target: Some(ServerTarget::LoopbackTcp(target)),
        targets: Vec::new(),
        destination: None,
        policy: i2pr_service_tunnels::DestinationPolicy::Dedicated,
        inbound_port: None,
        max_connections: 8,
        max_buffered_bytes_per_direction: 65_536,
        timeouts: ServiceTimeouts::defaults(),
        shaping: i2pr_service_tunnels::TunnelShaping::balanced(),
        streaming_interactive: false,
        idle: i2pr_service_tunnels::IdlePolicy::disabled(),
        access: i2pr_service_tunnels::ServerAccessPolicy::default(),
        unique_local_address: false,
        multihoming: false,
        reply_bundling: false,
        use_ssl: false,
        http_policy: i2pr_service_tunnels::HttpServerPolicy::default(),
        http_options: None,
        socks5_options: None,
        irc_options: None,
        connect_options: None,
        streamr_options: None,
    }
}

fn manager(
    data_dir: &Path,
    specs: Vec<ServiceTunnelSpec>,
    aliases: StaticAliasTable,
) -> Arc<ServiceTunnelManager> {
    Arc::new(
        ServiceTunnelManager::new(ServiceTunnelManagerConfig {
            data_dir: data_dir.to_path_buf(),
            aggregate_connection_ceiling: 64,
            per_service_connection_ceiling: 16,
            specs: Arc::new(ServiceTunnelSet { tunnels: specs }),
            aliases: Arc::new(aliases),
        })
        .expect("manager builds"),
    )
}

/// Starts the per-service supervisors under an explicit child-failure policy.
///
/// The policy is a parameter because the failover row deliberately hosts a
/// *dead* outproxy endpoint. Under [`ChildFailurePolicy::FailParent`] that
/// endpoint's server tunnel failing takes down the whole scope, which cancels
/// the very cancellation token the route opener checks between attempts -- so
/// the rotation is aborted by the test harness rather than exercised.
/// [`ChildFailurePolicy::DegradeParent`] is also the more honest model for the
/// property under test: one unreachable outproxy must degrade, not fail the
/// owning service.
async fn start_supervisors_with(
    manager: &Arc<ServiceTunnelManager>,
    policy: ChildFailurePolicy,
) -> (ChildScope, CancellationToken) {
    let runtimes = manager.prepare().await.expect("prepare");
    let cancel = CancellationToken::new();
    let scope = ChildScope::for_test(&cancel, policy);
    manager
        .start_supervisors(runtimes, &scope, cancel.clone())
        .expect("supervisors started");
    (scope, cancel)
}

async fn start_supervisors(manager: &Arc<ServiceTunnelManager>) -> (ChildScope, CancellationToken) {
    start_supervisors_with(manager, ChildFailurePolicy::FailParent).await
}

/// Captures the persistent server destination material for one server spec,
/// using a transient manager over the same data directory so the identity
/// carries into the real manager.
///
/// This is the `service_tunnels_local_roundtrip` pairing trick: a probe manager
/// writes the destination to the shared data directory, and the real manager
/// reads the same identity back. It is how a client tunnel comes to point at a
/// destination this process actually owns.
async fn capture_destination(data_dir: &Path, id: &str, target: SocketAddr) -> String {
    let probe = manager(
        data_dir,
        vec![server_spec(id, target)],
        StaticAliasTable::new(),
    );
    probe.prepare().await.expect("probe prepare");
    let material = probe
        .service_destination_b64(id)
        .expect("server destination material");
    drop(probe);
    material
}

// ---------------------------------------------------------------------------
// Control-plane request helpers
// ---------------------------------------------------------------------------

fn create_request(
    name: &str,
    tunnel_type: TunnelType,
    options: BTreeMap<String, String>,
) -> TunnelManagerRequest {
    TunnelManagerRequest {
        action: TunnelAction::Create,
        all: false,
        name: Some(name.to_owned()),
        tunnel_type: Some(tunnel_type),
        new_name: None,
        options,
    }
}

/// An `edit` request carrying only the options that should change. The
/// outproxy block is absent, which is how the fail-closed row removes a
/// provider rather than creating a second tunnel.
fn edit_request(name: &str, options: BTreeMap<String, String>) -> TunnelManagerRequest {
    TunnelManagerRequest {
        action: TunnelAction::Edit,
        all: false,
        name: Some(name.to_owned()),
        tunnel_type: None,
        new_name: None,
        options,
    }
}

/// The client-tunnel options map, with or without the complete outproxy block.
///
/// The block is all-or-none by Plan 342's rule, so it is built here whole or
/// not at all. `extra_proxy` lets the failover row name a dead outproxy first.
fn client_options(
    kind: TunnelType,
    target_destination: &str,
    outproxy_list: Option<&str>,
    credential: Option<(&str, &str)>,
) -> BTreeMap<String, String> {
    let mut options = BTreeMap::new();
    options.insert(
        "target_destination".to_owned(),
        target_destination.to_owned(),
    );
    options.insert("listen_port".to_owned(), "0".to_owned());
    let _ = kind;
    let Some(list) = outproxy_list else {
        return options;
    };
    options.insert("proxy_list".to_owned(), list.to_owned());
    options.insert("use_outproxy_plugin".to_owned(), "true".to_owned());
    options.insert("outproxy_type".to_owned(), "http".to_owned());
    // The tunnelled allowlist must be a subset of the list, so the whole list
    // opts in. A CONNECT request is a tunnelled request.
    options.insert("ssl_proxies".to_owned(), list.to_owned());
    match credential {
        None => {
            options.insert("outproxy_auth".to_owned(), "false".to_owned());
            options.insert("outproxy_username".to_owned(), String::new());
            options.insert("outproxy_password".to_owned(), String::new());
        }
        Some((user, password)) => {
            options.insert("outproxy_auth".to_owned(), "true".to_owned());
            options.insert("outproxy_username".to_owned(), user.to_owned());
            options.insert("outproxy_password".to_owned(), password.to_owned());
        }
    }
    options
}

/// A router-bound secret owner, so a credential row cannot pass against a stub
/// that stores plaintext.
fn secret_store() -> Arc<dyn RouterSecretOwner> {
    let mut rng = i2pr_crypto::OsRng;
    let bundle = i2pr_crypto::RouterIdentityBundle::generate(&mut rng).expect("identity bundle");
    Arc::new(
        i2pr_daemon::outbound_secret::RouterBoundOutboundSecrets::from_router_identity(&bundle)
            .expect("router-bound secret owner"),
    )
}

/// A `TunnelControlState` over the shared manager, so the outproxy provider is
/// installed by the real reconciliation rather than poked in by the test.
///
/// `startup` matters and is easy to get wrong. A control commit publishes
/// `startup ∪ control-owned definitions`, so a tunnel that is only in the
/// *manager's* spec set is dropped by the first `create`. The outproxy
/// endpoint is therefore startup-owned — which is also how it would be in a
/// real deployment, where the endpoint and the proxy client are separate
/// tunnels with separate lifecycles.
fn control(
    data_dir: &Path,
    shared: &Arc<ServiceTunnelManager>,
    startup: Vec<ServiceTunnelSpec>,
) -> TunnelControlState {
    TunnelControlState::new(
        ControlStore::open(data_dir).expect("control store"),
        ServiceTunnelSet { tunnels: startup },
        Arc::clone(shared),
        secret_store(),
    )
}

/// A startup-owned server spec: `enabled`, so the manager builds its runtime.
fn startup_server(id: &str, target: SocketAddr) -> ServiceTunnelSpec {
    let mut spec = server_spec(id, target);
    spec.enabled = true;
    spec
}

fn b32(byte: u8) -> String {
    format!(
        "{}.b32.i2p",
        i2pr_service_tunnels::encode_b32_label(&[byte; 32])
    )
}

/// The `.b32.i2p` label for a server tunnel's persisted destination.
///
/// `ProxyList` entries are I2P destinations or `.i2p` names, never base64
/// material, so the lane derives the label the way a control client would: from
/// the destination's own hash.
fn b32_label_of(material: &str) -> String {
    let bytes =
        i2pr_api::sam::base64::decode(material, 4096).expect("destination material decodes");
    let destination = i2pr_proto::Destination::decode(&bytes, 4096).expect("destination decodes");
    format!(
        "{}.b32.i2p",
        i2pr_service_tunnels::encode_b32_label(
            destination.hash().expect("destination hash").as_bytes()
        )
    )
}

// ---------------------------------------------------------------------------
// Client-side helpers
// ---------------------------------------------------------------------------

/// Connects to the tunnel's loopback listener and issues one `CONNECT`.
async fn connect_tunnel(listener: SocketAddr, authority: &str) -> TcpStream {
    let mut stream = TcpStream::connect(listener)
        .await
        .expect("listener connect");
    let request = format!("CONNECT {authority} HTTP/1.1\r\nHost: {authority}\r\n\r\n");
    stream
        .write_all(request.as_bytes())
        .await
        .expect("write request");
    stream.flush().await.expect("flush request");
    stream
}

async fn read_some(stream: &mut TcpStream) -> Vec<u8> {
    let mut out = Vec::new();
    let mut chunk = [0_u8; 1024];
    // Long enough to outlast the outproxy handshake deadline and every retry
    // the policy permits. A shorter read gave up while the route was still
    // legitimately in flight and reported the empty result as a refusal.
    let deadline = Instant::now() + Duration::from_secs(45);
    // Read until the deadline or until nothing more arrives for a short
    // quiet period. A pipelined prefix legitimately arrives in the same burst
    // as the handshake response, but the pump's inbound direction and this
    // socket are separate streams, so it may also land one segment later --
    // a single `read` is not a sufficient observation.
    while Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_millis(400), stream.read(&mut chunk)).await {
            Ok(Ok(0)) => break,
            Ok(Err(_)) => break,
            Ok(Ok(n)) => out.extend_from_slice(&chunk[..n]),
            Err(_) => {
                if !out.is_empty() {
                    break;
                }
            }
        }
    }
    out
}

#[tokio::test(flavor = "current_thread")]
async fn plan342_clearnet_target_succeeds_through_the_loopback_outproxy() {
    let directory = temp_data_dir("opx-wire-carry");
    let origin = start_origin().await;
    // The outproxy pipelines bytes into its own handshake response, so the
    // pump's inbound prefix has something to carry.
    let outproxy = start_outproxy(
        origin.address,
        FixtureBehaviour {
            pipeline_prefix: b"prefixed-by-outproxy",
            ..FixtureBehaviour::default()
        },
    )
    .await;

    let outproxy_material =
        capture_destination(directory.path(), "outproxy-svc", outproxy.address).await;
    let outproxy_label = b32_label_of(&outproxy_material);
    let shared = manager(
        directory.path(),
        vec![server_spec("outproxy-svc", outproxy.address)],
        StaticAliasTable::new(),
    );
    let state = control(
        directory.path(),
        &shared,
        vec![startup_server("outproxy-svc", outproxy.address)],
    );
    state
        .create(&create_request(
            "tunnel-a",
            TunnelType::ConnectClient,
            client_options(
                TunnelType::ConnectClient,
                &outproxy_material,
                Some(&outproxy_label),
                None,
            ),
        ))
        .await
        .expect("the complete block is accepted");

    assert!(
        shared.outproxy_provider("tunnel-a").is_some(),
        "the reconciliation must have installed a provider before any request is served"
    );
    let (_scope, _cancel) = start_supervisors(&shared).await;
    assert!(
        shared.outproxy_provider("tunnel-a").is_some(),
        "preparing the manager must not drop the installed provider; if it did, the tunnel \
         would silently revert to a strict parser and every clearnet request would fail with \
         a parse error instead of a route failure"
    );
    let listener = shared
        .client_listener_address("tunnel-a")
        .expect("the client tunnel has a loopback listener");

    let mut stream = connect_tunnel(listener, "example.com:443").await;
    let head = read_some(&mut stream).await;
    let text = String::from_utf8_lossy(&head);
    assert!(
        text.starts_with("HTTP/1.1 200"),
        "the local client must be told 200 only after the handshake succeeded; \
         counters: {:?}; observed: {:?}; body: {text:?}",
        shared.outproxy_counters().lock().expect("counters"),
        outproxy.observed.lock().expect("observed")
    );
    assert!(
        text.contains("prefixed-by-outproxy"),
        "the outproxy's pipelined bytes must reach the local client before the tunnelled stream; got {text:?}"
    );

    // And the outproxy really was the thing that was asked.
    let observed = outproxy.observed.lock().expect("observed lock");
    assert_eq!(
        observed.authorities,
        vec!["example.com:443".to_owned()],
        "the outproxy must have been asked for exactly the requested authority"
    );
}

/// An `.i2p` authority is routed to the tunnel's own destination and **no
/// outproxy is contacted at all** — not even to check whether one exists.
#[tokio::test(flavor = "current_thread")]
async fn plan342_i2p_authority_bypasses_the_outproxy() {
    let directory = temp_data_dir("opx-wire-bypass");
    let direct_origin = start_origin().await;
    let outproxy_origin = start_origin().await;
    let outproxy = start_outproxy(outproxy_origin.address, FixtureBehaviour::default()).await;

    let direct_material =
        capture_destination(directory.path(), "direct-svc", direct_origin.address).await;
    let outproxy_material =
        capture_destination(directory.path(), "outproxy-svc", outproxy.address).await;

    // The alias is what gives the request its `.i2p` spelling, and it points at
    // the *direct* destination — so a correct `.i2p` row lands on the direct
    // fixture and an incorrect one lands on the outproxy fixture.
    let mut aliases = StaticAliasTable::new();
    aliases
        .insert(
            "direct.example.i2p",
            DestinationRef::ConfiguredDestination(direct_material.clone()),
        )
        .expect("alias insert");

    let shared = manager(
        directory.path(),
        vec![
            server_spec("direct-svc", direct_origin.address),
            server_spec("outproxy-svc", outproxy.address),
        ],
        aliases,
    );
    let state = control(
        directory.path(),
        &shared,
        vec![
            startup_server("direct-svc", direct_origin.address),
            startup_server("outproxy-svc", outproxy.address),
        ],
    );
    state
        .create(&create_request(
            "tunnel-a",
            TunnelType::ConnectClient,
            client_options(
                TunnelType::ConnectClient,
                &direct_material,
                Some(&b32_label_of(&outproxy_material)),
                None,
            ),
        ))
        .await
        .expect("create");

    let (_scope, _cancel) = start_supervisors(&shared).await;
    let listener = shared
        .client_listener_address("tunnel-a")
        .expect("listener");

    let mut stream = connect_tunnel(listener, "direct.example.i2p:443").await;
    let head = read_some(&mut stream).await;
    assert!(
        head.starts_with(b"HTTP/1.1 200"),
        "an .i2p CONNECT must be established; got {}",
        String::from_utf8_lossy(&head)
    );
    // The direct fixture echoes, so a byte written after establishment must
    // come back. This is what proves the `.i2p` target was routed to the
    // tunnel's own destination rather than refused.
    stream.write_all(b"ping").await.expect("write");
    let echoed = read_exact_bounded(&mut stream, 4).await;
    assert_eq!(
        echoed, b"ping",
        "an .i2p authority must reach the tunnel's own destination"
    );
    assert_eq!(
        outproxy
            .observed
            .lock()
            .expect("observed")
            .authorities
            .len(),
        0,
        "an .i2p authority must not consult the outproxy at all, not even to reject it"
    );
}

/// No provider means refusal, and no socket. The whole point is that the
/// absence of a route is an error rather than a silent direct connection.
#[tokio::test(flavor = "current_thread")]
async fn plan342_clearnet_target_without_a_provider_is_refused_and_opens_nothing() {
    let directory = temp_data_dir("opx-wire-noprovider");
    let origin = start_origin().await;
    let outproxy = start_outproxy(origin.address, FixtureBehaviour::default()).await;
    let material = capture_destination(directory.path(), "outproxy-svc", outproxy.address).await;

    let shared = manager(
        directory.path(),
        vec![server_spec("outproxy-svc", outproxy.address)],
        StaticAliasTable::new(),
    );
    let state = control(
        directory.path(),
        &shared,
        vec![startup_server("outproxy-svc", outproxy.address)],
    );
    // No outproxy block at all: the seven-field rule admits all seven or none.
    state
        .create(&create_request(
            "tunnel-a",
            TunnelType::ConnectClient,
            client_options(TunnelType::ConnectClient, &material, None, None),
        ))
        .await
        .expect("create");

    let (_scope, _cancel) = start_supervisors(&shared).await;
    let listener = shared
        .client_listener_address("tunnel-a")
        .expect("listener");

    let mut stream = connect_tunnel(listener, "example.com:443").await;
    let head = read_some(&mut stream).await;
    let text = String::from_utf8_lossy(&head);
    assert!(
        text.starts_with("HTTP/1.1 4") || text.starts_with("HTTP/1.1 5"),
        "a clearnet target with no provider must be refused; got {text:?}"
    );
    assert!(
        !text.contains("200"),
        "a refused target must never be answered 200"
    );
    assert_eq!(
        outproxy
            .observed
            .lock()
            .expect("observed")
            .authorities
            .len(),
        0,
        "nothing may be sent upstream when there is no route"
    );
}

/// The credential is presented as a header and never appears anywhere the local
/// client can read it.
#[tokio::test(flavor = "current_thread")]
async fn plan342_outproxy_auth_is_presented_and_never_echoed() {
    const PASSWORD: &str = "plan342-s3cret-value";
    let directory = temp_data_dir("opx-wire-auth");
    let origin = start_origin().await;
    // Require the real Basic value so the row proves the credential was
    // *recovered and encoded*, not merely that a header appeared.
    let expected = format!("Basic {}", base64_of("operator:plan342-s3cret-value"));
    let outproxy = start_outproxy(origin.address, FixtureBehaviour::default()).await;
    // Rebuild with the requirement, so the fixture holds the expected value.
    drop(outproxy);
    let outproxy = start_outproxy(
        origin.address,
        FixtureBehaviour {
            required_authorization: Some(Box::leak(expected.into_boxed_str())),
            ..FixtureBehaviour::default()
        },
    )
    .await;

    let material = capture_destination(directory.path(), "outproxy-svc", outproxy.address).await;
    let list = b32_label_of(&material);

    let shared = manager(
        directory.path(),
        vec![server_spec("outproxy-svc", outproxy.address)],
        StaticAliasTable::new(),
    );
    let state = control(
        directory.path(),
        &shared,
        vec![startup_server("outproxy-svc", outproxy.address)],
    );
    state
        .create(&create_request(
            "tunnel-a",
            TunnelType::ConnectClient,
            client_options(
                TunnelType::ConnectClient,
                &material,
                Some(&list),
                Some(("operator", PASSWORD)),
            ),
        ))
        .await
        .expect("create with a credential");

    let (_scope, _cancel) = start_supervisors(&shared).await;
    let listener = shared
        .client_listener_address("tunnel-a")
        .expect("listener");

    let mut stream = connect_tunnel(listener, "example.com:443").await;
    let head = read_some(&mut stream).await;
    let text = String::from_utf8_lossy(&head);
    assert!(
        text.starts_with("HTTP/1.1 200"),
        "a correctly presented credential must carry the request; got {text:?}"
    );
    assert!(
        !text.contains(PASSWORD),
        "the credential must never reach the local client"
    );
    let observed = outproxy.observed.lock().expect("observed");
    assert!(
        observed.saw_authorization,
        "the outproxy must have received an Authorization header"
    );
    assert_eq!(observed.authorities, vec!["example.com:443".to_owned()]);
}

/// The SOCKS family reaches the same route.
#[tokio::test(flavor = "current_thread")]
async fn plan342_socks5_request_is_carried_by_the_outproxy() {
    let directory = temp_data_dir("opx-wire-socks");
    let origin = start_origin().await;
    let outproxy = start_outproxy(origin.address, FixtureBehaviour::default()).await;
    let material = capture_destination(directory.path(), "outproxy-svc", outproxy.address).await;
    let list = b32_label_of(&material);

    let shared = manager(
        directory.path(),
        vec![server_spec("outproxy-svc", outproxy.address)],
        StaticAliasTable::new(),
    );
    let state = control(
        directory.path(),
        &shared,
        vec![startup_server("outproxy-svc", outproxy.address)],
    );
    state
        .create(&create_request(
            "socks-a",
            TunnelType::Socks,
            client_options(TunnelType::Socks, &material, Some(&list), None),
        ))
        .await
        .expect("create");

    let (_scope, _cancel) = start_supervisors(&shared).await;
    let listener = shared.client_listener_address("socks-a").expect("listener");

    let mut stream = TcpStream::connect(listener).await.expect("connect");
    // SOCKS5 greeting: no acceptable methods.
    stream
        .write_all(&[0x05, 0x01, 0x00])
        .await
        .expect("greeting");
    let mut greeting = [0_u8; 2];
    tokio::time::timeout(Duration::from_secs(10), stream.read_exact(&mut greeting))
        .await
        .expect("greeting reply")
        .expect("greeting bytes");
    assert_eq!(greeting, [0x05, 0x00], "no-auth must be selected");

    // CONNECT example.com:443 with a domain address type.
    let host = b"example.com";
    let mut request = vec![0x05, 0x01, 0x00, 0x03, u8::try_from(host.len()).unwrap()];
    request.extend_from_slice(host);
    request.extend_from_slice(&443_u16.to_be_bytes());
    stream.write_all(&request).await.expect("request");

    let mut reply = [0_u8; 10];
    tokio::time::timeout(Duration::from_secs(20), stream.read_exact(&mut reply))
        .await
        .expect("connect reply")
        .expect("reply bytes");
    assert_eq!(
        reply,
        [0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0],
        "a carried request must be answered with success"
    );
    assert_eq!(
        outproxy.observed.lock().expect("observed").authorities,
        vec!["example.com:443".to_owned()],
        "the SOCKS family must reach the same outproxy route"
    );
}

/// The forward path carries the *clearnet* authority in `Host:`, not the
/// tunnel's own b32 destination.
#[tokio::test(flavor = "current_thread")]
async fn plan342_http_forward_request_is_carried_by_the_outproxy() {
    let directory = temp_data_dir("opx-wire-forward");
    let origin = start_origin().await;
    let outproxy = start_outproxy(origin.address, FixtureBehaviour::default()).await;
    let material = capture_destination(directory.path(), "outproxy-svc", outproxy.address).await;
    let list = b32_label_of(&material);

    let shared = manager(
        directory.path(),
        vec![server_spec("outproxy-svc", outproxy.address)],
        StaticAliasTable::new(),
    );
    let state = control(
        directory.path(),
        &shared,
        vec![startup_server("outproxy-svc", outproxy.address)],
    );
    state
        .create(&create_request(
            "http-a",
            TunnelType::HttpClient,
            client_options(TunnelType::HttpClient, &material, Some(&list), None),
        ))
        .await
        .expect("create");

    let (_scope, _cancel) = start_supervisors(&shared).await;
    let listener = shared.client_listener_address("http-a").expect("listener");

    let mut stream = TcpStream::connect(listener).await.expect("connect");
    let request = "GET http://example.com/path?q=1 HTTP/1.1\r\nHost: example.com\r\n\r\n";
    stream.write_all(request.as_bytes()).await.expect("write");
    let response = read_some(&mut stream).await;
    assert!(
        response.starts_with(b"HTTP/1.1 200") || !response.is_empty(),
        "a carried forward request must produce a response; got {}",
        String::from_utf8_lossy(&response)
    );
    // The request named no port, so the scheme default applies and the
    // outproxy is asked for 80. Reading anything else here would mean the
    // forward path invented a port rather than carrying the request's own.
    assert_eq!(
        outproxy.observed.lock().expect("observed").authorities,
        vec!["example.com:80".to_owned()],
        "the forward path must reach the outproxy for the requested authority"
    );
}

/// The block grammar holds at the boundary: a malformed list or a name outside
/// the closed `OutproxyType` vocabulary is refused before a listener exists.
#[tokio::test(flavor = "current_thread")]
async fn plan342_malformed_block_is_refused_before_any_listener() {
    let directory = temp_data_dir("opx-wire-malformed");
    let shared = manager(directory.path(), Vec::new(), StaticAliasTable::new());
    let state = control(directory.path(), &shared, Vec::new());

    let material = b32(0x11);
    // A clearnet entry in the list: an outproxy is never a clearnet host.
    let clearnet_list = client_options(TunnelType::ConnectClient, &material, None, None);
    let mut with_clearnet = clearnet_list.clone();
    with_clearnet.insert("proxy_list".to_owned(), "outproxy.example.com".to_owned());
    with_clearnet.insert("use_outproxy_plugin".to_owned(), "true".to_owned());
    with_clearnet.insert("outproxy_type".to_owned(), "http".to_owned());
    with_clearnet.insert("ssl_proxies".to_owned(), "outproxy.example.com".to_owned());
    assert!(
        state
            .create(&create_request(
                "clearnet-list",
                TunnelType::ConnectClient,
                with_clearnet,
            ))
            .await
            .is_err(),
        "a clearnet ProxyList entry must be refused"
    );

    // An OutproxyType outside the closed vocabulary.
    let list = b32(0x22);
    let mut bad_type = client_options(TunnelType::ConnectClient, &material, Some(&list), None);
    bad_type.insert("outproxy_type".to_owned(), "/usr/bin/curl".to_owned());
    assert!(
        state
            .create(&create_request(
                "bad-type",
                TunnelType::ConnectClient,
                bad_type
            ))
            .await
            .is_err(),
        "an executable path as OutproxyType must be refused"
    );

    // Nothing was started, so no listener exists for either attempt.
    assert!(
        shared.client_listener_address("clearnet-list").is_none(),
        "a refused block must not have produced a listener"
    );
    assert!(
        shared.client_listener_address("bad-type").is_none(),
        "a refused block must not have produced a listener"
    );
}

/// Minimal standard-alphabet base64 for the expected Basic value. Written out
/// rather than pulled in as a dependency: the fixture needs to *verify* a
/// credential the daemon produced, and a test-only encoder keeps that from
/// becoming a production dependency.
fn base64_of(input: &str) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes = input.as_bytes();
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let triple = u32::from(b[0]) << 16 | u32::from(b[1]) << 8 | u32::from(b[2]);
        out.push(ALPHABET[((triple >> 18) & 0x3f) as usize] as char);
        out.push(ALPHABET[((triple >> 12) & 0x3f) as usize] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[((triple >> 6) & 0x3f) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[(triple & 0x3f) as usize] as char
        } else {
            '='
        });
    }
    out
}

/// A configured outproxy that cannot be reached terminates the request in
/// bounded time with a typed reason, and never leaves a half-open route.
///
/// This is the half of Plan 342's failover row that a loopback lane can
/// actually prove. What it cannot prove is the *rotation* — see the limitation
/// note in the module header. What it does prove is the property that makes
/// rotation safe to have at all: the request does not hang, and the counters
/// attribute the failure to a typed reason rather than to silence.
#[tokio::test(flavor = "current_thread")]
async fn plan342_unreachable_outproxy_fails_typed_and_bounded() {
    let directory = temp_data_dir("opx-wire-unreachable");
    // The endpoint is a service tunnel whose loopback target refuses every
    // connection, so the Streaming route opens and the handshake cannot
    // complete.
    let dead_origin = start_refusing_origin().await;
    let material = capture_destination(directory.path(), "dead-svc", dead_origin.address).await;
    let list = b32_label_of(&material);

    let shared = manager(
        directory.path(),
        vec![server_spec("dead-svc", dead_origin.address)],
        StaticAliasTable::new(),
    );
    let state = control(
        directory.path(),
        &shared,
        vec![startup_server("dead-svc", dead_origin.address)],
    );
    state
        .create(&create_request(
            "tunnel-a",
            TunnelType::ConnectClient,
            client_options(TunnelType::ConnectClient, &material, Some(&list), None),
        ))
        .await
        .expect("create");
    let (_scope, _cancel) =
        start_supervisors_with(&shared, ChildFailurePolicy::DegradeParent).await;
    let listener = shared
        .client_listener_address("tunnel-a")
        .expect("listener");

    let started = Instant::now();
    let mut stream = connect_tunnel(listener, "example.com:443").await;
    let head = read_some(&mut stream).await;
    let elapsed = started.elapsed();
    let text = String::from_utf8_lossy(&head);
    assert!(
        text.starts_with("HTTP/1.1 4") || text.starts_with("HTTP/1.1 5"),
        "an unreachable outproxy must be reported to the client, not silently dropped; got {text:?}"
    );
    assert!(
        elapsed < Duration::from_secs(60),
        "the request must terminate under a bounded deadline, took {elapsed:?}"
    );
    let counters = *shared.outproxy_counters().lock().expect("counters");
    assert!(
        counters.connect_attempts > 0,
        "the attempt must be counted so an operator can see the outproxy was tried"
    );
    assert!(
        counters.handshake_ok == 0,
        "a route that never completed its handshake must not be reported as one"
    );
    assert!(
        counters.target_unreachable > 0 || counters.attempts_exhausted > 0,
        "the failure must be attributed to a typed reason; counters were {counters:?}"
    );
}

/// A loopback origin that accepts and immediately closes every connection.
struct RefusingOrigin {
    address: SocketAddr,
}

async fn start_refusing_origin() -> RefusingOrigin {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("origin bind");
    let address = listener.local_addr().expect("origin addr");
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            drop(stream);
        }
    });
    RefusingOrigin { address }
}

// ---------------------------------------------------------------------------
// Plan 376 — live multi-endpoint failover, and a routed request across a
// product restart.
// ---------------------------------------------------------------------------
//
// Everything below extends the composition above rather than replacing it: the
// same real-Streaming route, the same service *server* tunnel standing in for
// the I2P outproxy destination, the same classifier, the same route owner. No
// socket here is a clearnet socket and the fixtures never resolve a name.
//
// # Ordered failover, not load rotation
//
// The provider contract is **ordered failover within one request**, not
// round-robin between requests: `OutproxyList::select(attempt)` maps attempt
// index to endpoint by `attempt % len`, and `open_via_outproxy` walks attempts
// from 0 until one succeeds. Two requests against the same tunnel both start
// at the first configured endpoint. That is deliberate — the operator's order
// is intent — and it is what the rows below pin. Between-request load rotation
// is not claimed, because no provider contract claims it.

/// A `.b32.i2p` label for a destination this router does **not** own.
///
/// This is the cleanest way to fail at the Streaming-connect stage: the
/// endpoint is well-formed, is in the operator's list, and is permitted for
/// the request, but `resolve_reference` cannot resolve it. The route owner
/// treats that as `TargetUnreachable`, which is retryable, so the same request
/// should continue to the next endpoint.
fn unowned_label(byte: u8) -> String {
    format!(
        "{}.b32.i2p",
        i2pr_service_tunnels::encode_b32_label(&[byte; 32])
    )
}

/// Composes a client tunnel whose `ProxyList` is `primary` then `secondary`.
///
/// Both endpoints must appear in `ssl_proxies`, because a CONNECT request is a
/// tunnelled request and `permits_tunnelled` is a subset check.
fn two_endpoint_options(
    target_destination: &str,
    primary: &str,
    secondary: &str,
    credential: Option<(&str, &str)>,
) -> BTreeMap<String, String> {
    let list = format!("{primary},{secondary}");
    client_options(
        TunnelType::ConnectClient,
        target_destination,
        Some(&list),
        credential,
    )
}

/// **HTTP CONNECT**, first endpoint unreachable at the Streaming-connect
/// stage, second endpoint live: the *same request* succeeds through the second.
#[tokio::test(flavor = "current_thread")]
async fn plan376_http_connect_fails_over_to_the_second_outproxy() {
    let directory = temp_data_dir("opx-376-http-failover");
    let origin = start_origin().await;
    let live = start_outproxy(origin.address, FixtureBehaviour::default()).await;
    let live_material = capture_destination(directory.path(), "outproxy-b", live.address).await;
    let live_label = b32_label_of(&live_material);
    // The dead endpoint is a valid I2P name with no destination behind it.
    let dead_label = unowned_label(0xA1);

    let shared = manager(
        directory.path(),
        vec![server_spec("outproxy-b", live.address)],
        StaticAliasTable::new(),
    );
    let state = control(
        directory.path(),
        &shared,
        vec![startup_server("outproxy-b", live.address)],
    );
    state
        .create(&create_request(
            "tunnel-a",
            TunnelType::ConnectClient,
            two_endpoint_options(&live_label, &dead_label, &live_label, None),
        ))
        .await
        .expect("the two-endpoint block is accepted");

    let (_scope, _cancel) = start_supervisors(&shared).await;
    let listener = shared
        .client_listener_address("tunnel-a")
        .expect("listener");

    let mut stream = connect_tunnel(listener, "example.com:443").await;
    let head = read_some(&mut stream).await;
    let text = String::from_utf8_lossy(&head);
    assert!(
        text.starts_with("HTTP/1.1 200"),
        "one request must survive a dead first endpoint; counters: {:?}; body: {text:?}",
        shared.outproxy_counters().lock().expect("counters")
    );

    // The live endpoint really is the one that was asked, and the dead one was
    // never asked for anything: it cannot be, having no destination.
    assert_eq!(
        live.observed.lock().expect("observed").authorities,
        vec!["example.com:443".to_owned()],
        "the second endpoint must receive the authority exactly once"
    );

    let counters = *shared.outproxy_counters().lock().expect("counters");
    assert_eq!(
        counters.connect_attempts, 2,
        "one attempt per configured endpoint and no more: {counters:?}"
    );
    assert_eq!(
        counters.handshake_ok, 1,
        "exactly one route completed its handshake: {counters:?}"
    );
    assert!(
        counters.target_unreachable >= 1,
        "the dead endpoint must be attributed to a typed reason, not silence: {counters:?}"
    );
}

/// The same composition over **SOCKS5**, so the failover is a property of the
/// route owner rather than of the HTTP request grammar.
#[tokio::test(flavor = "current_thread")]
async fn plan376_socks5_fails_over_to_the_second_outproxy() {
    let directory = temp_data_dir("opx-376-socks-failover");
    let origin = start_origin().await;
    let live = start_outproxy(origin.address, FixtureBehaviour::default()).await;
    let live_material = capture_destination(directory.path(), "outproxy-b", live.address).await;
    let live_label = b32_label_of(&live_material);
    let dead_label = unowned_label(0xA2);

    let shared = manager(
        directory.path(),
        vec![server_spec("outproxy-b", live.address)],
        StaticAliasTable::new(),
    );
    let state = control(
        directory.path(),
        &shared,
        vec![startup_server("outproxy-b", live.address)],
    );
    state
        .create(&create_request(
            "socks-a",
            TunnelType::Socks,
            two_endpoint_options(&live_label, &dead_label, &live_label, None),
        ))
        .await
        .expect("create");

    let (_scope, _cancel) = start_supervisors(&shared).await;
    let listener = shared.client_listener_address("socks-a").expect("listener");

    let mut stream = TcpStream::connect(listener).await.expect("connect");
    stream
        .write_all(&[0x05, 0x01, 0x00])
        .await
        .expect("greeting");
    let mut greeting = [0_u8; 2];
    tokio::time::timeout(Duration::from_secs(10), stream.read_exact(&mut greeting))
        .await
        .expect("greeting reply")
        .expect("greeting bytes");
    assert_eq!(greeting, [0x05, 0x00], "no-auth must be selected");

    let host = b"example.com";
    let mut request = vec![0x05, 0x01, 0x00, 0x03, u8::try_from(host.len()).unwrap()];
    request.extend_from_slice(host);
    request.extend_from_slice(&443_u16.to_be_bytes());
    stream.write_all(&request).await.expect("request");

    let mut reply = [0_u8; 10];
    tokio::time::timeout(Duration::from_secs(20), stream.read_exact(&mut reply))
        .await
        .expect("connect reply")
        .expect("reply bytes");
    assert_eq!(
        reply,
        [0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0],
        "a SOCKS request must survive a dead first endpoint"
    );
    assert_eq!(
        live.observed.lock().expect("observed").authorities,
        vec!["example.com:443".to_owned()],
        "the SOCKS family must fail over to the same second endpoint"
    );
}

/// The first endpoint **is** reachable and answers — with an upstream refusal.
/// `TargetUnreachable` is retryable, so the frozen taxonomy says the request
/// continues to the second endpoint. This row pins that half of the taxonomy,
/// which is the half a streaming-connect failure does not reach.
#[tokio::test(flavor = "current_thread")]
async fn plan376_an_upstream_refusal_is_retried_at_the_second_outproxy() {
    let directory = temp_data_dir("opx-376-upstream-refused");
    let origin = start_origin().await;
    let refused = start_outproxy(
        origin.address,
        FixtureBehaviour {
            refuse_upstream: true,
            ..FixtureBehaviour::default()
        },
    )
    .await;
    let live = start_outproxy(origin.address, FixtureBehaviour::default()).await;
    let refused_material =
        capture_destination(directory.path(), "outproxy-a", refused.address).await;
    let live_material = capture_destination(directory.path(), "outproxy-b", live.address).await;
    let refused_label = b32_label_of(&refused_material);
    let live_label = b32_label_of(&live_material);

    let shared = manager(
        directory.path(),
        vec![
            server_spec("outproxy-a", refused.address),
            server_spec("outproxy-b", live.address),
        ],
        StaticAliasTable::new(),
    );
    let state = control(
        directory.path(),
        &shared,
        vec![
            startup_server("outproxy-a", refused.address),
            startup_server("outproxy-b", live.address),
        ],
    );
    state
        .create(&create_request(
            "tunnel-a",
            TunnelType::ConnectClient,
            two_endpoint_options(
                &live_label,
                &refused_label,
                &live_label,
                Some(("operator", "s3cret!")),
            ),
        ))
        .await
        .expect("create");

    let (_scope, _cancel) = start_supervisors(&shared).await;
    let listener = shared
        .client_listener_address("tunnel-a")
        .expect("listener");

    let mut stream = connect_tunnel(listener, "example.com:443").await;
    let head = read_some(&mut stream).await;
    assert!(
        String::from_utf8_lossy(&head).starts_with("HTTP/1.1 200"),
        "an upstream refusal is retryable and must continue to the second endpoint; counters: {:?}; body: {:?}",
        shared.outproxy_counters().lock().expect("counters"),
        String::from_utf8_lossy(&head)
    );
    assert_eq!(
        refused.observed.lock().expect("observed").authorities,
        vec!["example.com:443".to_owned()],
        "the refusing endpoint must really have been asked first"
    );
    assert_eq!(
        live.observed.lock().expect("observed").authorities,
        vec!["example.com:443".to_owned()],
        "the live endpoint must receive the retry"
    );
    let counters = *shared.outproxy_counters().lock().expect("counters");
    assert!(
        counters.target_unreachable >= 1,
        "an upstream refusal must be attributed to target-unreachable: {counters:?}"
    );
}

/// The authentication-rejection half of the taxonomy, stated explicitly
/// because it is a security-relevant policy rather than an accident.
///
/// `OutproxyFailure::is_retryable` includes `AuthenticationRejected`, so the
/// current explicit policy **does** retry a 407 at the next configured
/// endpoint — and, because the credential is built once per attempt from the
/// sealed store, it presents the same credential to every endpoint in the
/// operator's list. Plan 376 requires that this either be an explicit policy
/// or be forbidden; it is explicit, so this row pins it rather than leaving it
/// implied.
///
/// The security consequence is real and is recorded in the closure: an
/// operator with two outproxies is offering one credential to two parties. That
/// is a consequence of the operator's own `ProxyList`, not of the router, but
/// an operator who does not expect it has no way to discover it, so the lane
/// header and the closure both say so out loud.
#[tokio::test(flavor = "current_thread")]
async fn plan376_authentication_rejection_is_retried_at_the_next_endpoint_by_the_current_policy() {
    let directory = temp_data_dir("opx-376-auth-retry");
    let origin = start_origin().await;
    let picky = start_outproxy(
        origin.address,
        FixtureBehaviour {
            required_authorization: Some("Basic Zm9yZ2VkOndoZW5seQ=="),
            ..FixtureBehaviour::default()
        },
    )
    .await;
    let live = start_outproxy(
        origin.address,
        FixtureBehaviour {
            required_authorization: Some("Basic b3BlcmF0b3I6czNjcmV0IQ=="),
            ..FixtureBehaviour::default()
        },
    )
    .await;
    let picky_material = capture_destination(directory.path(), "outproxy-a", picky.address).await;
    let live_material = capture_destination(directory.path(), "outproxy-b", live.address).await;
    let picky_label = b32_label_of(&picky_material);
    let live_label = b32_label_of(&live_material);

    let shared = manager(
        directory.path(),
        vec![
            server_spec("outproxy-a", picky.address),
            server_spec("outproxy-b", live.address),
        ],
        StaticAliasTable::new(),
    );
    let state = control(
        directory.path(),
        &shared,
        vec![
            startup_server("outproxy-a", picky.address),
            startup_server("outproxy-b", live.address),
        ],
    );
    state
        .create(&create_request(
            "tunnel-a",
            TunnelType::ConnectClient,
            two_endpoint_options(
                &live_label,
                &picky_label,
                &live_label,
                Some(("operator", "s3cret!")),
            ),
        ))
        .await
        .expect("create");

    let (_scope, _cancel) = start_supervisors(&shared).await;
    let listener = shared
        .client_listener_address("tunnel-a")
        .expect("listener");

    let mut stream = connect_tunnel(listener, "example.com:443").await;
    let head = read_some(&mut stream).await;
    assert!(
        String::from_utf8_lossy(&head).starts_with("HTTP/1.1 200"),
        "the frozen taxonomy retries an authentication rejection at the next endpoint; counters: {:?}; body: {:?}",
        shared.outproxy_counters().lock().expect("counters"),
        String::from_utf8_lossy(&head)
    );
    // Both endpoints saw a credential — that is the policy, pinned rather than
    // hidden. Neither saw anything but the sealed store's output.
    assert!(
        picky.observed.lock().expect("observed").saw_authorization,
        "the first endpoint must have been offered the credential and rejected it"
    );
    assert!(
        live.observed.lock().expect("observed").saw_authorization,
        "the retry must present the credential too; this is the documented policy"
    );
    let counters = *shared.outproxy_counters().lock().expect("counters");
    assert_eq!(
        counters.authentication_rejected, 1,
        "the rejection must be counted once: {counters:?}"
    );
    assert_eq!(
        counters.handshake_ok, 1,
        "exactly one route completed: {counters:?}"
    );
}

/// The attempt budget is the operator's list length, bounded by the hard
/// ceiling, and a request that spends all of it is refused typed — never
/// opened as a direct clearnet socket.
#[tokio::test(flavor = "current_thread")]
async fn plan376_the_attempt_budget_is_the_list_length_and_exhaustion_is_typed() {
    let directory = temp_data_dir("opx-376-budget");
    // Three configured endpoints, none of which this router can resolve.
    let shared = manager(directory.path(), Vec::new(), StaticAliasTable::new());
    let state = control(directory.path(), &shared, Vec::new());
    let target = b32(0xC1);
    let first = unowned_label(0xB1);
    let second = unowned_label(0xB2);
    let third = unowned_label(0xB3);
    state
        .create(&create_request(
            "tunnel-a",
            TunnelType::ConnectClient,
            two_endpoint_options(&target, &first, &format!("{second},{third}"), None),
        ))
        .await
        .expect("create");
    let (_scope, _cancel) = start_supervisors(&shared).await;
    let listener = shared
        .client_listener_address("tunnel-a")
        .expect("listener");

    let started = Instant::now();
    let mut stream = connect_tunnel(listener, "example.com:443").await;
    let head = read_some(&mut stream).await;
    let elapsed = started.elapsed();
    let text = String::from_utf8_lossy(&head);
    assert!(
        text.starts_with("HTTP/1.1 4") || text.starts_with("HTTP/1.1 5"),
        "an exhausted request must be refused, not dropped; got {text:?}"
    );
    assert!(
        elapsed < Duration::from_secs(90),
        "the whole budget must stay bounded; took {elapsed:?}"
    );
    let counters = *shared.outproxy_counters().lock().expect("counters");
    assert_eq!(
        counters.connect_attempts, 3,
        "the budget must be the list length: three endpoints, three attempts: {counters:?}"
    );
    assert_eq!(counters.handshake_ok, 0, "no route completed: {counters:?}");
    assert_eq!(
        counters.attempts_exhausted, 1,
        "the refusal must be attributed to exhaustion: {counters:?}"
    );
    assert_eq!(
        counters.direct_i2p, 0,
        "a clearnet target must never be routed directly"
    );
}

/// A router identity, generated once so two generations can share it the way
/// two runs of one router do.
#[allow(clippy::missing_panics_doc)]
fn router_identity() -> i2pr_crypto::RouterIdentityBundle {
    let mut rng = i2pr_crypto::OsRng;
    i2pr_crypto::RouterIdentityBundle::generate(&mut rng).expect("identity bundle")
}

/// The secret owner a restarted router rebuilds: same identity, fresh owner.
fn secret_owner_for(bundle: &i2pr_crypto::RouterIdentityBundle) -> Arc<dyn RouterSecretOwner> {
    Arc::new(
        i2pr_daemon::outbound_secret::RouterBoundOutboundSecrets::from_router_identity(bundle)
            .expect("router-bound secret owner"),
    )
}

/// One generation of the router for this tunnel pair: the manager, the control
/// state, the supervisor scope, and the cancellation token.
///
/// Everything it owns is dropped when it is dropped. A restart therefore means
/// dropping one of these and building another over the **same data
/// directory** — nothing else crosses the boundary.
struct Generation {
    shared: Arc<ServiceTunnelManager>,
    /// Held so the restart boundary drops it too. Never read: its only job is
    /// to be destroyed, and naming that is clearer than binding it to `_`.
    #[allow(dead_code)]
    state: TunnelControlState,
    #[allow(dead_code)]
    scope: ChildScope,
    cancel: CancellationToken,
}

/// Generation one: reconcile the operator's `create` into a fresh manager, then
/// start the manager's runtimes.
///
/// The order matters and it is the order Plan 342's lane uses. A `create`
/// reconciles the definition and installs the outproxy provider into the
/// registry; `manager.prepare()` then builds a runtime for every spec,
/// including the control-owned client tunnel, and `start_supervisors` starts
/// them. Starting the manager first and creating afterwards leaves the
/// client tunnel's listener bound by a supervisor that was already built, so
/// the tunnel never comes up — which looks exactly like a route failure and
/// is not one.
async fn build_generation(
    data_dir: &Path,
    specs: Vec<ServiceTunnelSpec>,
    startup: Vec<ServiceTunnelSpec>,
    aliases: StaticAliasTable,
    bundle: &i2pr_crypto::RouterIdentityBundle,
    options: BTreeMap<String, String>,
) -> Generation {
    let shared = manager(data_dir, specs, aliases);
    let state = TunnelControlState::new(
        ControlStore::open(data_dir).expect("control store"),
        ServiceTunnelSet { tunnels: startup },
        Arc::clone(&shared),
        secret_owner_for(bundle),
    );
    state
        .create(&create_request(
            "tunnel-a",
            TunnelType::ConnectClient,
            options,
        ))
        .await
        .expect("create");
    let (scope, cancel) = start_supervisors(&shared).await;
    Generation {
        shared,
        state,
        scope,
        cancel,
    }
}

/// Generation two: bring the product up from what it persisted.
///
/// This deliberately does **not** re-apply the operator's `create`. It opens a
/// fresh manager and a fresh control state over the same data directory and
/// runs [`TunnelControlState::startup`] — the product's own boot path, which
/// reloads the published generation, revalidates every stored definition,
/// reconciles the shared manager, and reinstalls the outproxy provider registry.
/// A provider that exists afterwards can therefore only have come from disk
/// plus the router-bound secret owner; nothing the test supplied survives.
async fn recover_generation(
    data_dir: &Path,
    specs: Vec<ServiceTunnelSpec>,
    startup: Vec<ServiceTunnelSpec>,
    aliases: StaticAliasTable,
    bundle: &i2pr_crypto::RouterIdentityBundle,
) -> Generation {
    let shared = manager(data_dir, specs, aliases);
    let (scope, cancel) = start_supervisors(&shared).await;
    let state = TunnelControlState::new(
        ControlStore::open(data_dir).expect("control store reopens"),
        ServiceTunnelSet { tunnels: startup },
        Arc::clone(&shared),
        secret_owner_for(bundle),
    );
    let failures = state.startup(&scope, &cancel).await;
    assert!(
        failures.is_empty(),
        "startup recovery must report no per-definition failures; got {failures:?}"
    );
    Generation {
        shared,
        state,
        scope,
        cancel,
    }
}

/// Plan 376 restart requirements 1–4: the canonical block survives a real
/// restart, the sealed password is still recoverable, the provider registry is
/// rebuilt before a request is served, and a post-restart CONNECT carries
/// application data.
#[tokio::test(flavor = "current_thread")]
async fn plan376_a_routed_request_survives_a_product_restart() {
    let directory = temp_data_dir("opx-376-restart");
    let origin = start_origin().await;
    let outproxy = start_outproxy(origin.address, FixtureBehaviour::default()).await;
    let material = capture_destination(directory.path(), "outproxy-svc", outproxy.address).await;
    let list = b32_label_of(&material);
    let bundle = router_identity();

    let specs = vec![server_spec("outproxy-svc", outproxy.address)];
    let startup = vec![startup_server("outproxy-svc", outproxy.address)];

    // ---- generation one -------------------------------------------------
    let first = build_generation(
        directory.path(),
        specs.clone(),
        startup.clone(),
        StaticAliasTable::new(),
        &bundle,
        client_options(
            TunnelType::ConnectClient,
            &material,
            Some(&list),
            Some(("operator", "s3cret!")),
        ),
    )
    .await;
    assert!(
        first.shared.outproxy_provider("tunnel-a").is_some(),
        "generation one must have installed a provider from the sealed credential"
    );
    let first_listener = first
        .shared
        .client_listener_address("tunnel-a")
        .expect("generation one listener");
    {
        let mut stream = connect_tunnel(first_listener, "example.com:443").await;
        let head = read_some(&mut stream).await;
        assert!(
            String::from_utf8_lossy(&head).starts_with("HTTP/1.1 200"),
            "generation one must serve the request; got {:?}",
            String::from_utf8_lossy(&head)
        );
    }

    // ---- the restart boundary ------------------------------------------
    // Every owner below is dropped whole. What survives is the data directory
    // and the fixture sockets, which is the entire point of the row.
    first
        .cancel
        .cancel(i2pr_runtime::CancellationReason::OperatorRequest);
    drop(first);
    assert!(
        tokio::time::timeout(
            Duration::from_secs(20),
            tokio::net::TcpStream::connect(first_listener)
        )
        .await
        .map(|result| result.is_err())
        .unwrap_or(false),
        "generation one's listener must actually be released, or this is not a restart"
    );

    // ---- generation two: brought up by startup recovery ------------------
    let second = recover_generation(
        directory.path(),
        specs,
        startup,
        StaticAliasTable::new(),
        &bundle,
    )
    .await;
    assert!(
        second.shared.outproxy_provider("tunnel-a").is_some(),
        "the provider registry must be reconstructed by startup before any request is served; \
         if it were not, every post-restart clearnet request would fall back to the strict parser"
    );

    let listener = second
        .shared
        .client_listener_address("tunnel-a")
        .expect("the recovered tunnel has a listener");
    let mut stream = connect_tunnel(listener, "example.com:443").await;
    let head = read_some(&mut stream).await;
    assert!(
        String::from_utf8_lossy(&head).starts_with("HTTP/1.1 200"),
        "a post-restart CONNECT must still be carried by an outproxy; counters: {:?}; body: {:?}",
        second.shared.outproxy_counters().lock().expect("counters"),
        String::from_utf8_lossy(&head)
    );
    // Requirement 4 says "application data passes", not merely "a head was
    // exchanged": the origin echoes, so bytes written after establishment must
    // come back over the restored route.
    stream.write_all(b"after-restart").await.expect("write");
    let echoed = read_exact_bounded(&mut stream, b"after-restart".len()).await;
    assert_eq!(
        echoed, b"after-restart",
        "application data must pass over the restored route"
    );
    assert_eq!(
        outproxy.observed.lock().expect("observed").authorities,
        vec!["example.com:443".to_owned(); 2],
        "the outproxy must have been asked exactly once per generation, and the \
         credential-bearing handshake must have been re-presented in the second"
    );

    // Requirements 1 and 2, read back rather than inferred: the recovered
    // definition still carries the whole canonical block, and the password is
    // still sealed.
    let stored = ControlStore::open(directory.path())
        .expect("store")
        .load()
        .expect("load")
        .definitions
        .into_iter()
        .find(|definition| definition.name == "tunnel-a")
        .expect("the client definition survived publication");
    for field in [
        "proxy_list",
        "use_outproxy_plugin",
        "outproxy_type",
        "ssl_proxies",
        "outproxy_auth",
        "outproxy_username",
        "outproxy_password",
    ] {
        assert!(
            stored.options.contains_key(field),
            "the canonical outproxy block lost {field} across the restart"
        );
    }
    assert_ne!(
        stored.options.get("outproxy_password"),
        Some(&"s3cret!".to_owned()),
        "the password must still be in its sealed form, not plaintext"
    );
    second
        .cancel
        .cancel(i2pr_runtime::CancellationReason::OperatorRequest);
}

/// Plan 376 restart requirements 5 and 6: `.i2p` traffic still bypasses the
/// provider after a restart, and a tunnel whose provider was never configured
/// fails closed on a clearnet target instead of opening a direct socket.
#[tokio::test(flavor = "current_thread")]
async fn plan376_after_restart_i2p_traffic_bypasses_and_removing_the_provider_fails_closed() {
    let directory = temp_data_dir("opx-376-restart-bypass");
    let direct_origin = start_origin().await;
    let outproxy_origin = start_origin().await;
    let outproxy = start_outproxy(outproxy_origin.address, FixtureBehaviour::default()).await;

    let direct_material =
        capture_destination(directory.path(), "direct-svc", direct_origin.address).await;
    let outproxy_material =
        capture_destination(directory.path(), "outproxy-svc", outproxy.address).await;
    let outproxy_label = b32_label_of(&outproxy_material);
    let bundle = router_identity();

    // The alias is what gives the request its `.i2p` spelling, and it points at
    // the *direct* destination — so a correct row lands on the direct fixture
    // and an incorrect one lands on the outproxy fixture. The alias is daemon
    // configuration, so it is re-supplied at restart exactly as a config file
    // would be; it is not the thing under test.
    let aliases = || {
        let mut table = StaticAliasTable::new();
        table
            .insert(
                "direct.example.i2p",
                DestinationRef::ConfiguredDestination(direct_material.clone()),
            )
            .expect("alias insert");
        table
    };
    let specs = || {
        vec![
            server_spec("direct-svc", direct_origin.address),
            server_spec("outproxy-svc", outproxy.address),
        ]
    };
    let startup = || {
        vec![
            startup_server("direct-svc", direct_origin.address),
            startup_server("outproxy-svc", outproxy.address),
        ]
    };

    let first = build_generation(
        directory.path(),
        specs(),
        startup(),
        aliases(),
        &bundle,
        client_options(
            TunnelType::ConnectClient,
            &direct_material,
            Some(&outproxy_label),
            None,
        ),
    )
    .await;
    assert!(first.shared.outproxy_provider("tunnel-a").is_some());

    first
        .cancel
        .cancel(i2pr_runtime::CancellationReason::OperatorRequest);
    drop(first);

    let second = recover_generation(directory.path(), specs(), startup(), aliases(), &bundle).await;
    assert!(
        second.shared.outproxy_provider("tunnel-a").is_some(),
        "the provider must be reconstructed by startup"
    );
    let listener = second
        .shared
        .client_listener_address("tunnel-a")
        .expect("recovered listener");

    // Requirement 5: `.i2p` traffic still bypasses the provider after restart.
    let before = outproxy
        .observed
        .lock()
        .expect("observed")
        .authorities
        .len();
    let mut stream = connect_tunnel(listener, "direct.example.i2p:443").await;
    let head = read_some(&mut stream).await;
    assert!(
        String::from_utf8_lossy(&head).starts_with("HTTP/1.1 200"),
        "an .i2p CONNECT must survive the restart; got {:?}",
        String::from_utf8_lossy(&head)
    );
    stream.write_all(b"ping").await.expect("write");
    let echoed = read_exact_bounded(&mut stream, 4).await;
    assert_eq!(
        echoed, b"ping",
        "the direct destination must receive the bytes"
    );
    assert_eq!(
        outproxy
            .observed
            .lock()
            .expect("observed")
            .authorities
            .len(),
        before,
        "an .i2p authority must still bypass the outproxy after a restart"
    );

    // Requirement 6: removing the provider makes clearnet requests fail
    // closed.
    //
    // `edit` cannot express this and the row says why, because it is a real
    // property of the control surface rather than a test inconvenience:
    // `TunnelControlState::edit` *merges* the request's options over the prior
    // definition, so an edit that omits the outproxy keys leaves them in place
    // and the provider stays installed. The only expressible way to remove a
    // provider is to destroy the definition and recreate it without the block.
    second
        .state
        .delete(&edit_request("tunnel-a", BTreeMap::new()))
        .await
        .expect("delete");
    let mut options = BTreeMap::new();
    options.insert("target_destination".to_owned(), direct_material.clone());
    options.insert("listen_port".to_owned(), "0".to_owned());
    second
        .state
        .create(&create_request(
            "tunnel-a",
            TunnelType::ConnectClient,
            options,
        ))
        .await
        .expect("a tunnel with no outproxy block is accepted");
    assert!(
        second.shared.outproxy_provider("tunnel-a").is_none(),
        "recreating the tunnel without the block must leave it with no provider, so the \
         next request is refused at the parser rather than routed"
    );
    let other = second
        .shared
        .client_listener_address("tunnel-a")
        .expect("the recreated tunnel has a listener");
    let mut stream = connect_tunnel(other, "example.com:443").await;
    let head = read_some(&mut stream).await;
    assert!(
        String::from_utf8_lossy(&head).starts_with("HTTP/1.1 4"),
        "a clearnet request with no provider must fail closed at the boundary, not open a direct socket; got {:?}",
        String::from_utf8_lossy(&head)
    );
    assert_eq!(
        outproxy
            .observed
            .lock()
            .expect("observed")
            .authorities
            .len(),
        before,
        "the refused request must not have consulted any outproxy"
    );
    second
        .cancel
        .cancel(i2pr_runtime::CancellationReason::OperatorRequest);
}

/// Plan 376 restart requirement 7, and the security claim it rests on: a
/// copied configuration directory without the matching router secret cannot
/// recover the credential, so it cannot present one.
#[tokio::test(flavor = "current_thread")]
async fn plan376_a_copied_config_without_the_router_secret_cannot_recover_the_credential() {
    let original = temp_data_dir("opx-376-copy-origin");
    let copied = temp_data_dir("opx-376-copy-target");
    let origin = start_origin().await;
    let outproxy = start_outproxy(origin.address, FixtureBehaviour::default()).await;
    let material = capture_destination(original.path(), "outproxy-svc", outproxy.address).await;
    let list = b32_label_of(&material);
    let bundle = router_identity();

    let specs = vec![server_spec("outproxy-svc", outproxy.address)];
    let startup = vec![startup_server("outproxy-svc", outproxy.address)];

    // Generation one under the real router identity, which seals the password.
    {
        let first = build_generation(
            original.path(),
            specs.clone(),
            startup.clone(),
            StaticAliasTable::new(),
            &bundle,
            client_options(
                TunnelType::ConnectClient,
                &material,
                Some(&list),
                Some(("operator", "s3cret!")),
            ),
        )
        .await;
        assert!(
            first.shared.outproxy_provider("tunnel-a").is_some(),
            "the sealed credential must have produced a working provider"
        );
        let listener = first
            .shared
            .client_listener_address("tunnel-a")
            .expect("listener");
        let mut stream = connect_tunnel(listener, "example.com:443").await;
        let head = read_some(&mut stream).await;
        assert!(
            String::from_utf8_lossy(&head).starts_with("HTTP/1.1 200"),
            "the original must genuinely work, or the copy proves nothing; got {:?}",
            String::from_utf8_lossy(&head)
        );
        first
            .cancel
            .cancel(i2pr_runtime::CancellationReason::OperatorRequest);
    }

    // Copy the whole directory, then recover under a *different* router
    // identity. The stored generation, including the sealed password, is intact
    // — what is missing is the key it was sealed with.
    copy_tree(original.path(), copied.path());
    let foreign = router_identity();
    // The two identities must actually differ, or the row would pass for the
    // wrong reason. Compare the public identities, never the private material.
    let original_router_id =
        i2pr_crypto::router_identity_hash(bundle.identity()).expect("the original identity hashes");
    let foreign_router_id =
        i2pr_crypto::router_identity_hash(foreign.identity()).expect("the foreign identity hashes");
    assert_ne!(
        original_router_id, foreign_router_id,
        "the two identities must actually differ, or the row tests nothing"
    );
    let shared = manager(copied.path(), specs, StaticAliasTable::new());
    let (scope, cancel) = start_supervisors(&shared).await;
    let recovered = TunnelControlState::new(
        ControlStore::open(copied.path()).expect("copied control store"),
        ServiceTunnelSet { tunnels: startup },
        Arc::clone(&shared),
        // A different router: the sealed form is bound to the original
        // identity's key and cannot be opened by this one.
        secret_owner_for(&foreign),
    );
    let _ = recovered.startup(&scope, &cancel).await;
    let listener = shared
        .client_listener_address("tunnel-a")
        .expect("the copied definition still builds; it just cannot authenticate");
    let mut stream = connect_tunnel(listener, "example.com:443").await;
    let head = read_some(&mut stream).await;
    assert!(
        String::from_utf8_lossy(&head).starts_with("HTTP/1.1 4")
            || String::from_utf8_lossy(&head).starts_with("HTTP/1.1 5"),
        "a copied config must not be able to present a credential it cannot recover; got {:?}",
        String::from_utf8_lossy(&head)
    );
    let counters = *shared.outproxy_counters().lock().expect("counters");
    assert!(
        counters.secret_owner_unavailable >= 1,
        "the refusal must be attributed to the secret owner, not to a network failure: {counters:?}"
    );
    assert_eq!(
        counters.handshake_ok, 0,
        "no route may have completed under a foreign identity: {counters:?}"
    );
    cancel.cancel(i2pr_runtime::CancellationReason::OperatorRequest);
}

/// Recursively copies a data directory, preserving the owner-only permissions
/// the store requires.
///
/// Two properties are deliberate. A **symlink is refused rather than
/// followed**, so the copy cannot escape the source tree and the row cannot be
/// satisfied by a link pointing back at the original. The **mode is copied**,
/// not just the bytes: `ControlStore` refuses a group- or world-readable
/// directory, and a `create_dir_all` default of `0o755` would make the copy
/// fail for a permissions reason while appearing to be a credential result.
fn copy_tree(from: &Path, to: &Path) {
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    let entries = std::fs::read_dir(from).expect("read source dir");
    for entry in entries {
        let entry = entry.expect("dir entry");
        let kind = entry.file_type().expect("entry type");
        assert!(
            !kind.is_symlink(),
            "a symlink in the data directory would make the copy test meaningless"
        );
        let source = entry.path();
        let target = to.join(entry.file_name());
        if kind.is_dir() {
            std::fs::create_dir(&target).expect("create copied dir");
            copy_tree(&source, &target);
        } else {
            std::fs::copy(&source, &target).expect("copy file");
        }
        #[cfg(unix)]
        {
            let mode = std::fs::metadata(&source)
                .expect("source metadata")
                .permissions()
                .mode()
                & 0o777;
            std::fs::set_permissions(&target, std::fs::Permissions::from_mode(mode))
                .expect("preserve permissions");
        }
    }
}
