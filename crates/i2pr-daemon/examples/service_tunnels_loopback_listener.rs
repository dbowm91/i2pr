//! Plan 181 — external-harness Milestone 10 service-tunnels listener.
//!
//! This example boots the i2pr M10 service-tunnel manager with a
//! pre-configured set of enabled client/server tunnels, prints the
//! bound ports as a single JSON document to stdout, and then serves
//! the manager until the process receives SIGINT / SIGTERM.
//!
//! It exists so the external independent-application-client harness
//! (`tests/integration/service-tunnels/run-independent.sh`) can drive
//! the real i2pr M10 service surface through plain TCP/UDP without
//! taking a hard dependency on `i2pr_daemon` library internals.
//!
//! The example is **not** a production daemon path; it deliberately
//! skips the SAM/I2CP listener bootstrap pipeline and uses the
//! existing Plan 174 runtime-neutral shared Streaming pump + Plan
//! 180 reconcile surface with a single committed generation. The
//! byte round-trip is local-only by construction (the manager's
//! `run_client_loop`/`run_server_loop` pair share the local SAM
//! destination bridge so the harness can prove the application-
//! facing service surface without a real I2P router).
//!
//! ## Two-phase construction
//!
//! The manager's `prepare()` validates the full spec set against
//! its destination references in one pass; client tunnels resolve
//! their destination by Base32 hash lookup against the runtimes
//! map populated by `prepare()`. To bridge the construction-time
//! ordering constraint, this example runs:
//!
//! 1. **Phase A**: build a transient manager with only the server
//!    tunnels, prepare, capture the server destination base64, drop
//!    the transient manager (so the server identities persist into
//!    the durable data dir, matching Plan 175's restart-stable
//!    invariant);
//! 2. **Phase B**: build the durable manager with the full set of
//!    server + client tunnels, where the client tunnels reference
//!    the server destinations by full destination base64
//!    ([`DestinationRef::ConfiguredDestination`]). The manager's
//!    `resolve_reference` decodes the public material directly so no
//!    NetDB lookup is required for the byte round-trip.
//!
//! Output schema (single JSON line, then the process keeps running):
//!
//! ```text
//! {
//!   "kind": "service-tunnels",
//!   "pid": 12345,
//!   "data_dir": "/tmp/i2pr-m10-XXXX",
//!   "services": {
//!     "alpha-generic-client":  {"port": 51001},
//!     "alpha-http-client":     {"port": 51002},
//!     "alpha-socks5-client":   {"port": 51003},
//!     "alpha-irc-client":      {"port": 51004},
//!     "alpha-generic-server":  {"destination_b64": "<..>"},
//!     "alpha-irc-server":      {"destination_b64": "<..>"}
//!   },
//!   "local_targets": {
//!     "alpha-generic-server": "127.0.0.1:51010",
//!     "alpha-irc-server":     "127.0.0.1:51011"
//!   }
//! }
//! ```
//!
//! Usage:
//!   cargo run --example service_tunnels_loopback_listener -- [--data-dir DIR]
//!     [--generic-server-target 127.0.0.1:NNNN]
//!     [--irc-server-target 127.0.0.1:NNNN]
//!
//! The server-target flags pin the loopback TCP targets to fixture
//! listeners the harness already owns (both must be loopback).
//! Without them the example probes ephemeral ports (which no
//! fixture serves, so Streaming establishment succeeds but the
//! target dial fails closed).
//!
//! The harness always runs with no extra environment and never
//! patches the source tree. Failure modes make this binary exit
//! non-zero so the harness can fail closed.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use i2pr_runtime::{CancellationToken, ChildFailurePolicy, ChildScope};
use i2pr_service_tunnels::{
    DestinationPolicy, DestinationRef, HttpClientOptions, IrcClientOptions, LocalListenerSpec,
    ServerTarget, ServiceTunnelKind, ServiceTunnelSet, ServiceTunnelSpec, Socks5ClientOptions,
    StaticAliasTable,
};
use tokio::net::TcpListener;

use i2pr_daemon::service_tunnels::{ServiceTunnelManager, ServiceTunnelManagerConfig};

/// Renders a JSON-safe string (escapes `"` and `\`).
fn json_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for byte in value.bytes() {
        match byte {
            b'"' => out.push_str("\\\""),
            b'\\' => out.push_str("\\\\"),
            b'\n' => out.push_str("\\n"),
            b'\r' => out.push_str("\\r"),
            b'\t' => out.push_str("\\t"),
            0x20..=0x7e => out.push(byte as char),
            _ => out.push_str(&format!("\\u{:04x}", byte)),
        }
    }
    out.push('"');
    out
}

/// Renders one JSON object for a single (id, json body) pair.
fn render_object(entries: &[(&str, String)]) -> String {
    let mut out = String::from("{");
    for (idx, (key, body)) in entries.iter().enumerate() {
        if idx > 0 {
            out.push(',');
        }
        out.push_str(&json_string(key));
        out.push(':');
        out.push_str(body);
    }
    out.push('}');
    out
}

/// One client-tunnel kind and its configured listener spec.
fn client_spec(
    id: &str,
    kind: ServiceTunnelKind,
    listener: SocketAddr,
    target: DestinationRef,
) -> ServiceTunnelSpec {
    let (http_options, socks5_options, irc_options) = match kind {
        ServiceTunnelKind::HttpClient => (Some(HttpClientOptions::defaults()), None, None),
        ServiceTunnelKind::Socks5Client => (None, Some(Socks5ClientOptions::defaults()), None),
        ServiceTunnelKind::IrcClient => (None, None, Some(IrcClientOptions::default())),
        _ => (None, None, None),
    };
    ServiceTunnelSpec {
        id: i2pr_service_tunnels::ServiceTunnelId::parse(id).expect("id"),
        kind,
        enabled: true,
        listener: Some(
            LocalListenerSpec::parse_socket(&format!("127.0.0.1:{}", listener.port()))
                .expect("listener"),
        ),
        target: None,
        targets: Vec::new(),
        destination: Some(target),
        policy: DestinationPolicy::Dedicated,
        max_connections: 4,
        max_buffered_bytes_per_direction: 65_536,
        timeouts: i2pr_service_tunnels::ServiceTimeouts::defaults(),
        http_options,
        socks5_options,
        irc_options,
    }
}

/// One server-tunnel kind and its loopback TCP target.
fn server_spec(id: &str, kind: ServiceTunnelKind, target: SocketAddr) -> ServiceTunnelSpec {
    ServiceTunnelSpec {
        id: i2pr_service_tunnels::ServiceTunnelId::parse(id).expect("id"),
        kind,
        enabled: true,
        listener: None,
        target: Some(ServerTarget::LoopbackTcp(target)),
        targets: Vec::new(),
        destination: None,
        policy: DestinationPolicy::Dedicated,
        max_connections: 4,
        max_buffered_bytes_per_direction: 65_536,
        timeouts: i2pr_service_tunnels::ServiceTimeouts::defaults(),
        http_options: None,
        socks5_options: None,
        irc_options: None,
    }
}

fn parse_data_dir(args: &mut impl Iterator<Item = String>) -> Result<PathBuf, String> {
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--data-dir" => {
                return Ok(PathBuf::from(
                    args.next().ok_or("missing value for --data-dir")?,
                ));
            }
            "--port-base" | "--generic-server-target" | "--irc-server-target" => {
                let _ = args.next();
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(std::env::temp_dir().join(format!("i2pr-m10-{}", std::process::id())))
}

/// Parses an explicit loopback server-target socket passed by the
/// harness (which owns the fixture listener). Returns `None` when
/// the flag is absent; the caller then probes an ephemeral port.
fn parse_server_target(
    args: &mut impl Iterator<Item = String>,
    flag: &str,
) -> Result<Option<SocketAddr>, String> {
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--data-dir" | "--port-base" => {
                let _ = args.next();
            }
            other if other == flag => {
                let value = args.next().ok_or(format!("missing value for {flag}"))?;
                let socket: SocketAddr = value
                    .parse()
                    .map_err(|_| format!("invalid {flag} value: {value}"))?;
                if !socket.ip().is_loopback() {
                    return Err(format!("{flag} must be loopback, got: {value}"));
                }
                return Ok(Some(socket));
            }
            _ => {}
        }
    }
    Ok(None)
}

async fn ephemeral_port() -> Result<u16, Box<dyn std::error::Error>> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    drop(listener);
    Ok(addr.port())
}

/// One iteration of phase A (transient manager, server-only, capture
/// destination base64).
async fn capture_server_destinations(
    data_dir: &std::path::Path,
    generic_server_target_port: u16,
    irc_server_target_port: u16,
) -> Result<(String, String), Box<dyn std::error::Error>> {
    let tunnel_set = ServiceTunnelSet {
        tunnels: vec![
            server_spec(
                "alpha-generic-server",
                ServiceTunnelKind::GenericServer,
                format!("127.0.0.1:{generic_server_target_port}").parse()?,
            ),
            server_spec(
                "alpha-irc-server",
                ServiceTunnelKind::IrcServer,
                format!("127.0.0.1:{irc_server_target_port}").parse()?,
            ),
        ],
    };
    tunnel_set.validate()?;
    let manager_config = ServiceTunnelManagerConfig {
        data_dir: data_dir.to_path_buf(),
        aggregate_connection_ceiling: 32,
        per_service_connection_ceiling: 8,
        specs: Arc::new(tunnel_set),
        aliases: Arc::new(StaticAliasTable::new()),
    };
    let manager = Arc::new(ServiceTunnelManager::new(manager_config)?);
    let _ = manager.prepare().await?;
    let generic = manager
        .service_destination_b64("alpha-generic-server")
        .ok_or("missing alpha-generic-server destination b64")?;
    let irc = manager
        .service_destination_b64("alpha-irc-server")
        .ok_or("missing alpha-irc-server destination b64")?;
    Ok((generic, irc))
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Observability for the external harness: INFO/above goes to
    // stderr (the harness captures it separately from the JSON
    // port document on stdout). Never logs key material or
    // application payloads (the daemon only logs ids/counters).
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_writer(std::io::stderr)
        .try_init();
    let raw_args: Vec<String> = std::env::args().skip(1).collect();
    let mut args_iter = raw_args.clone().into_iter();
    let data_dir =
        parse_data_dir(&mut args_iter).map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;
    std::fs::create_dir_all(&data_dir)?;

    let generic_client_port = ephemeral_port().await?;
    let http_client_port = ephemeral_port().await?;
    let socks5_client_port = ephemeral_port().await?;
    let irc_client_port = ephemeral_port().await?;
    // The harness may pin the server-target sockets to fixture
    // listeners it already owns; otherwise probe ephemeral ports.
    let mut target_args = raw_args.clone().into_iter();
    let generic_server_target: SocketAddr =
        match parse_server_target(&mut target_args, "--generic-server-target")
            .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?
        {
            Some(socket) => socket,
            None => format!("127.0.0.1:{}", ephemeral_port().await?).parse()?,
        };
    let mut target_args = raw_args.clone().into_iter();
    let irc_server_target: SocketAddr =
        match parse_server_target(&mut target_args, "--irc-server-target")
            .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?
        {
            Some(socket) => socket,
            None => format!("127.0.0.1:{}", ephemeral_port().await?).parse()?,
        };
    let generic_server_target_port = generic_server_target.port();
    let irc_server_target_port = irc_server_target.port();

    // Phase A: capture server destination base64 via a transient
    // manager. The data dir is durable so the persisted identity
    // carries into Phase B.
    let (generic_server_b64, irc_server_b64) = capture_server_destinations(
        &data_dir,
        generic_server_target_port,
        irc_server_target_port,
    )
    .await?;

    // Phase B: build the durable manager with both server and
    // client tunnels. The client tunnels reference the server
    // destination via the full public material
    // (DestinationRef::ConfiguredDestination) so the manager's
    // `resolve_reference` decodes the public bytes directly
    // without requiring a Base32 hash to match a registered
    // service destination at construction time.
    let generic_client_addr: SocketAddr = format!("127.0.0.1:{generic_client_port}").parse()?;
    let http_client_addr: SocketAddr = format!("127.0.0.1:{http_client_port}").parse()?;
    let socks5_client_addr: SocketAddr = format!("127.0.0.1:{socks5_client_port}").parse()?;
    let irc_client_addr: SocketAddr = format!("127.0.0.1:{irc_client_port}").parse()?;

    let generic_destination = DestinationRef::ConfiguredDestination(generic_server_b64.clone());
    let irc_destination = DestinationRef::ConfiguredDestination(irc_server_b64.clone());

    let tunnel_set = ServiceTunnelSet {
        tunnels: vec![
            client_spec(
                "alpha-generic-client",
                ServiceTunnelKind::GenericClient,
                generic_client_addr,
                generic_destination,
            ),
            client_spec(
                "alpha-http-client",
                ServiceTunnelKind::HttpClient,
                http_client_addr,
                DestinationRef::ConfiguredDestination(generic_server_b64.clone()),
            ),
            client_spec(
                "alpha-socks5-client",
                ServiceTunnelKind::Socks5Client,
                socks5_client_addr,
                DestinationRef::ConfiguredDestination(generic_server_b64.clone()),
            ),
            client_spec(
                "alpha-irc-client",
                ServiceTunnelKind::IrcClient,
                irc_client_addr,
                irc_destination,
            ),
            server_spec(
                "alpha-generic-server",
                ServiceTunnelKind::GenericServer,
                generic_server_target,
            ),
            server_spec(
                "alpha-irc-server",
                ServiceTunnelKind::IrcServer,
                irc_server_target,
            ),
        ],
    };
    tunnel_set
        .validate()
        .map_err(|error| -> Box<dyn std::error::Error> {
            format!("tunnel set validation failed: {error}").into()
        })?;
    // Wire a static alias so the harness can use a stable lowercase
    // `.i2p` host string instead of carrying the full destination
    // base64 through every curl/socks/irc invocation. The HTTP/SOCKS
    // target validation already requires lowercase `.i2p` hosts.
    let mut aliases = StaticAliasTable::new();
    aliases
        .insert(
            "alpha-test.i2p",
            DestinationRef::ConfiguredDestination(generic_server_b64.clone()),
        )
        .map_err(|e| -> Box<dyn std::error::Error> { format!("alias insert: {e}").into() })?;
    aliases
        .insert(
            "alpha-irc.i2p",
            DestinationRef::ConfiguredDestination(irc_server_b64.clone()),
        )
        .map_err(|e| -> Box<dyn std::error::Error> { format!("alias insert: {e}").into() })?;
    let manager_config = ServiceTunnelManagerConfig {
        data_dir: data_dir.clone(),
        aggregate_connection_ceiling: 32,
        per_service_connection_ceiling: 8,
        specs: Arc::new(tunnel_set),
        aliases: Arc::new(aliases),
    };
    let manager = Arc::new(
        ServiceTunnelManager::new(manager_config)
            .map_err(|e| -> Box<dyn std::error::Error> { format!("manager build: {e}").into() })?,
    );

    let parent = CancellationToken::new();
    let scope = ChildScope::for_test(&parent, ChildFailurePolicy::FailParent);
    let runtimes = manager
        .prepare()
        .await
        .map_err(|e| -> Box<dyn std::error::Error> { format!("prepare: {e}").into() })?;
    manager
        .start_supervisors(runtimes, &scope, parent.clone())
        .map_err(|e| -> Box<dyn std::error::Error> { format!("supervisors: {e}").into() })?;

    // Build the public listener-port snapshot for the harness.
    let mut services = BTreeMap::new();
    for spec_id in [
        "alpha-generic-client",
        "alpha-http-client",
        "alpha-socks5-client",
        "alpha-irc-client",
    ] {
        if let Some(addr) = manager.client_listener_address(spec_id) {
            let body = format!("{{\"port\":{}}}", addr.port());
            services.insert(spec_id.to_owned(), body);
        }
    }
    for spec_id in ["alpha-generic-server", "alpha-irc-server"] {
        if let Some(b64) = manager.service_destination_b64(spec_id) {
            let body = format!("{{\"destination_b64\":{}}}", json_string(&b64));
            services.insert(spec_id.to_owned(), body);
        }
    }

    let services_entries: Vec<(&str, String)> = services
        .iter()
        .map(|(key, body)| (key.as_str(), body.clone()))
        .collect();
    let services_json = render_object(&services_entries);

    let target_entries = [
        (
            "alpha-generic-server",
            json_string(&format!("127.0.0.1:{generic_server_target_port}")),
        ),
        (
            "alpha-irc-server",
            json_string(&format!("127.0.0.1:{irc_server_target_port}")),
        ),
    ];
    let local_targets_json = render_object(&target_entries);

    let aliases_entries = [
        ("alpha-test.i2p", json_string(&generic_server_b64)),
        ("alpha-irc.i2p", json_string(&irc_server_b64)),
    ];
    let aliases_json = render_object(&aliases_entries);

    let mut json = String::new();
    let _ = writeln!(
        json,
        "{{\"kind\":\"service-tunnels\",\"pid\":{},\"data_dir\":{},\"services\":{},\"local_targets\":{},\"aliases\":{}}}",
        std::process::id(),
        json_string(&data_dir.display().to_string()),
        services_json,
        local_targets_json,
        aliases_json,
    );
    {
        use std::io::Write;
        let stdout = std::io::stdout();
        let mut handle = stdout.lock();
        handle.write_all(json.as_bytes())?;
        handle.flush()?;
    }

    // Block until SIGINT / SIGTERM.
    let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .expect("install SIGTERM handler");
    let mut sigint = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
        .expect("install SIGINT handler");
    tokio::select! {
        _ = sigterm.recv() => {}
        _ = sigint.recv() => {}
    }

    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = tokio::time::timeout(Duration::from_secs(10), scope.shutdown()).await;
    Ok(())
}
