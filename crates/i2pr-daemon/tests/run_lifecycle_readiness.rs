//! Plan 360 — `i2pr run` must actually start the router.
//!
//! These rows are deliberately **black-box**: they drive the shipped `i2pr`
//! executable through its CLI exactly as an operator does, then observe the
//! child from outside over loopback TCP and the OS socket inventory. No
//! private bootstrap, supervisor, manager, or pump API is called from this
//! file, so a row cannot pass by exercising an internal seam the product
//! does not use.
//!
//! What each row proves:
//!
//! * `run_binds_configured_listener_and_shuts_down_cleanly` — the composition
//!   root starts, a listener is *observably bound* (a real SAM 3.1 handshake
//!   completes over loopback, twice, so the listener survives past startup),
//!   and SIGINT tears the daemon down with no orphaned listener. A router
//!   that reported ready having bound nothing fails this row, which is the
//!   point: that failure is worse than the `ReadinessTimeout` this plan
//!   removed.
//! * `occupied_loopback_port_is_refused_with_a_named_diagnostic` — a
//!   configuration that genuinely cannot start is refused with a diagnostic
//!   naming the service, its deadline, and the reason, rather than a bare
//!   `ReadinessTimeout`.
//! * `run_refuses_without_an_identity` — the pre-existing fail-closed
//!   identity gate still holds.
//! * `default_profile_starts_without_binding_a_listener` — no listener
//!   default was widened to make this plan pass, and readiness does not
//!   depend on a listener existing. Daemon-scoped, not host-scoped: the row
//!   enumerates the listeners owned by the child process itself, so a
//!   neighbor holding a port can neither fail it nor mask a widened
//!   default — and a widened default on *any* port fails it, not just SAM.
//!   (The conventional default values themselves are pinned host-free by
//!   `config::tests::sam_listener_defaults_stay_loopback_disabled_on_the_conventional_port`.)
//!
//! Timing note: the composition root runs in a child process against real
//! loopback sockets, so a paused Tokio clock cannot govern it. Every wait
//! below is an explicit bounded deadline over wall-clock time with a coarse
//! poll interval. There are no unbounded waits and no sleeps used to assert
//! behaviour.

use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use i2pr_crypto::{OsRng, RouterIdentityBundle, X25519PrivateKey};
use i2pr_proto::{Date, Mapping, RouterAddress};
use i2pr_runtime::{CancellationToken, Ntcp2RuntimeConfig, Ntcp2RuntimeService};
use i2pr_storage::{IdentityStore, Ntcp2TransportIdentityStore};
use tempfile::TempDir;

/// Upper bound on how long the daemon may take to become observable. The
/// supervisor's own startup deadline is 30s; this is deliberately shorter so
/// a regression surfaces as a test failure rather than a hang.
const READY_DEADLINE: Duration = Duration::from_secs(25);

/// Coarse poll interval while waiting for the child to bind or exit.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Upper bound on how long SIGINT shutdown may take before it is a failure.
const SHUTDOWN_DEADLINE: Duration = Duration::from_secs(20);

/// How long the positive row holds the daemon up before re-probing.
///
/// The composition root bounds the *bind* of a listener to one second and
/// must never bound the serving phase. Holding the router up slightly longer
/// than that bind deadline is what distinguishes "binds, then serves" from
/// "binds, then the timeout kills a healthy listener" — the second shape is
/// the pre-Plan-360 defect and passes every other assertion here.
const HOLD_OPEN: Duration = Duration::from_millis(1500);

/// How long the default-profile row observes the child holding zero
/// listeners. Listeners bind during startup (never lazily), so a clean
/// window past a live child closes the slow-start race without asserting
/// on timing.
const LISTEN_OBSERVE_WINDOW: Duration = Duration::from_secs(5);

/// Exit code the CLI reserves for a supervisor failure.
const EXIT_SUPERVISOR_FAILED: i32 = 46;

/// Exit code the CLI reserves for a missing or invalid router identity.
const EXIT_IDENTITY: i32 = 41;

/// A test-owned config file plus the router data directory it points at.
struct Router {
    _directory: TempDir,
    data_dir: PathBuf,
    config: PathBuf,
    /// The loopback address the config names for the SAM listener.
    sam_address: SocketAddr,
}

impl Router {
    /// Builds a router whose config names a freshly reserved ephemeral
    /// loopback SAM port.
    ///
    /// The port is reserved by binding and immediately releasing it, so the
    /// daemon is told to bind a specific address this test then probes. No
    /// listener default is changed: the test enables SAM explicitly.
    fn with_sam() -> Router {
        let address = reserve_loopback_port();
        Router::new(|data_dir| sam_config(address, data_dir))
    }

    /// Builds a router from a config body that receives the absolute data
    /// directory path. The path must be absolute: a relative `data_dir`
    /// resolves against the child's working directory, which would write
    /// identity state outside this test's temporary directory.
    fn new(config_body: impl FnOnce(&Path) -> String) -> Router {
        let directory = tempfile::tempdir().expect("temp directory");
        let data_dir = directory.path().join("state");
        std::fs::create_dir(&data_dir).expect("create data dir");
        // The identity store refuses a permissive directory, so create the
        // data dir 0700 rather than relying on the ambient umask.
        set_mode(&data_dir, 0o700);

        let config = directory.path().join("router.toml");
        std::fs::write(&config, config_body(&data_dir)).expect("write config");
        let sam_address = configured_sam_address(&config).unwrap_or_else(|| {
            format!("127.0.0.1:{}", reserve_loopback_port().port())
                .parse()
                .expect("loopback address")
        });
        Router {
            _directory: directory,
            data_dir,
            config,
            sam_address,
        }
    }

    /// Runs `identity generate` through the CLI, so the test exercises the
    /// same path an operator does.
    fn generate_identity(&self) {
        let output = command()
            .args(["identity", "generate", "--config"])
            .arg(&self.config)
            .output()
            .expect("run identity generate");
        assert!(
            output.status.success(),
            "identity generate failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn spawn(&self) -> RunningRouter {
        RunningRouter::spawn(&self.config, self.sam_address)
    }
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).expect("set mode");
}

#[cfg(not(unix))]
fn set_mode(_path: &Path, _mode: u32) {}

fn command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_i2pr"))
}

/// Reserves and releases an ephemeral loopback port.
fn reserve_loopback_port() -> SocketAddr {
    let probe = TcpListener::bind("127.0.0.1:0").expect("reserve loopback port");
    probe.local_addr().expect("reserved address")
}

/// Config enabling only the loopback SAM listener on the given port.
///
/// SAM stays disabled by default and non-advertised; this test enables it
/// explicitly rather than the product widening a default.
fn sam_config(address: SocketAddr, data_dir: &Path) -> String {
    format!(
        "schema_version = 1\n\n[router]\ndata_dir = {:?}\n\n[sam]\nenabled = true\nbind_address = \"127.0.0.1\"\nport = {}\nudp_port = 0\n",
        data_dir.to_string_lossy(),
        address.port()
    )
}

fn controlled_ntcp2_config(port: u16, data_dir: &Path) -> String {
    format!(
        "schema_version = 1\n\n[router]\ndata_dir = {:?}\n\n[transport.ntcp2]\nenabled = true\nqualification_profile = \"controlled_loopback\"\nbind_address = \"127.0.0.1\"\nlisten_port = {}\n",
        data_dir.to_string_lossy(),
        port,
    )
}

fn test_i2p_base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-~";
    let mut output = String::new();
    for chunk in bytes.chunks(3) {
        let mut value = 0_u32;
        for byte in chunk {
            value = (value << 8) | u32::from(*byte);
        }
        value <<= 8 * (3 - chunk.len());
        let digits = if chunk.len() == 1 {
            2
        } else if chunk.len() == 2 {
            3
        } else {
            4
        };
        for index in 0..digits {
            output.push(ALPHABET[((value >> (18 - 6 * index)) & 0x3f) as usize] as char);
        }
        for _ in digits..4 {
            output.push('=');
        }
    }
    output
}

fn controlled_ntcp2_client_session(
    router: &Router,
    endpoint: SocketAddr,
    network_id: u8,
) -> Result<(), String> {
    let bundle = IdentityStore::in_data_dir(&router.data_dir)
        .load()
        .map_err(|_| "router identity unavailable".to_owned())?;
    let router_hash = bundle
        .identity()
        .hash()
        .map_err(|_| "router identity hash unavailable".to_owned())?;
    let transport = Ntcp2TransportIdentityStore::in_data_dir(&router.data_dir)
        .load(*router_hash.as_bytes())
        .map_err(|_| "NTCP2 transport identity unavailable".to_owned())?;
    let peer_info_bytes = i2pr_daemon::router_i2np::generate_controlled_ntcp2_router_info(
        &bundle,
        endpoint.ip(),
        endpoint.port(),
        u16::from(network_id),
        &transport,
    )
    .map_err(|_| "daemon NTCP2 RouterInfo could not be reconstructed".to_owned())?;
    let peer_info = i2pr_proto::RouterInfo::decode(&peer_info_bytes, 1 << 20)
        .map_err(|_| "daemon NTCP2 RouterInfo could not be decoded".to_owned())?;
    i2pr_crypto::verify_router_info(&peer_info)
        .map_err(|_| "daemon NTCP2 RouterInfo signature invalid".to_owned())?;
    let responder_hash = bundle
        .identity()
        .hash()
        .map_err(|_| "router identity hash unavailable".to_owned())?;
    if i2pr_crypto::router_identity_hash(peer_info.router_identity())
        .map_err(|_| "RouterInfo identity hash unavailable".to_owned())?
        != responder_hash
    {
        return Err("daemon NTCP2 RouterInfo identity mismatch".to_owned());
    }
    let address = peer_info
        .addresses()
        .iter()
        .find(|address| address.transport_style() == "NTCP2")
        .ok_or_else(|| "daemon NTCP2 RouterAddress missing".to_owned())?;
    if address.options().get("host") != Some(endpoint.ip().to_string().as_str())
        || address.options().get("port") != Some(endpoint.port().to_string().as_str())
        || address.options().get("v") != Some("2")
        || address.options().get("s")
            != Some(test_i2p_base64(&transport.transport().key().public_bytes()).as_str())
        || address.options().get("i") != Some(test_i2p_base64(transport.transport().iv()).as_str())
    {
        return Err("daemon NTCP2 RouterAddress material or endpoint mismatch".to_owned());
    }
    let client_bundle =
        RouterIdentityBundle::from_private_bytes([0x91; 32], [0x92; 32], &mut OsRng)
            .map_err(|_| "client identity generation failed".to_owned())?;
    let client_static = X25519PrivateKey::from_bytes([0x93; 32]);
    let client_address = RouterAddress::new(
        1,
        Date::from_millis(86_400_000),
        "NTCP2".to_owned(),
        Mapping::from_entries(vec![
            ("v".to_owned(), "2".to_owned()),
            (
                "s".to_owned(),
                test_i2p_base64(&client_static.public_bytes()),
            ),
        ])
        .map_err(|_| "client address options invalid".to_owned())?,
    )
    .map_err(|_| "client address invalid".to_owned())?;
    let client_info = client_bundle
        .sign_router_info(
            Date::from_millis(1_000),
            vec![client_address],
            Vec::new(),
            Mapping::from_entries(vec![("netId".to_owned(), network_id.to_string())])
                .map_err(|_| "client RouterInfo options invalid".to_owned())?,
        )
        .map_err(|_| "client RouterInfo signing failed".to_owned())?
        .encode_to_vec(1 << 20)
        .map_err(|_| "client RouterInfo encoding failed".to_owned())?;
    let cancellation = CancellationToken::new();
    let runtime = Ntcp2RuntimeService::new(Ntcp2RuntimeConfig::default())
        .map_err(|_| "client NTCP2 runtime config invalid".to_owned())?;
    let scope = runtime.child_scope(&cancellation);
    let tokio_runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| "client runtime creation failed".to_owned())?;
    tokio_runtime.block_on(async {
        let link = runtime
            .dial_authenticated_loopback_peer(
                &scope,
                &peer_info,
                *responder_hash.as_bytes(),
                client_static,
                &client_info,
                network_id,
                1,
                &cancellation,
            )
            .await
            .map_err(|_| "daemon rejected NTCP2 authentication".to_owned())?;
        link.close();
        Ok(())
    })
}

/// Configures the ordinary daemon's controlled loopback SSU2 owner and SAM
/// listener. Both remain opt-in; the test names the loopback profile itself.
fn controlled_ssu2_sam_config(address: SocketAddr, ssu2_port: u16, data_dir: &Path) -> String {
    format!(
        "schema_version = 1\n\n[router]\ndata_dir = {:?}\n\n[sam]\nenabled = true\nproduct_mode = \"local-only\"\nbind_address = \"127.0.0.1\"\nport = {}\nudp_port = 0\n\n[ssu2]\nenabled = true\nbind_ipv4 = \"127.0.0.1\"\nbind_ipv6 = \"\"\nport = {}\nadvertise = false\nintroducer_service = false\n",
        data_dir.to_string_lossy(),
        address.port(),
        ssu2_port
    )
}

fn reserve_loopback_udp_port() -> u16 {
    let socket = std::net::UdpSocket::bind("127.0.0.1:0").expect("reserve UDP port");
    socket.local_addr().expect("reserved UDP address").port()
}

/// Config naming no listener at all.
fn default_config(data_dir: &Path) -> String {
    format!(
        "schema_version = 1\n\n[router]\ndata_dir = {:?}\n",
        data_dir.to_string_lossy()
    )
}

/// Reads the port the config names for SAM, so a probe targets exactly what
/// the daemon was told to bind.
fn configured_sam_address(config: &Path) -> Option<SocketAddr> {
    let text = std::fs::read_to_string(config).ok()?;
    let port: u16 = text
        .lines()
        .find_map(|line| line.trim().strip_prefix("port ="))
        .and_then(|value| value.trim().parse().ok())?;
    format!("127.0.0.1:{port}").parse().ok()
}

/// A spawned `i2pr run` child whose output is drained on reader threads.
///
/// Draining matters: the child writes structured logs to stdout and stderr,
/// and a full pipe buffer would block the router before it could bind.
struct RunningRouter {
    child: Child,
    address: SocketAddr,
    stdout: Option<std::thread::JoinHandle<String>>,
    stderr: Option<std::thread::JoinHandle<String>>,
}

impl RunningRouter {
    fn spawn(config: &Path, address: SocketAddr) -> RunningRouter {
        let mut child = command()
            .args(["run", "--config"])
            .arg(config)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn i2pr run");
        let stdout = child.stdout.take().expect("piped stdout");
        let stderr = child.stderr.take().expect("piped stderr");
        RunningRouter {
            child,
            address,
            stdout: Some(drain(stdout)),
            stderr: Some(drain(stderr)),
        }
    }

    /// Returns the exit status once the child exits on its own, or `None` if
    /// the deadline passes first.
    fn wait_for_exit(&mut self, deadline: Duration) -> Option<ExitStatus> {
        let started = Instant::now();
        while started.elapsed() < deadline {
            match self.child.try_wait().expect("try_wait") {
                Some(status) => return Some(status),
                None => std::thread::sleep(POLL_INTERVAL),
            }
        }
        None
    }

    /// Requests SIGINT, which `run_daemon` treats as operator shutdown.
    #[cfg(unix)]
    fn interrupt(&self) {
        let _ = Command::new("kill")
            .args(["-INT", &self.child.id().to_string()])
            .status();
    }

    #[cfg(not(unix))]
    fn interrupt(&self) {
        let _ = self.child.kill();
    }

    /// Joins both reader threads and returns everything the child printed.
    /// Kills the child if it is still alive so a failure cannot leak a
    /// process into the rest of the suite.
    fn finish(mut self) -> (ExitStatus, String) {
        let status = self.wait_for_exit(SHUTDOWN_DEADLINE).unwrap_or_else(|| {
            let _ = self.child.kill();
            self.child.wait().expect("reap killed child")
        });
        // `take` (not move): `Drop` below forbids moving fields out of
        // `self`, and dropping the `Option` shell is harmless either way.
        let stdout = self
            .stdout
            .take()
            .map(|handle| handle.join().unwrap_or_default())
            .unwrap_or_default();
        let stderr = self
            .stderr
            .take()
            .map(|handle| handle.join().unwrap_or_default())
            .unwrap_or_default();
        (status, format!("{stdout}\n{stderr}"))
    }
}

impl Drop for RunningRouter {
    /// A panic between `spawn` and `finish` must not leak the child into
    /// the rest of the suite. `finish` consumes `self`, so this only runs
    /// on the failure paths that used to orphan one daemon per failed run.
    /// Best-effort and panic-free: dropping during unwinding must not
    /// panic again.
    fn drop(&mut self) {
        if self
            .child
            .try_wait()
            .map(|status| status.is_none())
            .unwrap_or(false)
        {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
    }
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

/// The documented default loopback SAM address, probed only where the OS
/// offers no way to enumerate a process's own listeners (see below).
#[cfg(not(unix))]
fn default_sam_address() -> SocketAddr {
    "127.0.0.1:7656".parse().expect("default SAM address")
}

/// Lists the TCP listeners owned by `pid`, as `ip:port` strings. An I/O or
/// parse failure is an `Err`: an unobservable child must fail the row, never
/// pass it with an empty list.
#[cfg(target_os = "linux")]
fn child_listeners(pid: u32) -> Result<Vec<String>, String> {
    use std::collections::HashSet;

    let mut owned: HashSet<u64> = HashSet::new();
    let fds = std::fs::read_dir(format!("/proc/{pid}/fd"))
        .map_err(|error| format!("cannot list the file descriptors of {pid}: {error}"))?;
    for entry in fds {
        let entry =
            entry.map_err(|error| format!("cannot read a file descriptor of {pid}: {error}"))?;
        let target = std::fs::read_link(entry.path())
            .map_err(|error| format!("cannot read the link {}: {error}", entry.path().display()))?;
        if let Some(inode) = target
            .to_string_lossy()
            .strip_prefix("socket:[")
            .and_then(|inner| inner.strip_suffix(']'))
            .and_then(|inner| inner.parse::<u64>().ok())
        {
            owned.insert(inode);
        }
    }
    let mut listeners = Vec::new();
    for table in ["tcp", "tcp6"] {
        let text = std::fs::read_to_string(format!("/proc/net/{table}"))
            .map_err(|error| format!("cannot read /proc/net/{table}: {error}"))?;
        for line in text.lines().skip(1) {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() < 10 || fields[3] != "0A" {
                continue;
            }
            let inode: u64 = fields[9]
                .parse()
                .map_err(|_| format!("bad inode in /proc/net/{table}: {line}"))?;
            if owned.contains(&inode) {
                listeners.push(decode_proc_address(table, fields[1])?);
            }
        }
    }
    Ok(listeners)
}

/// Decodes a `/proc/net/tcp*` local address (`HEXIP:HEXPORT`) for failure
/// messages. Each 32-bit word prints in host byte order, so words are
/// swapped back to network order before rendering.
#[cfg(target_os = "linux")]
fn decode_proc_address(table: &str, field: &str) -> Result<String, String> {
    let (ip_hex, port_hex) = field
        .split_once(':')
        .ok_or_else(|| format!("bad address in /proc/net/{table}: {field}"))?;
    let port = u16::from_str_radix(port_hex, 16)
        .map_err(|_| format!("bad port in /proc/net/{table}: {field}"))?;
    if ip_hex.len() == 8 {
        let word = u32::from_str_radix(ip_hex, 16)
            .map_err(|_| format!("bad IPv4 in /proc/net/{table}: {field}"))?;
        Ok(format!(
            "{}:{port}",
            std::net::Ipv4Addr::from(word.swap_bytes())
        ))
    } else if ip_hex.len() == 32 {
        let mut octets = [0u8; 16];
        for (index, chunk) in octets.chunks_mut(4).enumerate() {
            let word = u32::from_str_radix(&ip_hex[index * 8..index * 8 + 8], 16)
                .map_err(|_| format!("bad IPv6 in /proc/net/{table}: {field}"))?;
            chunk.copy_from_slice(&word.swap_bytes().to_be_bytes());
        }
        Ok(format!("{}:{port}", std::net::Ipv6Addr::from(octets)))
    } else {
        Err(format!("bad IP length in /proc/net/{table}: {field}"))
    }
}

/// `lsof` edition of [`child_listeners`] for Unix without `/proc` (notably
/// macOS CI). `lsof` exits 1 with empty output when nothing matches: that is
/// the pass shape, not an error. Any other non-zero exit fails loudly with
/// the tool's own output, so a missing or changed `lsof` can never pass
/// silent.
#[cfg(all(unix, not(target_os = "linux")))]
fn child_listeners(pid: u32) -> Result<Vec<String>, String> {
    let output = Command::new("lsof")
        .args([
            "-n",
            "-P",
            "-a",
            "-p",
            &pid.to_string(),
            "-iTCP",
            "-sTCP:LISTEN",
        ])
        .output()
        .map_err(|error| format!("cannot execute lsof: {error}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    if output.status.success() {
        let mut listeners = Vec::new();
        for line in stdout.lines().skip(1) {
            if let Some(name) = line
                .find("(LISTEN)")
                .and_then(|index| line[..index].split_whitespace().last())
            {
                listeners.push(name.to_owned());
            }
        }
        Ok(listeners)
    } else if output.status.code() == Some(1) && stdout.trim().is_empty() {
        Ok(Vec::new())
    } else {
        Err(format!(
            "lsof for pid {pid} exited {:?} with stdout {stdout:?} and stderr {:?}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

/// Observes the child holding zero listeners for a bounded window.
///
/// Daemon-scoped, not host-scoped: the listeners enumerated are the ones
/// owned by the child process itself, so a neighbor holding a port can
/// neither fail this row nor mask a widened default — and a widened
/// default on *any* port fails it, not just SAM. The child being alive is
/// re-checked every poll alongside the inventory.
#[cfg(unix)]
fn assert_no_child_listeners(router: &mut RunningRouter) {
    let deadline = Instant::now() + LISTEN_OBSERVE_WINDOW;
    loop {
        let listeners = child_listeners(router.child.id())
            .expect("enumerate the listeners owned by the daemon child");
        assert!(
            listeners.is_empty(),
            "a default profile must bind nothing, but the child holds: {listeners:?}"
        );
        assert!(
            router.child.try_wait().expect("try_wait").is_none(),
            "a default profile must stay up rather than exit"
        );
        if Instant::now() >= deadline {
            return;
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}

/// No socket inventory exists off Unix: keep the legacy host-global probe
/// of the conventional default port, documented as weaker (a neighbor
/// holding the port fails the row; only SAM is covered). Every platform
/// the floor runs on takes the Unix path above.
#[cfg(not(unix))]
fn assert_no_child_listeners(router: &mut RunningRouter) {
    assert!(
        router.child.try_wait().expect("try_wait").is_none(),
        "a default profile must stay up rather than exit"
    );
    assert!(
        TcpStream::connect_timeout(&default_sam_address(), Duration::from_millis(200)).is_err(),
        "the default SAM port must stay unbound; no listener default may change"
    );
}

/// Opens a loopback connection and completes one SAM 3.1 handshake.
///
/// This is the proof that a listener is genuinely bound and serving: a
/// process that reported ready without accepting connections cannot satisfy
/// it.
fn sam_hello_once(address: SocketAddr) -> std::io::Result<String> {
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(2))?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    stream.write_all(b"HELLO VERSION MIN=3.1 MAX=3.1\n")?;
    stream.flush()?;
    let mut reply = String::new();
    BufReader::new(stream).read_line(&mut reply)?;
    Ok(reply.trim_end_matches(['\r', '\n']).to_owned())
}

/// Polls until the SAM listener answers or the deadline passes, panicking
/// with the last observed error if the daemon never binds. Returns the
/// handshake reply on success.
fn await_sam_ready(router: &mut RunningRouter) -> String {
    let started = Instant::now();
    let mut last_error = String::from("no attempt was made");
    while started.elapsed() < READY_DEADLINE {
        if let Ok(Some(status)) = router.child.try_wait() {
            panic!("the daemon exited with {status} before binding a listener");
        }
        match sam_hello_once(router.address) {
            Ok(reply) => return reply,
            Err(error) => last_error = error.to_string(),
        }
        std::thread::sleep(POLL_INTERVAL);
    }
    panic!("the listener never became ready within {READY_DEADLINE:?}: {last_error}");
}

#[test]
fn run_binds_configured_listener_and_shuts_down_cleanly() {
    let router = Router::with_sam();
    router.generate_identity();
    let mut child = router.spawn();

    let reply = await_sam_ready(&mut child);
    assert!(
        reply.starts_with("HELLO REPLY RESULT=OK VERSION=3.1"),
        "expected SAM HELLO OK from the bound listener, got {reply:?}"
    );

    // Hold the router up past its bind deadline and re-probe. A second,
    // independent connection also proves the listener serves more than one
    // accept.
    std::thread::sleep(HOLD_OPEN);
    assert!(
        child.child.try_wait().expect("try_wait").is_none(),
        "the daemon must stay up after binding, not exit once its bind deadline passes"
    );
    let second = sam_hello_once(router.sam_address)
        .expect("the listener must still be serving after its bind deadline");
    assert!(
        second.starts_with("HELLO REPLY RESULT=OK VERSION=3.1"),
        "expected the listener to keep serving, got {second:?}"
    );

    child.interrupt();
    let (status, output) = child.finish();

    assert!(
        status.success(),
        "expected a clean exit, got {status}\n{output}"
    );
    assert!(
        output.contains("router shutdown cleanly"),
        "expected a clean shutdown on SIGINT, got:\n{output}"
    );
    assert!(
        !output.contains("ReadinessTimeout"),
        "the daemon must not exit on a readiness timeout:\n{output}"
    );

    // No orphaned listener: once the child is gone the port must be free.
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if TcpStream::connect_timeout(&router.sam_address, Duration::from_millis(200)).is_err() {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the listener outlived the daemon at {}",
            router.sam_address
        );
        std::thread::sleep(POLL_INTERVAL);
    }
}

#[test]
fn router_network_sam_keeps_serving_without_claiming_product_readiness() {
    let address = reserve_loopback_port();
    let router = Router::new(|data_dir| sam_config(address, data_dir));
    router.generate_identity();

    let mut child = router.spawn();
    let hello = await_sam_ready(&mut child);
    assert!(hello.starts_with("HELLO REPLY RESULT=OK VERSION=3.1"));

    let mut client = TcpStream::connect_timeout(&router.sam_address, Duration::from_secs(2))
        .expect("connect to the real SAM listener");
    client
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("set SAM reply deadline");
    client
        .write_all(
            b"HELLO VERSION MIN=3.1 MAX=3.1\nSESSION CREATE STYLE=STREAM ID=network DESTINATION=TRANSIENT\n",
        )
        .expect("send SAM CREATE in the default router-network mode");
    let mut reader = BufReader::new(client);
    let mut hello = String::new();
    let mut session = String::new();
    reader.read_line(&mut hello).expect("read SAM HELLO");
    reader
        .read_line(&mut session)
        .expect("read SAM SESSION CREATE");
    assert!(hello.starts_with("HELLO REPLY RESULT=OK VERSION=3.1"));
    assert!(
        session.starts_with("SESSION STATUS RESULT=I2P_ERROR"),
        "missing router product must refuse SESSION CREATE: {session:?}"
    );
    assert!(session.contains("router-backed SAM destination product is unavailable"));
    drop(reader);

    child.interrupt();
    let (status, output) = child.finish();
    assert!(
        status.success(),
        "router shutdown failed: {status}\n{output}"
    );
}

#[test]
fn router_network_sam_with_no_service_groups_keeps_router_serving() {
    let address = reserve_loopback_port();
    let ssu2_port = reserve_loopback_udp_port();
    let router = Router::new(|data_dir| {
        controlled_ssu2_sam_config(address, ssu2_port, data_dir)
            .replace("product_mode = \"local-only\"\n", "")
    });
    router.generate_identity();

    let mut child = router.spawn();
    let hello = await_sam_ready(&mut child);
    assert!(hello.starts_with("HELLO REPLY RESULT=OK VERSION=3.1"));

    let mut client = TcpStream::connect_timeout(&router.sam_address, Duration::from_secs(2))
        .expect("connect to the real SAM listener");
    client
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("set SAM reply deadline");
    client
        .write_all(
            b"HELLO VERSION MIN=3.1 MAX=3.1\nSESSION CREATE STYLE=STREAM ID=network DESTINATION=TRANSIENT\n",
        )
        .expect("send SAM CREATE in router-network mode");
    let mut reader = BufReader::new(client);
    let mut hello = String::new();
    let mut session = String::new();
    reader.read_line(&mut hello).expect("read SAM HELLO");
    reader
        .read_line(&mut session)
        .expect("read SAM SESSION CREATE");
    assert!(hello.starts_with("HELLO REPLY RESULT=OK VERSION=3.1"));
    assert!(
        session.starts_with("SESSION STATUS RESULT=I2P_ERROR"),
        "an empty product must still reject remote SESSION CREATE: {session:?}"
    );
    assert!(
        session.contains("router-backed SAM destination provisioning failed"),
        "a running product must report its failed route provisioning: {session:?}"
    );
    drop(reader);

    assert!(
        child.child.try_wait().expect("try_wait").is_none(),
        "missing SAM destination composition must not stop the router"
    );
    child.interrupt();
    let (status, output) = child.finish();
    assert!(
        status.success(),
        "router shutdown failed: {status}\n{output}"
    );
}

#[test]
fn controlled_ssu2_and_sam_survive_real_process_restart() {
    let address = reserve_loopback_port();
    let ssu2_port = reserve_loopback_udp_port();
    let router = Router::new(|data_dir| controlled_ssu2_sam_config(address, ssu2_port, data_dir));
    router.generate_identity();

    let mut first = router.spawn();
    let reply = await_sam_ready(&mut first);
    assert!(reply.starts_with("HELLO REPLY RESULT=OK VERSION=3.1"));
    let conflict = std::net::UdpSocket::bind(("127.0.0.1", ssu2_port))
        .expect_err("the ordinary daemon must own the configured SSU2 UDP socket");
    assert_eq!(conflict.kind(), std::io::ErrorKind::AddrInUse);

    let transport_identity = router.data_dir.join("ssu2.transport.identity");
    let first_identity = std::fs::read(&transport_identity)
        .expect("the normal SSU2 owner must persist its transport identity");
    assert!(!first_identity.is_empty());

    let mut client = TcpStream::connect_timeout(&router.sam_address, Duration::from_secs(2))
        .expect("connect to the real SAM listener");
    client
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("set SAM read deadline");
    client
        .write_all(b"HELLO VERSION MIN=3.1 MAX=3.1\nSESSION CREATE STYLE=STREAM ID=gate-a DESTINATION=TRANSIENT\n")
        .expect("send SAM session create");
    let mut reader = BufReader::new(client);
    let mut hello = String::new();
    let mut session = String::new();
    reader.read_line(&mut hello).expect("read SAM HELLO");
    reader
        .read_line(&mut session)
        .expect("read SAM SESSION CREATE");
    assert!(
        hello.starts_with("HELLO REPLY RESULT=OK VERSION=3.1"),
        "{hello:?}"
    );
    assert!(
        session.starts_with("SESSION STATUS RESULT=OK"),
        "{session:?}"
    );
    drop(reader);

    first.interrupt();
    let (status, output) = first.finish();
    assert!(status.success(), "first run failed: {status}\n{output}");

    let mut second = router.spawn();
    let reply = await_sam_ready(&mut second);
    assert!(reply.starts_with("HELLO REPLY RESULT=OK VERSION=3.1"));
    let second_identity = std::fs::read(&transport_identity)
        .expect("restart must retain the SSU2 transport identity");
    assert_eq!(
        first_identity, second_identity,
        "transport identity rotated on restart"
    );
    second.interrupt();
    let (status, output) = second.finish();
    assert!(status.success(), "second run failed: {status}\n{output}");
}

#[test]
fn controlled_ntcp2_binds_loopback_and_reuses_identity_bound_keys_after_restart() {
    let address = reserve_loopback_port();
    let router = Router::new(|data_dir| controlled_ntcp2_config(address.port(), data_dir));
    router.generate_identity();
    let key_path = router.data_dir.join("ntcp2.transport.identity");

    let mut first = router.spawn();
    let mut first_socket = None;
    let deadline = Instant::now() + READY_DEADLINE;
    while Instant::now() < deadline {
        if key_path.exists()
            && let Ok(socket) = TcpStream::connect_timeout(&address, Duration::from_millis(200))
        {
            first_socket = Some(socket);
            break;
        }
        if let Some(status) = first.child.try_wait().expect("check daemon") {
            let (_, output) = first.finish();
            panic!("NTCP2 daemon exited before bind ({status}):\n{output}");
        }
        std::thread::sleep(POLL_INTERVAL);
    }
    drop(first_socket.expect("controlled NTCP2 listener did not bind"));
    assert!(
        controlled_ntcp2_client_session(&router, address, 3).is_err(),
        "the daemon must reject a wrong-network authenticated session"
    );
    controlled_ntcp2_client_session(&router, address, 2)
        .expect("matching-network client must authenticate to the real daemon process");
    let first_keys = std::fs::read(&key_path).expect("persist NTCP2 identity-bound keys");
    assert!(!first_keys.is_empty());
    first.interrupt();
    let (status, output) = first.finish();
    assert!(status.success(), "shutdown failed: {status}\n{output}");

    let mut second = router.spawn();
    let mut second_socket = None;
    let deadline = Instant::now() + READY_DEADLINE;
    while Instant::now() < deadline {
        if let Ok(socket) = TcpStream::connect_timeout(&address, Duration::from_millis(200)) {
            second_socket = Some(socket);
            break;
        }
        if let Some(status) = second.child.try_wait().expect("check restarted daemon") {
            let (_, output) = second.finish();
            panic!("restarted NTCP2 daemon exited before bind ({status}):\n{output}");
        }
        std::thread::sleep(POLL_INTERVAL);
    }
    drop(second_socket.expect("restarted NTCP2 listener did not bind"));
    let second_keys = std::fs::read(&key_path).expect("read restarted NTCP2 keys");
    assert_eq!(second_keys, first_keys, "restart must preserve key and IV");
    second.interrupt();
    let (status, output) = second.finish();
    assert!(
        status.success(),
        "restarted shutdown failed: {status}\n{output}"
    );
}

#[test]
fn occupied_controlled_ntcp2_port_degrades_without_stopping_router() {
    let address = reserve_loopback_port();
    let ssu2_port = reserve_loopback_udp_port();
    let router = Router::new(|data_dir| {
        format!(
            "{}\n[ssu2]\nenabled = true\nbind_ipv4 = \"127.0.0.1\"\nbind_ipv6 = \"\"\nport = {}\nadvertise = false\nintroducer_service = false\n",
            controlled_ntcp2_config(address.port(), data_dir),
            ssu2_port,
        )
    });
    router.generate_identity();
    let occupier = TcpListener::bind(address).expect("occupy configured NTCP2 port");

    let mut child = router.spawn();
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        if let Some(status) = child.child.try_wait().expect("check daemon") {
            let (_, output) = child.finish();
            panic!("optional NTCP2 bind failure stopped the router ({status}):\n{output}");
        }
        std::thread::sleep(POLL_INTERVAL);
    }
    assert!(
        child
            .child
            .try_wait()
            .expect("router remains running")
            .is_none(),
        "the optional NTCP2 bind failure must not stop the daemon"
    );
    let ssu2_conflict = std::net::UdpSocket::bind(("127.0.0.1", ssu2_port))
        .expect_err("SSU2 must remain bound while NTCP2 degrades");
    assert_eq!(ssu2_conflict.kind(), std::io::ErrorKind::AddrInUse);
    drop(occupier);
    child.interrupt();
    let (status, output) = child.finish();
    assert!(
        status.success(),
        "router shutdown failed: {status}\n{output}"
    );
    assert!(
        !router.data_dir.join("ntcp2.transport.identity").exists(),
        "a failed pre-key bind must not create NTCP2 key material"
    );
}

#[test]
fn occupied_loopback_port_is_refused_with_a_named_diagnostic() {
    let router = Router::with_sam();
    // Occupy the configured loopback port for the whole run so the daemon
    // genuinely cannot start.
    let occupier = TcpListener::bind(router.sam_address).expect("occupy configured port");

    router.generate_identity();
    let mut child = router.spawn();
    let status = child
        .wait_for_exit(READY_DEADLINE)
        .expect("a router that cannot bind must exit rather than hang");
    let (_, output) = child.finish();
    drop(occupier);

    assert_eq!(
        status.code(),
        Some(EXIT_SUPERVISOR_FAILED),
        "expected the supervisor-failed exit code, got {status}\n{output}"
    );
    assert!(
        output.contains("sam-bridge"),
        "the diagnostic must name the service that failed:\n{output}"
    );
    assert!(
        output.contains("bind failed"),
        "the diagnostic must name the reason:\n{output}"
    );
    assert!(
        output.contains("readiness deadline"),
        "the diagnostic must name the deadline:\n{output}"
    );
    assert!(
        !output.contains("ReadinessTimeout"),
        "a bind failure must not be reported as an anonymous readiness timeout:\n{output}"
    );
}

#[test]
fn run_refuses_without_an_identity() {
    let router = Router::new(default_config);
    let output = command()
        .args(["run", "--config"])
        .arg(&router.config)
        .output()
        .expect("run without identity");
    assert_eq!(output.status.code(), Some(EXIT_IDENTITY));
    assert!(String::from_utf8_lossy(&output.stderr).contains("router identity not found"));
}

#[test]
fn default_profile_starts_without_binding_a_listener() {
    let router = Router::new(default_config);
    router.generate_identity();

    // Nothing is enabled, so there is nothing to bind. The daemon must still
    // reach a running state: readiness means "this service is running", not
    // "a listener exists", and no listener default was widened to pass the
    // rows above. The binds-nothing half is daemon-scoped (see
    // `assert_no_child_listeners`), never a probe of a host-global port.
    let mut child = router.spawn();
    assert_no_child_listeners(&mut child);

    child.interrupt();
    let (status, output) = child.finish();
    assert!(
        status.success(),
        "expected a clean exit, got {status}\n{output}"
    );
    assert!(
        output.contains("router shutdown cleanly"),
        "a default profile must start and shut down cleanly:\n{output}"
    );
    assert!(
        !output.contains("ReadinessTimeout"),
        "the daemon must not exit on a readiness timeout:\n{output}"
    );
}
