//! Plan 360 — `i2pr run` must actually start the router.
//!
//! These rows are deliberately **black-box**: they drive the shipped `i2pr`
//! executable through its CLI exactly as an operator does, then observe the
//! result over loopback TCP. No private bootstrap, supervisor, manager, or
//! pump API is called from this file, so a row cannot pass by exercising an
//! internal seam the product does not use.
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
//!   depend on a listener existing.
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

/// Exit code the CLI reserves for a supervisor failure.
const EXIT_SUPERVISOR_FAILED: i32 = 46;

/// Exit code the CLI reserves for a missing or invalid router identity.
const EXIT_IDENTITY: i32 = 41;

/// A test-owned config file plus the router data directory it points at.
struct Router {
    _directory: TempDir,
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
        "schema_version = 1\n\n[router]\ndata_dir = {:?}\n\n[sam]\nenabled = true\nbind_address = \"127.0.0.1\"\nport = {}\n",
        data_dir.to_string_lossy(),
        address.port()
    )
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
    stdout: std::thread::JoinHandle<String>,
    stderr: std::thread::JoinHandle<String>,
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
            stdout: drain(stdout),
            stderr: drain(stderr),
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
        let stdout = self.stdout.join().unwrap_or_default();
        let stderr = self.stderr.join().unwrap_or_default();
        (status, format!("{stdout}\n{stderr}"))
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

/// The documented default loopback SAM address, which must stay unbound.
fn default_sam_address() -> SocketAddr {
    "127.0.0.1:7656".parse().expect("default SAM address")
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
    // rows above.
    let mut child = router.spawn();
    std::thread::sleep(POLL_INTERVAL * 4);
    assert!(
        child.child.try_wait().expect("try_wait").is_none(),
        "a default profile must stay up rather than exit"
    );
    // No listener default was widened: the default SAM port is not bound.
    assert!(
        TcpStream::connect_timeout(&default_sam_address(), Duration::from_millis(200)).is_err(),
        "the default SAM port must stay unbound; no listener default may change"
    );

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
