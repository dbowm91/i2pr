//! Plan 476 — the sanitized local receipt export.
//!
//! These rows are deliberately **black-box**: they drive the shipped
//! `i2pr` executable through its CLI exactly as an operator does, then
//! observe the exported receipt from outside. No private bootstrap,
//! supervisor, or observer API is called from this file, so a row
//! cannot pass by exercising an internal seam the product does not
//! use.
//!
//! What each row proves:
//!
//! * `daemon_writes_a_sanitized_local_receipt` — the export tick is
//!   actually wired into the composition root. A module that compiles
//!   but is never called would leave no file, and this row is the only
//!   thing that distinguishes the two.
//! * `the_exported_receipt_carries_no_secret_material` — the receipt
//!   is count-only by construction: no key, no RouterInfo, no address,
//!   no endpoint, no payload, and no transient message ID.
//! * `the_exported_receipt_is_private_to_the_operator` — the file is
//!   created with mode `0600`, not with the ambient umask.
//! * `a_relative_export_destination_is_refused` — the readout cannot
//!   be pointed at a relative path, so it cannot resolve against an
//!   unexpected working directory.
//!
//! Timing note: the composition root runs in a child process, so a
//! paused Tokio clock cannot govern it. Every wait below is an
//! explicit bounded deadline over wall-clock time with a coarse poll
//! interval. There are no unbounded waits and no sleeps used to
//! assert behaviour.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;
use tempfile::TempDir;

/// Upper bound on how long the daemon may take to write the receipt.
const RECEIPT_DEADLINE: Duration = Duration::from_secs(30);

/// Coarse poll interval while waiting for the child to write the
/// receipt or exit.
const POLL_INTERVAL: Duration = Duration::from_millis(100);

/// The canonical local phase vocabulary. Must stay equal to
/// `i2pr_daemon::ssu2_qualification_receipts::LOCAL_PHASES`; the
/// Plan 476 binding checker compares the two.
const LOCAL_PHASES: [&str; 9] = [
    "permit_checked",
    "router_started",
    "peer_session_accepted",
    "peer_session_initiated",
    "peer_authenticated",
    "request_sent",
    "request_received",
    "product_effect_observed",
    "router_stopped",
];

/// A router process under test, with its own temporary state.
struct Router {
    _directory: TempDir,
    config: PathBuf,
    receipt: PathBuf,
}

impl Router {
    /// Builds a router whose SSU2 transport is bound to loopback and
    /// whose sanitized receipt is exported to a private path.
    ///
    /// The controlled-loopback profile is used on purpose: it is the
    /// one qualification scope that needs no RFC1918 interface, so
    /// the row is exercisable on any host. The private-LAN profile is
    /// exercised by the profile-scope unit test in the daemon crate.
    fn new() -> Router {
        let directory = tempfile::tempdir().expect("temp directory");
        let data_dir = directory.path().join("state");
        std::fs::create_dir(&data_dir).expect("create data dir");
        // The identity store refuses a permissive directory, so create
        // the data dir 0700 rather than relying on the ambient umask.
        set_mode(&data_dir, 0o700);
        let receipt = directory.path().join("local-receipt.json");
        let config = directory.path().join("router.toml");
        std::fs::write(
            &config,
            format!(
                "schema_version = 1\n\n[router]\ndata_dir = {:?}\n\n[ssu2]\nenabled = true\nbind_ipv4 = \"127.0.0.1\"\nport = 0\nqualification_profile = \"controlled_loopback\"\nqualification_receipt_path = {:?}\n",
                data_dir.to_string_lossy(),
                receipt.to_string_lossy()
            ),
        )
        .expect("write config");
        Router {
            _directory: directory,
            config,
            receipt,
        }
    }

    /// Runs `identity generate` through the CLI, so the row exercises
    /// the same path an operator does.
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

    fn spawn(&self) -> Child {
        command()
            .args(["run", "--config"])
            .arg(&self.config)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn router")
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

/// Waits for the exported receipt to appear and to parse, or gives up
/// at an explicit deadline.
fn wait_for_receipt(path: &Path, child: &mut Child) -> Option<Value> {
    let deadline = Instant::now() + RECEIPT_DEADLINE;
    while Instant::now() < deadline {
        if let Ok(text) = std::fs::read_to_string(path)
            && let Ok(value) = serde_json::from_str::<Value>(&text)
        {
            return Some(value);
        }
        if let Some(status) = child.try_wait().expect("poll child") {
            assert!(
                status.success(),
                "router exited before writing the receipt: {status}"
            );
        }
        std::thread::sleep(POLL_INTERVAL);
    }
    None
}

/// Terminates the child and reaps it.
fn stop(child: &mut Child) {
    // SIGINT is the daemon's documented shutdown signal.
    let _ = Command::new("kill")
        .arg("-INT")
        .arg(child.id().to_string())
        .status();
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        if child.try_wait().expect("poll child").is_some() {
            return;
        }
        std::thread::sleep(POLL_INTERVAL);
    }
    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn daemon_writes_a_sanitized_local_receipt() {
    let router = Router::new();
    router.generate_identity();
    let mut child = router.spawn();
    let receipt = wait_for_receipt(&router.receipt, &mut child);
    stop(&mut child);
    let receipt = receipt.expect("daemon must write the local receipt");

    assert_eq!(
        receipt.get("schema").and_then(Value::as_str),
        Some("i2pr-plan476-local-receipt-v1")
    );
    assert_eq!(
        receipt.get("profile").and_then(Value::as_str),
        Some("controlled_loopback")
    );
    // A locally observed export is never a two-host claim.
    assert_eq!(
        receipt.get("live_qualification").and_then(Value::as_str),
        Some("not-run")
    );
    assert!(
        receipt.get("error").is_some(),
        "error field must be present"
    );
    assert!(
        receipt.get("error").and_then(Value::as_str).is_none(),
        "a quiet daemon must not report a collector failure"
    );

    let phases = receipt
        .get("phases")
        .and_then(Value::as_array)
        .expect("phases array");
    for phase in phases {
        let name = phase
            .get("phase")
            .and_then(Value::as_str)
            .expect("phase name");
        assert!(
            LOCAL_PHASES.contains(&name),
            "phase {name} is outside the canonical vocabulary"
        );
        let count = phase
            .get("count")
            .and_then(Value::as_u64)
            .expect("phase count");
        assert!(
            count > 0,
            "phase {name} must not be exported with a zero count"
        );
    }
    // The export is permissioned, so the permit phase is always
    // present even before any observation.
    assert!(
        phases
            .iter()
            .any(|phase| phase.get("phase").and_then(Value::as_str) == Some("permit_checked")),
        "the permit phase must be present"
    );
}

#[test]
fn the_exported_receipt_carries_no_secret_material() {
    let router = Router::new();
    router.generate_identity();
    let mut child = router.spawn();
    let receipt = wait_for_receipt(&router.receipt, &mut child);
    stop(&mut child);
    let receipt = receipt.expect("daemon must write the local receipt");
    let text = serde_json::to_string(&receipt).expect("serialize receipt");
    for forbidden in [
        "message_id",
        "messageId",
        "peer_commitment",
        "peerCommitment",
        "endpoint",
        "address",
        "router_info",
        "routerInfo",
        "key",
        "payload",
        "secret",
        "0x",
    ] {
        assert!(
            !text.contains(forbidden),
            "the exported receipt must not contain {forbidden}: {text}"
        );
    }
}

#[test]
fn the_exported_receipt_is_private_to_the_operator() {
    let router = Router::new();
    router.generate_identity();
    let mut child = router.spawn();
    let receipt = wait_for_receipt(&router.receipt, &mut child);
    stop(&mut child);
    assert!(receipt.is_some(), "daemon must write the local receipt");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&router.receipt)
            .expect("receipt metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600, "the exported receipt must be private");
    }
}

#[test]
fn a_relative_export_destination_is_refused() {
    let directory = tempfile::tempdir().expect("temp directory");
    let data_dir = directory.path().join("state");
    std::fs::create_dir(&data_dir).expect("create data dir");
    set_mode(&data_dir, 0o700);
    let config = directory.path().join("router.toml");
    std::fs::write(
        &config,
        format!(
            "schema_version = 1\n\n[router]\ndata_dir = {:?}\n\n[ssu2]\nenabled = true\nbind_ipv4 = \"127.0.0.1\"\nport = 0\nqualification_receipt_path = \"relative-receipt.json\"\n",
            data_dir.to_string_lossy()
        ),
    )
    .expect("write config");
    let output = command()
        .args(["run", "--config"])
        .arg(&config)
        .output()
        .expect("run router");
    assert!(
        !output.status.success(),
        "a relative export destination must be refused"
    );
}
