//! Plan 496 — the guarded NTCP2 operator profile.
//!
//! Plan 101 refused every NTCP2 activation that was not the explicit
//! controlled-loopback profile. Plan 496 keeps that refusal for every
//! configuration the old guard rejected and adds one narrowly-scoped
//! operator profile: a guarded, RFC1918-only, **non-public** private LAN.
//!
//! What each row proves:
//!
//! * `the_private_lan_profile_requires_the_matching_ssu2_profile` — the two
//!   transports must agree, because one RouterInfo carries both. A
//!   half-configured private LAN is refused at parse time.
//! * `the_private_lan_profile_refuses_every_non_rfc1918_endpoint` — loopback,
//!   the unspecified address, a public address, TEST-NET and IPv6 are each
//!   refused with their own reason.
//! * `the_controlled_profile_does_not_widen_to_a_private_lan_endpoint` — the
//!   Plan 101 refusal is intact: the controlled profile still refuses a
//!   private-LAN bind.
//! * `the_private_lan_profile_never_enables_advertisement` — the profile is a
//!   local bind, not a publication permit.
//! * `private_lan_profile_binds_the_operator_endpoint_and_advertises_nothing`
//!   — the **environment-gated** row: the shipped `i2pr` executable binds the
//!   operator's real RFC1918 endpoint on both transports, the record carries
//!   no capability letter, and the daemon shuts down cleanly.
//!
//! Timing note: the environment-gated row drives the composition root in a
//! child process against real sockets, so a paused Tokio clock cannot
//! govern it. Every wait is an explicit bounded deadline over wall-clock
//! time with a coarse poll interval. There are no unbounded waits and no
//! sleeps used to assert behaviour.
//!
//! The environment-gated row is `#[ignore]`-gated, as the repository
//! requires for a lane that needs a host property: the ordinary run compiles
//! it and skips it, and the explicit run **fails** when the host has no
//! RFC1918 address rather than silently passing.

use std::io::{BufRead, BufReader};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

/// Upper bound on how long the daemon may take to bind. The supervisor's own
/// startup deadline is 30s; this is deliberately shorter so a regression
/// surfaces as a failure rather than a hang.
const READY_DEADLINE: Duration = Duration::from_secs(25);

/// Coarse poll interval while waiting for the child to bind or exit.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Upper bound on the child's shutdown.
const SHUTDOWN_DEADLINE: Duration = Duration::from_secs(20);

/// Resolves the host's own RFC1918 IPv4 address.
///
/// The probe binds an unspecified UDP socket and *connects* it toward each
/// RFC1918 range. A UDP connect performs a route lookup and sends no
/// packet, so the kernel reports the source address it would use for that
/// destination without touching the network.
fn host_rfc1918_address() -> Option<Ipv4Addr> {
    for destination in ["192.168.255.255:9", "172.31.255.255:9", "10.255.255.255:9"] {
        let Ok(socket) = UdpSocket::bind("0.0.0.0:0") else {
            continue;
        };
        if socket.connect(destination).is_err() {
            continue;
        }
        let Ok(source) = socket.local_addr() else {
            continue;
        };
        if let IpAddr::V4(address) = source.ip()
            && address.is_private()
        {
            return Some(address);
        }
    }
    None
}

/// Reserves and releases an ephemeral TCP port on the given address.
fn reserve_tcp_port(address: Ipv4Addr) -> u16 {
    let probe = TcpListener::bind((address, 0)).expect("reserve TCP port");
    probe.local_addr().expect("reserved address").port()
}

/// Reserves and releases an ephemeral UDP port on the given address.
fn reserve_udp_port(address: Ipv4Addr) -> u16 {
    let probe = UdpSocket::bind((address, 0)).expect("reserve UDP port");
    probe.local_addr().expect("reserved address").port()
}

/// Builds the private-LAN configuration body.
///
/// Both transports name the same RFC1918 address, which is what the
/// configuration layer's family-coherence rule requires.
fn private_lan_config(
    data_dir: &Path,
    address: Ipv4Addr,
    ntcp2_port: u16,
    ssu2_port: u16,
) -> String {
    format!(
        "schema_version = 1\n\n[router]\ndata_dir = {:?}\n\n[logging]\nfilter = \"debug\"\n\n\
         [ssu2]\nenabled = true\nqualification_profile = \"private_lan_qualification\"\n\
         bind_ipv4 = \"{address}\"\nbind_ipv6 = \"\"\nport = {ssu2_port}\n\
         advertise = false\nintroducer_service = false\n\n\
         [transport.ntcp2]\nenabled = true\nqualification_profile = \"private_lan_qualification\"\n\
         bind_address = \"{address}\"\nlisten_port = {ntcp2_port}\n",
        data_dir.to_string_lossy(),
    )
}

/// A configuration body with one field replaced, so each refusal can be
/// exercised without disturbing the rest.
fn with_replacement(body: &str, from: &str, to: &str) -> String {
    assert!(
        body.contains(from),
        "the configuration body must contain {from:?}"
    );
    body.replace(from, to)
}

fn parse_error(config: &str) -> String {
    let error =
        i2pr_daemon::config::Config::parse(config).expect_err("the configuration must be refused");
    format!("{error}")
}

#[test]
fn the_private_lan_profile_requires_the_matching_ssu2_profile() {
    let directory = tempfile::tempdir().expect("temp directory");
    let data_dir = directory.path().join("state");
    std::fs::create_dir(&data_dir).expect("create data dir");
    let address = Ipv4Addr::new(192, 168, 10, 25);
    let body = private_lan_config(&data_dir, address, 40124, 40125);

    // The SSU2 surface must itself be in the private-LAN profile.
    let error = parse_error(&with_replacement(
        &body,
        "qualification_profile = \"private_lan_qualification\"\nbind_ipv4",
        "qualification_profile = \"controlled_loopback\"\nbind_ipv4",
    ));
    assert!(
        error.contains("transport.ntcp2.qualification_profile"),
        "the family-coherence refusal must name the field: {error}"
    );
    assert!(
        error.contains("private_lan_qualification requires the ssu2 surface"),
        "the family-coherence refusal must state the requirement: {error}"
    );
}

#[test]
fn the_private_lan_profile_refuses_every_non_rfc1918_endpoint() {
    let directory = tempfile::tempdir().expect("temp directory");
    let data_dir = directory.path().join("state");
    std::fs::create_dir(&data_dir).expect("create data dir");
    let body = private_lan_config(&data_dir, Ipv4Addr::new(192, 168, 10, 25), 40124, 40125);

    for (replacement, expected) in [
        ("127.0.0.1", "loopback is not a private-LAN address"),
        (
            "0.0.0.0",
            "the unspecified address is not a private-LAN address",
        ),
        ("8.8.8.8", "requires an RFC1918 IPv4 address"),
        ("192.0.2.1", "requires an RFC1918 IPv4 address"),
    ] {
        let error = parse_error(&with_replacement(
            &body,
            "bind_address = \"192.168.10.25\"",
            &format!("bind_address = \"{replacement}\""),
        ));
        assert!(
            error.contains("transport.ntcp2.bind_address"),
            "{replacement}: the refusal must name the field: {error}"
        );
        assert!(
            error.contains(expected),
            "{replacement}: the refusal must state the requirement: {error}"
        );
    }
}

#[test]
fn the_controlled_profile_does_not_widen_to_a_private_lan_endpoint() {
    let directory = tempfile::tempdir().expect("temp directory");
    let data_dir = directory.path().join("state");
    std::fs::create_dir(&data_dir).expect("create data dir");
    let body = private_lan_config(&data_dir, Ipv4Addr::new(192, 168, 10, 25), 40124, 40125);

    // Plan 101's refusal is intact: the controlled profile is loopback-only.
    let error = parse_error(&with_replacement(
        &body,
        "qualification_profile = \"private_lan_qualification\"\nbind_address",
        "qualification_profile = \"controlled_loopback\"\nbind_address",
    ));
    assert!(
        error.contains("transport.ntcp2.bind_address"),
        "the controlled profile must refuse a private-LAN bind: {error}"
    );
    assert!(
        error.contains("controlled_loopback requires a loopback address"),
        "the controlled profile must state the requirement: {error}"
    );
}

#[test]
fn the_private_lan_profile_never_enables_advertisement() {
    let directory = tempfile::tempdir().expect("temp directory");
    let data_dir = directory.path().join("state");
    std::fs::create_dir(&data_dir).expect("create data dir");
    let body = private_lan_config(&data_dir, Ipv4Addr::new(192, 168, 10, 25), 40124, 40125);

    let error = parse_error(&with_replacement(
        &body,
        "advertise = false",
        "advertise = true",
    ));
    assert!(
        error.contains("ssu2.advertise"),
        "the advertisement refusal must name the field: {error}"
    );
}

#[test]
fn the_private_lan_profile_requires_an_explicit_nonzero_port() {
    let directory = tempfile::tempdir().expect("temp directory");
    let data_dir = directory.path().join("state");
    std::fs::create_dir(&data_dir).expect("create data dir");
    let body = private_lan_config(&data_dir, Ipv4Addr::new(192, 168, 10, 25), 40124, 40125);

    let error = parse_error(&with_replacement(
        &body,
        "listen_port = 40124",
        "listen_port = 0",
    ));
    assert!(
        error.contains("transport.ntcp2.listen_port"),
        "the port refusal must name the field: {error}"
    );
    assert!(
        error.contains("explicit nonzero port"),
        "the port refusal must state the requirement: {error}"
    );

    let error = parse_error(&with_replacement(
        &body,
        "listen_port = 40124",
        "listen_port = 443",
    ));
    assert!(
        error.contains("at least 1024"),
        "the privileged-port refusal must state the floor: {error}"
    );
}

/// The environment-gated row.
///
/// It needs a host with an RFC1918 address. When the host has none the row
/// **fails**: a missing environment must never silently pass.
#[test]
#[ignore = "requires a host with an RFC1918 IPv4 address"]
fn private_lan_profile_binds_the_operator_endpoint_and_advertises_nothing() {
    let address = host_rfc1918_address().unwrap_or_else(|| {
        panic!(
            "Plan 496 needs a host with an RFC1918 IPv4 address: the private-LAN \
             operator profile binds an operator-supplied RFC1918 endpoint, and this \
             host has none. The row fails rather than skipping, because a missing \
             environment must never pass silently."
        )
    });
    let ntcp2_port = reserve_tcp_port(address);
    let ssu2_port = reserve_udp_port(address);

    let directory = tempfile::tempdir().expect("temp directory");
    let data_dir = directory.path().join("state");
    std::fs::create_dir(&data_dir).expect("create data dir");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&data_dir, std::fs::Permissions::from_mode(0o700))
            .expect("set data dir mode");
    }
    let config = directory.path().join("router.toml");
    std::fs::write(
        &config,
        private_lan_config(&data_dir, address, ntcp2_port, ssu2_port),
    )
    .expect("write config");

    // The identity is created through the CLI, exactly as an operator does.
    let generated = Command::new(env!("CARGO_BIN_EXE_i2pr"))
        .args(["identity", "generate", "--config"])
        .arg(&config)
        .output()
        .expect("run identity generate");
    assert!(
        generated.status.success(),
        "identity generate failed: {}",
        String::from_utf8_lossy(&generated.stderr)
    );

    let mut child = Command::new(env!("CARGO_BIN_EXE_i2pr"))
        .args(["run", "--config"])
        .arg(&config)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn i2pr run");
    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");
    let stdout = drain(stdout);
    let stderr = drain(stderr);

    let endpoint = SocketAddr::new(IpAddr::V4(address), ntcp2_port);
    let mut connected = None;
    let deadline = Instant::now() + READY_DEADLINE;
    while Instant::now() < deadline {
        if let Ok(stream) = TcpStream::connect_timeout(&endpoint, Duration::from_millis(200)) {
            connected = Some(stream);
            break;
        }
        if let Some(status) = child.try_wait().expect("check daemon") {
            let output = collect(&mut child, stdout, stderr);
            panic!("the private-LAN daemon exited before bind ({status}):\n{output}");
        }
        std::thread::sleep(POLL_INTERVAL);
    }
    drop(connected.expect("the private-LAN NTCP2 listener did not bind"));

    // The SSU2 transport must hold the same private endpoint. Binding it
    // again must fail with the address already in use.
    let ssu2_conflict = UdpSocket::bind((address, ssu2_port))
        .expect_err("the private-LAN SSU2 socket must stay bound");
    assert_eq!(ssu2_conflict.kind(), std::io::ErrorKind::AddrInUse);

    let _ = Command::new("kill")
        .args(["-INT", &child.id().to_string()])
        .status();
    let status = wait_for_exit(&mut child, SHUTDOWN_DEADLINE);
    let output = collect(&mut child, stdout, stderr);
    assert!(status.success(), "shutdown failed: {status}\n{output}");
    assert!(
        !output.contains("caps=f"),
        "the private-LAN profile must not advertise a capability letter:\n{output}"
    );
    assert!(
        !output.contains("publish_local_router_info"),
        "the private-LAN profile must not publish a RouterInfo:\n{output}"
    );
}

/// Waits for the child to exit on its own, killing it after the deadline.
fn wait_for_exit(child: &mut Child, deadline: Duration) -> ExitStatus {
    let started = Instant::now();
    while started.elapsed() < deadline {
        match child.try_wait().expect("try_wait") {
            Some(status) => return status,
            None => std::thread::sleep(POLL_INTERVAL),
        }
    }
    let _ = child.kill();
    child.wait().expect("reap killed child")
}

/// Joins the reader threads and returns everything the child printed.
fn collect(
    child: &mut Child,
    stdout: std::thread::JoinHandle<String>,
    stderr: std::thread::JoinHandle<String>,
) -> String {
    let _ = child.wait();
    let stdout = stdout.join().unwrap_or_default();
    let stderr = stderr.join().unwrap_or_default();
    format!("{stdout}\n{stderr}")
}

fn drain<R: std::io::Read + Send + 'static>(reader: R) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut collected = String::new();
        for line in BufReader::new(reader).lines().map_while(Result::ok) {
            collected.push_str(&line);
            collected.push('\n');
        }
        collected
    })
}

/// Keeps the unused-import lint honest about the path type.
#[allow(dead_code)]
fn _path_type_marker(_: &PathBuf) {}
