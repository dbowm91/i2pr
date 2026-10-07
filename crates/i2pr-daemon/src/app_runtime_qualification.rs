//! Plan 369 WP5 — black-box qualification of the whole managed-application path.
//!
//! # What is actually under test
//!
//! Each case here drives the complete product chain with real operating-system
//! processes and no in-memory stand-in for any of them:
//!
//! ```text
//! daemon AppManagerBridge  ──anonymous pipes──▶  i2pr-app-fixture-manager
//!         (real)                                       (real i2pr-appd::Appd)
//!                                                            │ resolves sibling
//!                                                            ▼
//!                                                  i2pr-apphost  (real process)
//!                                                            │ execs, no shell
//!                                                            ▼
//!                                                  i2pr-app-fixture  (real process)
//!                                                            │ app v1 on stdin/stdout
//!                                                            ▼
//!                            AppGatewaySession ──▶ SamServiceState / I2cpServiceState
//! ```
//!
//! The only seam used is the *existing* Plan 369 §4 test-only composition path
//! for the manager executable. Nothing here reaches into manager or gateway
//! internals to make an assertion pass; assertions read the fixture's own
//! transcript, the manager's stderr, and the bridge's own counts.
//!
//! # Why the harness spawns the manager itself
//!
//! The supervisor path (`run_manager_service` → `spawn_manager`) is proven
//! separately by `tests/app_runtime_supervision.rs`. Driving it here would force
//! the fixture manager to be launched with **no arguments and a cleared
//! environment**, which is exactly what makes scenario selection impossible.
//! Spawning the same real binary over the same real pipes keeps every part of
//! the chain under test while letting each case name its own behavior.

use std::collections::BTreeSet;
use std::fs::File;
use std::io::Write;
use std::net::{IpAddr, Ipv4Addr};
use std::os::fd::OwnedFd;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use i2pr_runtime::{CancellationToken, ChildFailurePolicy, ChildScope};
use tokio::io::AsyncReadExt;
use tokio::net::unix::pipe::{Receiver, Sender};
use tokio::process::{Child, Command};
use tokio::sync::oneshot;
use tokio::time::Instant;

use crate::app_manager_bridge::{AppManagerBridge, AppManagerComposition};
use crate::config::{I2cpConfig, SamConfig};

/// The fixture manager binary, standing in for the shipped `i2pr-appd` sibling.
const FIXTURE_MANAGER: &str = "i2pr-app-fixture-manager";
/// The fixture application `i2pr-apphost` will exec.
const FIXTURE_APP: &str = "i2pr-app-fixture";
/// The apphost, resolved by `i2pr-appd`'s own unchanged sibling rule.
const APPHOST: &str = "i2pr-apphost";

/// Every wait in this module is bounded, so a defect fails the case rather than
/// hanging the suite.
const DEADLINE: Duration = Duration::from_secs(180);

/// How often the transcript and the manager's stderr are re-read.
const POLL_STEP: Duration = Duration::from_millis(25);

/// Transcript steps that mean a request is outstanding.
///
/// The quiet period exists to detect a *finished* run, including one that ended
/// in a refusal and therefore never wrote a terminal step. It must not fire while
/// a request is still in flight: SAM session creation and I2CP handshake
/// generation both take real time in a debug build, and a gap of a second and a
/// half between "sent" and "answered" is ordinary, not a hang.
const PENDING_STEPS: &[&str] = &["sam-hello", "sam-session-create", "i2cp-get-date"];

/// How long the transcript must stop growing before a run counts as settled.
///
/// A run that ends in a refusal never writes a terminal record at all, so
/// "quiet for this long" is what distinguishes a finished run from one that is
/// still working. It has to be comfortably longer than the gap between any two
/// real steps, or a slow-but-correct SAM or I2CP exchange would be mistaken for
/// a finished run and cancelled mid-flight.
const QUIET_PERIOD: Duration = Duration::from_millis(1500);

/// Bounded grace before the manager is killed directly, mirroring
/// `app_runtime::MANAGER_EXIT_GRACE`.
///
/// The manager has no shutdown channel by design — EOF is the only signal, and
/// the daemon holds the write half of that pipe for as long as its bridge future
/// exists. So on shutdown the shipped daemon does not *ask* the manager to stop:
/// it waits this long and then terminates the direct child. The harness
/// reproduces that exactly rather than inventing a gentler shutdown, because
/// "terminated and reaped within a bound" is the property Plan 369 §B states.
const MANAGER_EXIT_GRACE: Duration = Duration::from_secs(5);

/// How long a bridge that is about to be discarded is given to finish by itself.
///
/// Long enough for a bridge that has already seen end-of-stream to complete its
/// own teardown and report why, short enough that a bridge still waiting on the
/// manager does not extend the run.
const TEARDOWN_GRACE: Duration = Duration::from_secs(2);

/// Bounded read of the manager's stderr.
///
/// The manager's stderr is diagnostics and never protocol, so reading it here
/// cannot affect the qualification; it is evidence of *why* a launch ended.
const STDERR_CEILING: usize = 256 * 1024;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::addressbook::SharedAddressBook;
    use ed25519_dalek::{Signer, SigningKey};
    use i2pr_app_proto::{AppId, Capability, LaunchProfile};
    use i2pr_app_state::{AppPolicy, AppStateStore, ResourceCeilings};
    use sha2::{Digest, Sha256};
    use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

    // -- harness ------------------------------------------------------------

    /// A temporary directory that owns one run's transcript.
    ///
    /// Ordinary runs delete their scratch directory on drop. Setting
    /// `I2PR_APP_FIXTURE_EVIDENCE_DIR` keeps it instead, so a closure record can
    /// quote a real transcript rather than a reconstruction of one. The
    /// environment read is confined to this `#[cfg(test)]` module and changes
    /// only *retention* -- the transcript content, the fixture, and every
    /// assertion are identical either way, so retaining evidence cannot change
    /// what a green run proves.
    struct Scratch {
        path: PathBuf,
        retained: bool,
    }

    impl Scratch {
        fn new(label: &str) -> Self {
            let unique = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos();
            let name = format!("i2pr-app-fixture-{label}-{unique}");
            let retained = std::env::var_os(EVIDENCE_DIR_VAR).is_some();
            let path = match std::env::var_os(EVIDENCE_DIR_VAR) {
                Some(root) => PathBuf::from(root).join(name),
                None => std::env::temp_dir().join(name),
            };
            std::fs::create_dir_all(&path).expect("scratch dir");
            Self { path, retained }
        }

        fn transcript(&self) -> PathBuf {
            self.path.join("transcript.jsonl")
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            if !self.retained {
                let _ = std::fs::remove_dir_all(&self.path);
            }
        }
    }

    /// Opt-in evidence retention. See [`Scratch`].
    const EVIDENCE_DIR_VAR: &str = "I2PR_APP_FIXTURE_EVIDENCE_DIR";

    /// The Cargo target directory these binaries were built into.
    ///
    /// An in-crate test binary lives in `<target>/debug/deps`, and the fixture,
    /// the manager, and the apphost are all plain bin targets in
    /// `<target>/debug`. Walking up is therefore exact rather than a guess, and
    /// it is the same rule `i2pr-appd` itself uses to find its apphost sibling.
    fn target_dir() -> PathBuf {
        let executable = std::env::current_exe().expect("current test executable");
        executable
            .parent()
            .and_then(Path::parent)
            .expect("target directory")
            .to_path_buf()
    }

    /// Resolves one fixture binary, failing with the build command that produces it.
    fn sibling(name: &str) -> PathBuf {
        let path = target_dir().join(name);
        assert!(
            path.is_file(),
            "{name} was not found at {} — build the workspace first, e.g. \
             `cargo build --locked --workspace --all-targets`",
            path.display()
        );
        path
    }

    fn sha256_hex(bytes: &[u8]) -> String {
        Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    fn signed_fixture_package(path: &Path, app_id: &AppId, application: &[u8]) -> String {
        let signing = SigningKey::from_bytes(&[73; 32]);
        let key = signing.verifying_key().to_bytes();
        let publisher = sha256_hex(&key);
        let manifest = serde_json::json!({
            "schema_version": 1,
            "app_id": app_id,
            "publisher_id": publisher,
            "version": "1.0.0",
            "name": "managed catalog fixture",
            "description": "",
            "host_protocol_min": {"major": 1, "minor": 0},
            "host_protocol_max": {"major": 1, "minor": 0},
            "entrypoints": [{"target": i2pr_app_state::target_triple().unwrap(), "path": "bin/app"}],
            "requested_capabilities": ["sam", "i2cp"],
            "resources": [],
            "ui": null,
            "autostart_requested": false,
            "restart_requested": false
        });
        let manifest = serde_json::to_vec(&manifest).unwrap();
        let inventory = serde_json::json!([{
            "path": "bin/app",
            "size": application.len(),
            "sha256": sha256_hex(application),
            "executable": true
        }]);
        let inventory = serde_json::to_vec(&inventory).unwrap();
        let mut transcript = b"I2PR-APP-PACKAGE-V1\0".to_vec();
        transcript.extend_from_slice(&(manifest.len() as u32).to_be_bytes());
        transcript.extend_from_slice(&manifest);
        transcript.extend_from_slice(&(inventory.len() as u32).to_be_bytes());
        transcript.extend_from_slice(&inventory);
        transcript.extend_from_slice(&key);
        let signature = signing.sign(&transcript).to_bytes();
        let mut zip = ZipWriter::new(File::create(path).unwrap());
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        for (name, bytes) in [
            ("manifest.json", manifest.as_slice()),
            ("inventory.json", inventory.as_slice()),
            ("publisher.ed25519", key.as_slice()),
            ("signature.ed25519", signature.as_slice()),
            ("payload/bin/app", application),
        ] {
            zip.start_file(name, options).unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
        publisher
    }

    fn prepare_persistent_catalog(root: &Path, fixture_binary: &Path) -> (PathBuf, Vec<AppId>) {
        let managed_root = root.join("managed-apps");
        let store = AppStateStore::open(&managed_root).expect("state store opens");
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let app_ids = [
            AppId::parse(format!("catalog.{nonce}.sam")).expect("SAM fixture app id"),
            AppId::parse(format!("catalog.{nonce}.i2cp")).expect("I2CP fixture app id"),
        ];
        let broken_app_id =
            AppId::parse(format!("catalog.{nonce}.bad")).expect("broken fixture app id");
        let application = std::fs::read(fixture_binary).expect("fixture application bytes");
        let mut installed = Vec::new();
        let mut publisher = None;
        let mut broken_package = None;
        for (index, app_id) in app_ids.iter().chain([&broken_app_id]).enumerate() {
            let archive = root.join(format!("catalog-{index}.i2prapp"));
            let current_publisher = signed_fixture_package(&archive, app_id, &application);
            if let Some(expected) = &publisher {
                assert_eq!(expected, &current_publisher);
            } else {
                publisher = Some(current_publisher.clone());
            }
            let package = store
                .install_package(&archive)
                .expect("signed fixture installs");
            assert_eq!(package.identity.app_id, *app_id);
            installed.push(package.identity.clone());
            if app_id == &broken_app_id {
                broken_package = Some(package);
            }
            std::fs::remove_file(archive).expect("remove package input");
        }
        let publisher = publisher.expect("publisher fingerprint");
        store
            .mutate(|state| {
                state.trusted_publishers.push(publisher.clone());
                state.apps = installed
                    .iter()
                    .zip(app_ids.iter().chain([&broken_app_id]))
                    .map(|(identity, app_id)| AppPolicy {
                        publisher_id: publisher.clone(),
                        app_id: app_id.clone(),
                        selected: Some(identity.clone()),
                        granted_capabilities: if app_id.as_str().ends_with(".sam") {
                            vec![Capability::Sam]
                        } else {
                            vec![Capability::I2cp]
                        },
                        launch_profile: Some(LaunchProfile::UnsafeDirect),
                        autostart: true,
                        max_connections: 8,
                        resource_ceilings: ResourceCeilings::default(),
                    })
                    .collect();
                state.apps.sort_by(|a, b| {
                    (&a.publisher_id, &a.app_id).cmp(&(&b.publisher_id, &b.app_id))
                });
                Ok(())
            })
            .expect("persist trusted explicit launch policy");
        let broken_package = broken_package.expect("broken sibling package");
        let broken_payload = broken_package.path.join("payload/bin/app");
        let permissions = std::fs::metadata(&broken_payload)
            .expect("installed payload metadata")
            .permissions();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(
                &broken_payload,
                std::fs::Permissions::from_mode(permissions.mode() | 0o200),
            )
            .expect("make test-owned installed payload owner-writable");
        }
        #[cfg(not(unix))]
        {
            let mut permissions = permissions;
            permissions.set_readonly(false);
            std::fs::set_permissions(&broken_payload, permissions)
                .expect("make test-owned installed payload writable");
        }
        std::fs::write(broken_payload, b"tampered")
            .expect("tamper the selected sibling after installation");
        (
            managed_root.canonicalize().expect("canonical managed root"),
            app_ids.to_vec(),
        )
    }

    /// The workspace `crates/` directory.
    ///
    /// Resolved from the manifest, not the working directory: an integration
    /// test runs with its crate root as CWD, so a relative path would look for
    /// `crates/i2pr-daemon/src/i2pr-app-fixture/src` and find nothing.
    fn crates_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("workspace crates directory")
            .to_path_buf()
    }

    /// Refuses a stale fixture binary.
    ///
    /// These binaries are *prebuilt siblings*, not Cargo targets of this test, so
    /// `cargo test -p i2pr-daemon` does not rebuild them. A qualification run
    /// against a stale manager is worse than no run at all: it reports the
    /// behaviour of code that is no longer there. That is not hypothetical — it
    /// hid a real missing-flush defect for several iterations of this module,
    /// because the manager under test predated the fix being tested.
    ///
    /// The comparison is per binary and lists only the sources that binary
    /// actually contains. A coarser "newest file anywhere in the crate" rule
    /// misfires on the first edit to either binary and would be muted, which is
    /// strictly worse than not having the guard.
    fn assert_fresh(binary: &Path, sources: &[&str]) {
        let binary_mtime = std::fs::metadata(binary)
            .and_then(|meta| meta.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        let newest = sources
            .iter()
            .filter_map(|relative| {
                std::fs::metadata(crates_dir().join(relative))
                    .and_then(|meta| meta.modified())
                    .ok()
            })
            .max()
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        assert!(
            binary_mtime >= newest,
            "{} predates its own sources ({sources:?}) — the qualification would \
             run stale code. Rebuild with `cargo build --locked --workspace \
             --all-targets` before testing.",
            binary.display()
        );
    }

    /// The daemon-side transport: two anonymous pipes, no listening socket.
    ///
    /// Built from the same `tokio::net::unix::pipe` primitives production uses,
    /// in one object rather than a pair, because the Plan-368 codec splits it
    /// with `tokio::io::split` and needs a single duplex.
    struct DaemonTransport {
        read: Receiver,
        write: Sender,
    }

    impl DaemonTransport {
        fn new(read: OwnedFd, write: OwnedFd) -> std::io::Result<Self> {
            Ok(Self {
                read: Receiver::from_owned_fd(read)?,
                write: Sender::from_owned_fd(write)?,
            })
        }
    }

    impl tokio::io::AsyncRead for DaemonTransport {
        fn poll_read(
            mut self: std::pin::Pin<&mut Self>,
            context: &mut std::task::Context<'_>,
            buffer: &mut tokio::io::ReadBuf<'_>,
        ) -> std::task::Poll<std::io::Result<()>> {
            std::pin::Pin::new(&mut self.read).poll_read(context, buffer)
        }
    }

    impl tokio::io::AsyncWrite for DaemonTransport {
        fn poll_write(
            mut self: std::pin::Pin<&mut Self>,
            context: &mut std::task::Context<'_>,
            bytes: &[u8],
        ) -> std::task::Poll<std::io::Result<usize>> {
            std::pin::Pin::new(&mut self.write).poll_write(context, bytes)
        }
        fn poll_flush(
            mut self: std::pin::Pin<&mut Self>,
            context: &mut std::task::Context<'_>,
        ) -> std::task::Poll<std::io::Result<()>> {
            std::pin::Pin::new(&mut self.write).poll_flush(context)
        }
        fn poll_shutdown(
            mut self: std::pin::Pin<&mut Self>,
            context: &mut std::task::Context<'_>,
        ) -> std::task::Poll<std::io::Result<()>> {
            std::pin::Pin::new(&mut self.write).poll_shutdown(context)
        }
    }

    /// The daemon-owned composition, identical to production inputs except that
    /// the private gateway never binds a listener regardless.
    fn composition() -> AppManagerComposition {
        let cancellation = CancellationToken::new();
        let children = ChildScope::for_test(&cancellation, ChildFailurePolicy::CollectResult);
        AppManagerComposition {
            sam: SamConfig {
                enabled: true,
                bind_address: IpAddr::V4(Ipv4Addr::LOCALHOST),
                port: 7656,
                limits: i2pr_api::sam::limits::SamLimits::loopback_test_profile(),
            },
            i2cp: I2cpConfig::loopback_test_profile(8, 16, 64 * 1024, 64),
            addressbook: SharedAddressBook::new(),
            children,
            cancellation,
        }
    }

    /// How many fixture instances to launch and which grants to issue.
    #[derive(Clone, Debug)]
    struct Plan {
        scenario: &'static str,
        capabilities: &'static str,
        instances: usize,
    }

    impl Plan {
        const fn new(scenario: &'static str, capabilities: &'static str) -> Self {
            Self {
                scenario,
                capabilities,
                instances: 1,
            }
        }

        const fn instances(mut self, count: usize) -> Self {
            self.instances = count;
            self
        }
    }

    /// Everything one run produced.
    struct Outcome {
        transcript: Vec<serde_json::Value>,
        manager_stderr: String,
        /// The reaped manager's status. Always `Some`: an unreaped child would
        /// have panicked rather than been reported.
        manager_status: Option<std::process::ExitStatus>,
        /// Whether the manager chose to exit rather than being killed.
        manager_exited_cleanly: bool,
        /// `true` when the bridge completed its manager-protocol handshake.
        bridge_ready: bool,
        sessions: usize,
        streams: usize,
        bridge_error: Option<String>,
    }

    impl Outcome {
        fn steps(&self) -> Vec<&str> {
            self.transcript
                .iter()
                .filter_map(|record| record.get("step").and_then(|s| s.as_str()))
                .collect()
        }

        fn recorded(&self, step: &str) -> bool {
            self.steps().contains(&step)
        }

        fn detail(&self, step: &str) -> &serde_json::Value {
            self.transcript
                .iter()
                .find(|record| record.get("step").and_then(|s| s.as_str()) == Some(step))
                .and_then(|record| record.get("detail"))
                .unwrap_or(&serde_json::Value::Null)
        }

        fn fixture_error(&self) -> Option<String> {
            self.transcript
                .iter()
                .find(|record| record.get("step").and_then(|s| s.as_str()) == Some("fixture-error"))
                .and_then(|record| record.get("detail").and_then(|d| d.as_str()))
                .map(str::to_owned)
        }

        /// Asserts the run finished cleanly, surfacing both sides on failure.
        fn assert_succeeded(&self, context: &str) {
            assert!(
                self.bridge_ready,
                "{context}: the manager never completed the manager-protocol handshake"
            );
            assert_eq!(
                self.bridge_error, None,
                "{context}: the daemon bridge ended with an error"
            );
            if let Some(error) = self.fixture_error() {
                panic!(
                    "{context}: the fixture reported failure: {error}\n  steps={:?}\n  stderr={:?}\n  bridge_error={:?} sessions={} streams={}",
                    self.steps(),
                    self.manager_stderr,
                    self.bridge_error,
                    self.sessions,
                    self.streams
                );
            }
            assert!(
                self.recorded("complete"),
                "{context}: the fixture never finished; steps were {:?}; stderr={:?}",
                self.steps(),
                self.manager_stderr
            );
            assert!(
                self.manager_status.is_some(),
                "{context}: the manager was not reaped"
            );
            // Deliberately *not* asserting the manager exited voluntarily. It
            // cannot: EOF is the manager's only shutdown signal and the daemon
            // holds the write half for as long as its bridge future exists, so
            // production terminates the direct child after a bounded grace. The
            // property Plan 369 §B states is that it is terminated and reaped,
            // which `manager_status` being `Some` proves.
            let _ = self.manager_exited_cleanly;
        }
    }

    /// Spawns the real fixture manager over real pipes and drives the real bridge.
    async fn run(label: &str, plan: Plan) -> Outcome {
        let manager = sibling(FIXTURE_MANAGER);
        // Proved here as well as used, so a missing apphost fails with a build
        // hint instead of a confusing launch refusal three layers down.
        let apphost = sibling(APPHOST);
        let application = sibling(FIXTURE_APP);
        // Each binary is checked only against what it actually contains. The
        // manager links `i2pr-appd` as a library, so a change there relinks it;
        // the application does not.
        assert_fresh(
            &manager,
            &[
                "i2pr-app-fixture/src/lib.rs",
                "i2pr-app-fixture/src/bin/manager.rs",
                "i2pr-appd/src",
            ],
        );
        assert_fresh(
            &application,
            &[
                "i2pr-app-fixture/src/lib.rs",
                "i2pr-app-fixture/src/main.rs",
            ],
        );
        // apphost is exec'd by the manager rather than by this test, but a stale
        // apphost would change the behaviour under test just as much.
        assert_fresh(&apphost, &["i2pr-apphost/src"]);

        let scratch = Scratch::new(label);
        let transcript_path = scratch.transcript();

        // Exactly the production split: the child inherits the read end of one
        // pipe and the write end of the other, and the daemon keeps the rest.
        let (to_manager_read, to_manager_write) = std::io::pipe().expect("daemon->manager pipe");
        let (from_manager_read, from_manager_write) =
            std::io::pipe().expect("manager->daemon pipe");

        let mut child: Child = Command::new(&manager)
            // Same discipline as production: no inherited environment.
            .env_clear()
            .arg(format!("--scenario={}", plan.scenario))
            .arg("--app-id=i2pr.fixture.app")
            .arg("--instance=101")
            .arg(format!("--capabilities={}", plan.capabilities))
            .arg(format!("--instances={}", plan.instances))
            .arg(format!("--transcript={}", transcript_path.display()))
            .stdin(Stdio::from(to_manager_read))
            .stdout(Stdio::from(from_manager_write))
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn the fixture manager");

        let mut stderr = child.stderr.take().expect("piped stderr");
        let stderr_task = tokio::spawn(async move {
            let mut bytes = Vec::new();
            let mut chunk = [0_u8; 8192];
            loop {
                match stderr.read(&mut chunk).await {
                    Ok(0) | Err(_) => break,
                    Ok(read) => {
                        if bytes.len() < STDERR_CEILING {
                            bytes.extend_from_slice(
                                &chunk[..read.min(STDERR_CEILING - bytes.len())],
                            );
                        }
                    }
                }
            }
            String::from_utf8_lossy(&bytes).into_owned()
        });

        let transport = DaemonTransport::new(
            OwnedFd::from(from_manager_read),
            OwnedFd::from(to_manager_write),
        )
        .expect("daemon transport");
        let bridge = Arc::new(AppManagerBridge::new(composition()));
        let (ready_tx, ready_rx) = oneshot::channel();
        let driver = {
            let bridge = Arc::clone(&bridge);
            async move { bridge.run_announcing_ready(transport, Some(ready_tx)).await }
        };
        let mut bridge_task = tokio::spawn(driver);

        // The handshake is the only thing that can be learned from a
        // not-yet-finished bridge, and it is the thing readiness hangs on.
        let (bridge_ready, handshake_error) = match tokio::time::timeout(DEADLINE, ready_rx).await {
            Ok(Ok(Ok(()))) => (true, None),
            Ok(Ok(Err(error))) => (false, Some(format!("{error:?}"))),
            Ok(Err(_)) => (false, Some("bridge ended before announcing".to_owned())),
            Err(_) => (false, Some("manager handshake timed out".to_owned())),
        };

        // Wait until the run reaches a terminal state, so teardown below cannot
        // race the fixture's own writes and produce a partial transcript.
        wait_until_settled(&transcript_path).await;

        // EOF is the only shutdown signal the manager has, and in production it
        // arrives because the owning service *drops* the bridge future along
        // with the transport it owns. `AppManagerBridge::cancel` alone does not
        // end the read loop, so awaiting the task after cancelling it would be
        // waiting for a manager this harness is itself responsible for stopping.
        // Give the bridge a moment to finish *on its own* before forcing it.
        // A bridge that ended early has a real reason — a protocol violation,
        // a truncated frame — and dropping that reason would make every later
        // symptom look like a manager timeout.
        let early_end = tokio::time::timeout(TEARDOWN_GRACE, &mut bridge_task).await;
        let mut bridge_error = handshake_error;
        match early_end {
            Ok(Ok(Ok(()))) => {}
            Ok(Ok(Err(error))) => {
                bridge_error = Some(format!("the bridge ended early: {error:?}"));
            }
            Ok(Err(error)) => {
                bridge_error = Some(format!("the bridge task failed: {error}"));
            }
            Err(_) => {
                bridge.cancel();
                bridge_task.abort();
                let _ = bridge_task.await;
            }
        }

        // `terminate_manager`, reproduced: bounded grace, then a direct kill.
        // What matters is that the child is *always* reaped; whether it chose to
        // exit first is not a property the product promises.
        let (manager_status, manager_exited_cleanly) =
            match tokio::time::timeout(MANAGER_EXIT_GRACE, child.wait()).await {
                Ok(Ok(status)) => (status, true),
                Ok(Err(_)) | Err(_) => {
                    let _ = child.start_kill();
                    let status = tokio::time::timeout(DEADLINE, child.wait())
                        .await
                        .expect("a killed manager must still be reaped")
                        .expect("manager reap");
                    (status, false)
                }
            };
        let manager_stderr = tokio::time::timeout(DEADLINE, stderr_task)
            .await
            .expect("stderr drain must end with the child")
            .unwrap_or_default();

        let (sessions, streams) = bridge.counts().await;

        Outcome {
            transcript: read_transcript(&transcript_path),
            manager_stderr,
            manager_status: Some(manager_status),
            manager_exited_cleanly,
            bridge_ready,
            sessions,
            streams,
            bridge_error,
        }
    }

    /// Runs the shipped `i2pr-appd` over the same real manager pipes and
    /// private SAM/I2CP gateway used above, but with its production persistent
    /// policy catalog and a signed, locally installed fixture package.
    async fn run_persistent_catalog(
        root: &Path,
        app_ids: &[AppId],
        already_seen: &BTreeSet<PathBuf>,
    ) -> (Outcome, BTreeSet<PathBuf>) {
        let manager = sibling("i2pr-appd");
        let apphost = sibling(APPHOST);
        let application = sibling(FIXTURE_APP);
        assert_fresh(&manager, &["i2pr-appd/src"]);
        assert_fresh(&apphost, &["i2pr-apphost/src"]);
        assert_fresh(
            &application,
            &[
                "i2pr-app-fixture/src/lib.rs",
                "i2pr-app-fixture/src/main.rs",
            ],
        );

        let (to_manager_read, to_manager_write) = std::io::pipe().expect("daemon->appd pipe");
        let (from_manager_read, from_manager_write) = std::io::pipe().expect("appd->daemon pipe");
        let mut child: Child = Command::new(&manager)
            .env_clear()
            .env("I2PR_APP_STATE_ROOT", root)
            .stdin(Stdio::from(to_manager_read))
            .stdout(Stdio::from(from_manager_write))
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn the shipped manager with the daemon-owned state root");
        let mut stderr = child.stderr.take().expect("piped appd stderr");
        let stderr_task = tokio::spawn(async move {
            let mut bytes = Vec::new();
            let mut chunk = [0_u8; 8192];
            loop {
                match stderr.read(&mut chunk).await {
                    Ok(0) | Err(_) => break,
                    Ok(read) => {
                        if bytes.len() < STDERR_CEILING {
                            bytes.extend_from_slice(
                                &chunk[..read.min(STDERR_CEILING - bytes.len())],
                            );
                        }
                    }
                }
            }
            String::from_utf8_lossy(&bytes).into_owned()
        });

        let transport = DaemonTransport::new(
            OwnedFd::from(from_manager_read),
            OwnedFd::from(to_manager_write),
        )
        .expect("daemon transport");
        let bridge = Arc::new(AppManagerBridge::new(composition()));
        let (ready_tx, ready_rx) = oneshot::channel();
        let driver = {
            let bridge = Arc::clone(&bridge);
            async move { bridge.run_announcing_ready(transport, Some(ready_tx)).await }
        };
        let bridge_task = tokio::spawn(driver);
        let (bridge_ready, handshake_error) = match tokio::time::timeout(DEADLINE, ready_rx).await {
            Ok(Ok(Ok(()))) => (true, None),
            Ok(Ok(Err(error))) => (false, Some(format!("{error:?}"))),
            Ok(Err(_)) => (false, Some("bridge ended before announcing".to_owned())),
            Err(_) => (false, Some("manager handshake timed out".to_owned())),
        };
        let (transcripts, transcript_paths) =
            wait_for_managed_transcripts(app_ids, already_seen).await;
        let (sessions, streams) = bridge.counts().await;

        // Closing the bridge closes its owned transport; appd then observes
        // EOF. The manager remains responsible for its apphost children.
        bridge.cancel();
        bridge_task.abort();
        let _ = bridge_task.await;
        let (manager_status, manager_exited_cleanly) =
            match tokio::time::timeout(MANAGER_EXIT_GRACE, child.wait()).await {
                Ok(Ok(status)) => (status, true),
                Ok(Err(_)) | Err(_) => {
                    let _ = child.start_kill();
                    let status = tokio::time::timeout(DEADLINE, child.wait())
                        .await
                        .expect("a killed manager must still be reaped")
                        .expect("manager reap");
                    (status, false)
                }
            };
        let manager_stderr = tokio::time::timeout(DEADLINE, stderr_task)
            .await
            .expect("appd stderr drain must end with the child")
            .unwrap_or_default();
        (
            Outcome {
                transcript: transcripts.into_iter().flat_map(|(_, rows)| rows).collect(),
                manager_stderr,
                manager_status: Some(manager_status),
                manager_exited_cleanly,
                bridge_ready,
                sessions,
                streams,
                bridge_error: handshake_error,
            },
            transcript_paths,
        )
    }

    async fn wait_for_managed_transcripts(
        app_ids: &[AppId],
        already_seen: &BTreeSet<PathBuf>,
    ) -> (Vec<(PathBuf, Vec<serde_json::Value>)>, BTreeSet<PathBuf>) {
        let deadline = Instant::now() + DEADLINE;
        loop {
            let mut found = Vec::new();
            let directory = std::env::temp_dir();
            for app_id in app_ids {
                let prefix = format!("i2pr-app-fixture-managed-{}-", app_id.as_str());
                for entry in std::fs::read_dir(&directory).expect("temp directory reads") {
                    let entry = entry.expect("temp entry reads");
                    let path = entry.path();
                    let name = entry.file_name();
                    let Some(name) = name.to_str() else { continue };
                    if !name.starts_with(&prefix)
                        || !name.ends_with(".jsonl")
                        || already_seen.contains(&path)
                        || !entry.file_type().expect("temp entry type").is_file()
                    {
                        continue;
                    }
                    found.push((path.clone(), read_transcript(&path)));
                }
            }
            found.sort_by(|a, b| a.0.cmp(&b.0));
            let complete = found.iter().all(|(_, rows)| {
                rows.iter()
                    .any(|row| row.get("step").and_then(|v| v.as_str()) == Some("complete"))
            });
            if found.len() >= app_ids.len() && complete {
                let paths = found.iter().map(|(path, _)| path.clone()).collect();
                return (found, paths);
            }
            assert!(
                Instant::now() < deadline,
                "persistent catalog did not autostart all signed apps; found transcripts={found:?}"
            );
            tokio::time::sleep(POLL_STEP).await;
        }
    }

    /// Waits until the run has genuinely finished.
    ///
    /// Three ways out, in priority order: the fixture recorded a terminal step,
    /// the transcript stopped growing for [`QUIET_PERIOD`], or the hard deadline
    /// expired. The quiet period is what covers a *refused* launch, which
    /// writes its `start` record and is then killed with no terminal step — the
    /// manager's refusal reaches stderr well inside the quiet period, and the
    /// stderr is drained in full afterwards, so nothing is raced.
    async fn wait_until_settled(transcript_path: &Path) {
        let deadline = Instant::now() + DEADLINE;
        let mut last_count = 0_usize;
        let mut quiet_since = Instant::now();
        loop {
            let records = read_transcript(transcript_path);
            if records.len() != last_count {
                last_count = records.len();
                quiet_since = Instant::now();
            }
            let terminal = records.iter().any(|record| {
                matches!(
                    record.get("step").and_then(|s| s.as_str()),
                    Some("complete") | Some("fixture-error") | Some("hang-entered")
                )
            });
            // A run whose newest record is a request in flight is not settled
            // however long the transcript has been quiet.
            let awaiting_reply = records
                .last()
                .and_then(|record| record.get("step"))
                .and_then(|step| step.as_str())
                .is_some_and(|step| PENDING_STEPS.contains(&step));
            if terminal
                || (!awaiting_reply
                    && Instant::now().saturating_duration_since(quiet_since) >= QUIET_PERIOD)
                || Instant::now() >= deadline
            {
                return;
            }
            tokio::time::sleep(POLL_STEP).await;
        }
    }

    fn read_transcript(path: &Path) -> Vec<serde_json::Value> {
        let Ok(text) = std::fs::read_to_string(path) else {
            return Vec::new();
        };
        text.lines()
            .filter(|line| !line.trim().is_empty())
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect()
    }

    /// Asserts a violation was refused rather than tolerated.
    ///
    /// Two-sided on purpose. The fixture writes `verdict-requested` after it
    /// sends the offending byte and then blocks. A product that refuses kills
    /// the session, so nothing is ever written after that marker. A product that
    /// wrongly tolerates the violation answers, and the fixture records both a
    /// verdict and `complete`. Asserting only "the manager said something on
    /// stderr" would have been wrong — most of these are *session* refusals
    /// discovered after the launch already succeeded, which are silent by
    /// design — and asserting only "no `complete`" would pass for any reason the
    /// app might have died.
    fn assert_refused(outcome: &Outcome, scenario: &str) {
        let steps = outcome.steps();
        assert!(
            steps.contains(&"verdict-requested"),
            "{scenario}: the fixture never sent its offending byte; steps were {steps:?}"
        );
        assert!(
            !outcome.recorded("complete"),
            "{scenario}: the violation was tolerated rather than refused; steps were {steps:?}"
        );
        let answered = steps
            .iter()
            .any(|step| step.starts_with("verdict-") && *step != "verdict-requested");
        assert!(
            !answered,
            "{scenario}: the host answered a violation it should have refused; steps were {steps:?}"
        );
    }

    // -- the two required black-box end-to-end paths ------------------------

    /// Plan 369 "End-to-end": fixture → managed SAM HELLO/session path through
    /// the private daemon gateway, with no listener bound.
    #[tokio::test]
    async fn a_real_fixture_reaches_sam_over_the_private_gateway() {
        let outcome = run("sam", Plan::new("sam-happy-path", "sam,i2cp")).await;
        outcome.assert_succeeded("sam happy path");

        assert_eq!(
            outcome.detail("capabilities"),
            &serde_json::json!(["Sam", "I2cp"]),
            "the host must present the launch authority's own capability set"
        );
        let version = outcome.detail("sam-version")["reply"]
            .as_str()
            .expect("a SAM version line")
            .to_owned();
        assert!(
            version.contains("RESULT=OK") && version.contains("VERSION=3.1"),
            "SAM HELLO must be answered with an OK reply naming the negotiated \
             version, got {version:?}"
        );
        let session = outcome.detail("sam-session");
        let result = session["result"].as_str().expect("a session result");
        assert!(
            result.contains("RESULT=OK"),
            "SESSION CREATE must be accepted over the private gateway, got {result:?}"
        );
    }

    /// Plan 369 "End-to-end": fixture → managed I2CP protocol-byte/GetDate path.
    #[tokio::test]
    async fn a_real_fixture_reaches_i2cp_over_the_private_gateway() {
        let outcome = run("i2cp", Plan::new("i2cp-happy-path", "sam,i2cp")).await;
        outcome.assert_succeeded("i2cp happy path");

        assert!(
            outcome.recorded("i2cp-get-date") && outcome.recorded("i2cp-set-date"),
            "the fixture must complete a GetDate/SetDate exchange; steps were {:?}",
            outcome.steps()
        );
        assert!(
            outcome.detail("i2cp-set-date")["bytes"]
                .as_u64()
                .expect("a body length")
                > 0,
            "SetDate must carry a real body, not an empty frame; detail was {:?}",
            outcome.detail("i2cp-set-date")
        );
    }

    // -- capability and identity refusal ------------------------------------

    /// Plan 369 invariant 8: a permission request cannot change effective
    /// capabilities. It must be denied and must leave the granted set alone.
    #[tokio::test]
    async fn a_permission_request_is_denied_and_mutates_nothing() {
        let outcome = run(
            "denied-capability",
            Plan::new("denied-capability", "sam,i2cp"),
        )
        .await;
        outcome.assert_succeeded("denied capability");

        assert_eq!(
            outcome.detail("capabilities"),
            &serde_json::json!(["Sam", "I2cp"]),
            "the granted set must be exactly the launch authority's"
        );
        assert_eq!(
            outcome.detail("permission-denied"),
            &serde_json::json!({ "capability": "lifecycle", "status": "denied" }),
            "an ungranted capability must be denied"
        );
    }

    /// A service open without the matching capability must be refused, and the
    /// refusal must be typed rather than a silent no-op.
    #[tokio::test]
    async fn opening_a_service_the_launch_was_not_granted_is_refused() {
        let outcome = run("denied-service", Plan::new("denied-service", "sam")).await;
        outcome.assert_succeeded("denied service");

        let refusal = outcome.detail("denied-service")["refusal"]
            .as_str()
            .expect("a typed refusal")
            .to_owned();
        assert_eq!(
            refusal, "PermissionDenied",
            "a service the launch was not granted must be refused as PermissionDenied"
        );
        assert!(
            !outcome.recorded("sam-hello"),
            "the refused stream must carry no backend traffic"
        );
        assert_eq!(outcome.sessions, 1, "the session itself stays alive");
    }

    // -- the refusals that must kill the launch -----------------------------

    /// Plan 369 invariant 7: hello identity must exactly match the
    /// manager-created launch identity. A mismatch kills the launch.
    #[tokio::test]
    async fn a_hello_whose_identity_differs_from_the_authority_kills_the_launch() {
        let outcome = run("wrong-identity", Plan::new("hello-wrong-identity", "sam")).await;

        assert!(
            outcome.bridge_ready,
            "the handshake precedes identity checking"
        );
        assert_refused(&outcome, "a mismatched hello");
        assert_eq!(outcome.sessions, 0, "the refused session must be released");
    }

    #[tokio::test]
    async fn a_frame_before_hello_kills_the_launch() {
        let outcome = run("hello-not-first", Plan::new("hello-not-first", "sam")).await;

        // This one is refused *at* the offending frame, so the fixture is killed
        // before it can write the marker that follows. The absence of both the
        // capability set and completion is the signal.
        let steps = outcome.steps();
        assert!(
            !outcome.recorded("capabilities") && !outcome.recorded("complete"),
            "a frame before hello reached the running session; steps were {steps:?}"
        );
        // No session-count assertion here. This session is created *before* the
        // offending frame, so whether the bridge has already processed the
        // manager's close by the time the harness reads it is a scheduling
        // question, not a property. The transcript is the evidence.
    }

    #[tokio::test]
    async fn an_oversized_frame_is_refused_rather_than_truncated() {
        let outcome = run("oversized", Plan::new("oversized-frame", "sam")).await;

        assert_refused(&outcome, "an oversized frame");
        assert_eq!(outcome.sessions, 0, "the refused session must be released");
    }

    #[tokio::test]
    async fn a_malformed_frame_is_refused_rather_than_skipped() {
        let outcome = run("malformed", Plan::new("malformed-frame", "sam")).await;

        assert_refused(&outcome, "a malformed frame");
        assert_eq!(outcome.sessions, 0, "the refused session must be released");
    }

    #[tokio::test]
    async fn an_early_close_ends_only_that_application() {
        let outcome = run("close-early", Plan::new("close-early", "sam")).await;

        assert!(
            outcome.bridge_ready,
            "the handshake precedes the app's exit"
        );
        assert!(
            !outcome.recorded("capabilities"),
            "an early close ends the app before it uses the capability set"
        );
        assert!(
            outcome.recorded("complete"),
            "the app must have finished its own short run; steps were {:?}",
            outcome.steps()
        );
        assert!(
            outcome.manager_status.is_some(),
            "an application ending must still leave a reaped manager"
        );
        assert!(
            !outcome.manager_stderr.contains("manager stopped"),
            "an application ending must not take the manager down; stderr was {:?}",
            outcome.manager_stderr
        );
    }

    /// A fixture that never stops must still be stopped: the manager's bounded
    /// shutdown is what ends it, and nothing is left running.
    #[tokio::test]
    async fn an_application_that_hangs_is_stopped_by_a_bounded_shutdown() {
        let outcome = run("hang", Plan::new("shutdown-hang", "sam")).await;

        assert!(
            outcome.recorded("hang-entered"),
            "the fixture never started; steps were {:?}",
            outcome.steps()
        );
        assert!(
            !outcome.recorded("complete"),
            "a hanging application must not report completion"
        );
        assert!(
            outcome.manager_status.is_some(),
            "a hung application must leave a reaped manager, not an orphan; stderr was {:?}",
            outcome.manager_stderr
        );
    }

    /// stderr flooding must not stop the app working, and must not be mistaken
    /// for protocol: the run still has to complete normally.
    #[tokio::test]
    async fn an_application_that_floods_stderr_still_completes() {
        let outcome = run("stderr-flood", Plan::new("stderr-flood", "sam")).await;

        assert!(
            outcome.recorded("stderr-flood"),
            "the flood must have happened"
        );
        assert_eq!(
            outcome.detail("stderr-flood")["bytes"].as_u64(),
            Some(512 * 1024),
            "the fixture must have written its full bounded flood"
        );
        assert!(
            outcome.bridge_ready,
            "stderr is diagnostics and must never affect the protocol"
        );
        assert!(
            outcome.recorded("complete"),
            "the flood must not disturb the app's own lifecycle; steps were {:?}",
            outcome.steps()
        );
    }

    // -- stream and multi-instance isolation --------------------------------

    /// Data on a stream that was never opened is refused.
    #[tokio::test]
    async fn data_before_open_is_refused() {
        let outcome = run("data-before-open", Plan::new("data-before-open", "sam")).await;

        assert_refused(&outcome, "data before open");
        assert_eq!(outcome.sessions, 0, "the refused session must be released");
    }

    #[tokio::test]
    async fn a_foreign_stream_id_cannot_reach_this_instances_stream() {
        let outcome = run(
            "sibling-stream-id",
            Plan::new("sibling-stream-id-misuse", "sam"),
        )
        .await;
        outcome.assert_succeeded("sibling stream id misuse");

        assert_eq!(
            outcome.detail("sibling-stream-id-misuse"),
            &serde_json::json!({ "stream_id": 9_999, "held_stream": 1 }),
            "the misuse must name a foreign id while a real stream is held"
        );
        assert!(
            outcome.recorded("sam-hello-reply") && outcome.recorded("sam-session-result"),
            "the held stream must keep working across the foreign reset; steps were {:?}",
            outcome.steps()
        );
        let result = outcome.detail("sam-session")["result"]
            .as_str()
            .unwrap_or_default();
        assert!(
            result.contains("RESULT=OK"),
            "the live stream must survive a foreign reset, got {result:?}"
        );
    }

    /// Opening a stream id that is already open is a typed conflict, not a
    /// silent alias onto the first stream's backend.
    #[tokio::test]
    async fn opening_an_already_open_stream_id_is_a_typed_conflict() {
        let outcome = run(
            "duplicate-stream-id",
            Plan::new("duplicate-stream-id", "sam"),
        )
        .await;
        outcome.assert_succeeded("duplicate stream id");

        assert_eq!(
            outcome.detail("duplicate-stream-id")["refusal"],
            "Conflict",
            "a repeated stream id must be refused as a conflict"
        );
    }

    /// Two live applications, each with its own session, own streams, and own
    /// identity. One ending must not touch the other.
    #[tokio::test]
    async fn two_instances_are_isolated_and_one_ending_does_not_touch_the_sibling() {
        let outcome = run(
            "multi-instance",
            Plan::new("sam-happy-path", "sam,i2cp").instances(2),
        )
        .await;
        outcome.assert_succeeded("multi-instance");

        let starts: Vec<&serde_json::Value> = outcome
            .transcript
            .iter()
            .filter(|record| record.get("step").and_then(|s| s.as_str()) == Some("start"))
            .collect();
        assert_eq!(
            starts.len(),
            2,
            "two instances must have been launched; steps were {:?}",
            outcome.steps()
        );

        let instances: Vec<&str> = starts
            .iter()
            .map(|record| record["detail"]["instance"].as_str().expect("an instance"))
            .collect();
        assert_eq!(
            instances,
            vec!["101", "102"],
            "each launch must carry its own distinct manager-created instance id"
        );
    }

    /// Two instances declaring the *same* SAM session id must not collide: the
    /// router-side sessions stay separate, so each app's SAM session is its own.
    #[tokio::test]
    async fn two_instances_sharing_a_sam_session_id_stay_isolated() {
        let outcome = run(
            "duplicate-sam-session",
            Plan::new("duplicate-sam-session-id", "sam,i2cp").instances(2),
        )
        .await;
        outcome.assert_succeeded("duplicate sam session id");

        let sessions: Vec<&serde_json::Value> = outcome
            .transcript
            .iter()
            .filter(|record| record.get("step").and_then(|s| s.as_str()) == Some("sam-session"))
            .collect();
        assert_eq!(
            sessions.len(),
            2,
            "both instances must reach a SAM session; steps were {:?}",
            outcome.steps()
        );
        for record in &sessions {
            assert_eq!(
                record["detail"]["id"], SHARED_SAM_SESSION_ID,
                "both applications must have declared the same SAM session id"
            );
            let result = record["detail"]["result"].as_str().unwrap_or_default();
            assert!(
                result.contains("RESULT=OK"),
                "each instance must get its own accepted SAM session, got {result:?}"
            );
        }
    }

    /// The SAM session id both duplicate-id instances declare.
    const SHARED_SAM_SESSION_ID: &str = "5e6e1a2b-0000-4000-8000-000000000001";

    // -- production persistent catalog qualification ------------------------

    /// Plans 373–374: install signed packages into a fresh local store, persist
    /// explicit publisher trust, per-app grants, exact selection and
    /// autostart, then exercise the shipped manager twice through the real
    /// private SAM and I2CP gateway. A tampered selected sibling is present on
    /// both starts and must not prevent either valid app from running.
    #[tokio::test]
    async fn persisted_autostarts_reach_sam_and_i2cp_again_after_manager_restart() {
        let _ = sibling("i2pr-appd");
        let fixture = sibling(FIXTURE_APP);
        let _ = sibling(APPHOST);
        let scratch = Scratch::new("persistent-catalog");
        let (root, app_ids) = prepare_persistent_catalog(&scratch.path, &fixture);

        let (first, first_paths) = run_persistent_catalog(&root, &app_ids, &BTreeSet::new()).await;
        first.assert_succeeded("first persistent catalog start");
        assert!(
            first.recorded("sam-session"),
            "SAM transcript: {:?}",
            first.steps()
        );
        assert!(
            first.recorded("i2cp-set-date"),
            "I2CP transcript: {:?}",
            first.steps()
        );
        assert!(
            first
                .manager_stderr
                .contains("one autostart application was refused by local policy"),
            "the tampered sibling must be refused while valid siblings continue; stderr={:?}",
            first.manager_stderr
        );
        let first_instances: BTreeSet<String> = first
            .transcript
            .iter()
            .filter(|row| row.get("step").and_then(|v| v.as_str()) == Some("start"))
            .filter_map(|row| {
                row.get("detail")
                    .and_then(|detail| detail.get("instance"))
                    .and_then(|instance| instance.as_str())
                    .map(str::to_owned)
            })
            .collect();
        assert_eq!(first_instances.len(), 2, "two apps autostart once");

        let (second, second_paths) = run_persistent_catalog(&root, &app_ids, &first_paths).await;
        second.assert_succeeded("restarted persistent catalog");
        assert!(
            second.recorded("sam-session"),
            "SAM transcript: {:?}",
            second.steps()
        );
        assert!(
            second.recorded("i2cp-set-date"),
            "I2CP transcript: {:?}",
            second.steps()
        );
        assert!(
            second
                .manager_stderr
                .contains("one autostart application was refused by local policy"),
            "bad sibling refusal must be isolated on restart; stderr={:?}",
            second.manager_stderr
        );
        let second_instances: BTreeSet<String> = second
            .transcript
            .iter()
            .filter(|row| row.get("step").and_then(|v| v.as_str()) == Some("start"))
            .filter_map(|row| {
                row.get("detail")
                    .and_then(|detail| detail.get("instance"))
                    .and_then(|instance| instance.as_str())
                    .map(str::to_owned)
            })
            .collect();
        assert_eq!(
            second_instances.len(),
            2,
            "two apps autostart after restart"
        );
        assert!(
            first_instances.is_disjoint(&second_instances),
            "restart must mint fresh instance ids: first={first_instances:?}, second={second_instances:?}"
        );

        for path in first_paths.union(&second_paths) {
            let _ = std::fs::remove_file(path);
        }
    }

    // -- production manager does not name fixture tooling -------------------

    /// Plan 369 §G / Plan 374: the fixture is evidence tooling. Production
    /// launch authority comes only from the persistent local policy catalog;
    /// the shipped manager never names the fixture executable.
    #[test]
    fn the_shipped_manager_cannot_name_the_fixture() {
        let shipped = sibling("i2pr-appd");
        assert!(
            shipped.is_file(),
            "the shipped manager must be built alongside the fixture"
        );
        let source = std::fs::read_to_string(crates_dir().join("i2pr-appd/src/main.rs"))
            .expect("i2pr-appd main");
        assert!(
            source.contains("PersistentLaunchCatalog::new()")
                && source.contains("serve_with_catalog"),
            "the shipped manager must use the persistent policy catalog"
        );
        assert!(
            !source.contains(FIXTURE_APP),
            "the shipped manager must never name the fixture application"
        );
    }
}
