//! Plan 369 WP2 — daemon↔`i2pr-appd` supervision evidence.
//!
//! These drive the real composition root and the real supervisor. The manager
//! child is a generated fixture script standing in for the sibling binary,
//! installed through Plan 369 §4's test-only composition seam — never through
//! configuration, which is the property under test.
//!
//! Nothing here opens a listener: the manager transport is two anonymous pipes
//! the daemon creates and hands to the child it spawns.

use std::path::{Path, PathBuf};
use std::time::Duration;

use i2pr_core::{LifecycleState, ShutdownReason};
use i2pr_daemon::app_runtime::set_manager_path_override_for_tests;
use i2pr_daemon::config::{AppRuntimeConfig, Config};

/// Every await is bounded so a defect fails the test instead of hanging.
const DEADLINE: Duration = Duration::from_secs(30);
/// Bounded shutdown grace for the supervisor under test.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(10);
/// How long a bounded poll waits between supervisor-state observations.
const POLL_STEP: Duration = Duration::from_millis(25);

/// The frozen manager greeting as POSIX `printf` octal escapes: `I2PM`, major
/// 1, minor 1, then the reserved zero byte.
const GOOD_HANDSHAKE: &str = "printf 'I2PM\\001\\000\\001\\000\\000'";

/// Writes an executable fixture manager and returns its absolute path.
///
/// A `#!/bin/sh` script is a legitimate absolute-path executable on the test
/// host and, crucially, is *not* a shell invocation by the daemon: the daemon
/// execs this file directly with no argument and an empty environment, exactly
/// as it would exec a real sibling binary. Using a script keeps the fixture out
/// of the shipped binary set.
fn fixture(dir: &Path, name: &str, body: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).expect("write fixture");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = std::fs::metadata(&path).expect("stat").permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&path, permissions).expect("chmod");
    }
    path
}

/// A unique scratch directory for one test's fixtures.
struct Scratch(PathBuf);

impl Scratch {
    fn new(label: &str) -> Self {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("i2pr-app-runtime-{label}-{unique}"));
        std::fs::create_dir_all(&path).expect("scratch dir");
        Self(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Loads a configuration with the app runtime explicitly enabled.
fn enabled_config(data_dir: &Path) -> Config {
    let raw = format!(
        r#"
schema_version = 1
[router]
data_dir = "{data}"
profile = "balanced"

[app_runtime]
enabled = true
"#,
        data = data_dir.display()
    );
    let path = data_dir.join("router.toml");
    std::fs::write(&path, raw).expect("write config");
    Config::load(&path).expect("config must load")
}

fn write_config(data_dir: &Path, body: &str, label: &str) -> PathBuf {
    let path = data_dir.join(format!("{label}.toml"));
    std::fs::write(&path, body).expect("write config");
    path
}

/// Polls the supervisor until `predicate` holds or the deadline expires.
async fn wait_until<F>(handle: &i2pr_runtime::SupervisorHandle, predicate: F) -> bool
where
    F: Fn(&i2pr_runtime::SupervisorSnapshot) -> bool,
{
    let deadline = tokio::time::Instant::now() + DEADLINE;
    loop {
        if predicate(&handle.snapshot()) {
            return true;
        }
        if tokio::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(POLL_STEP).await;
    }
}

/// Reads one service's lifecycle state from a snapshot.
fn lifecycle_of(
    snapshot: &i2pr_runtime::SupervisorSnapshot,
    service: &str,
) -> Option<LifecycleState> {
    snapshot
        .services
        .iter()
        .find(|entry| entry.service.as_str() == service)
        .map(|entry| entry.lifecycle)
}

#[test]
fn the_app_runtime_is_disabled_by_default_and_selects_no_executable() {
    let scratch = Scratch::new("default");
    let path = write_config(
        &scratch.0,
        &format!(
            "schema_version = 1\n[router]\ndata_dir = \"{}\"\nprofile = \"balanced\"\n",
            scratch.0.display()
        ),
        "default",
    );
    let config = Config::load(&path).expect("config must load");

    assert!(
        !config.app_runtime.enabled,
        "Plan 369 §5 requires the app runtime to be off unless an operator opts in"
    );
    assert_eq!(config.app_runtime, AppRuntimeConfig { enabled: false });
}

#[test]
fn the_block_rejects_any_manager_executable_key() {
    // Plan 369 §4 / invariant 9: the manager is a distribution-owned sibling,
    // so a configuration field that selects an executable must be a hard error
    // rather than a silently ignored key.
    let scratch = Scratch::new("forbidden");
    let path = write_config(
        &scratch.0,
        &format!(
            r#"
schema_version = 1
[router]
data_dir = "{data}"
profile = "balanced"

[app_runtime]
enabled = true
manager_path = "/tmp/evil"
"#,
            data = scratch.0.display()
        ),
        "forbidden",
    );
    assert!(
        Config::load(&path).is_err(),
        "a manager_path key must be refused, not ignored"
    );
}

#[tokio::test]
async fn a_manager_that_completes_the_handshake_reaches_ready_and_is_reaped_on_shutdown() {
    let scratch = Scratch::new("good");
    // Greets correctly, then blocks on stdin. EOF from the daemon is the only
    // shutdown signal, so this also proves the direct child is closed and
    // reaped rather than orphaned.
    let path = fixture(
        &scratch.0,
        "good-manager",
        &format!("{GOOD_HANDSHAKE}; cat > /dev/null"),
    );
    set_manager_path_override_for_tests(Some(path));
    let config = enabled_config(&scratch.0);

    let graph = i2pr_daemon::build_daemon_graph(&config).expect("graph builds");
    let supervisor = i2pr_runtime::Supervisor::new(graph, SHUTDOWN_GRACE).expect("supervisor");
    let handle = supervisor.handle();
    let runner = tokio::spawn(async move { supervisor.run().await });

    let became_ready = wait_until(&handle, |snapshot| snapshot.ready).await;
    let snapshot = handle.snapshot();
    set_manager_path_override_for_tests(None);

    assert!(
        became_ready,
        "a manager that completes the handshake must reach ready; snapshot={snapshot:?}"
    );
    assert_eq!(
        lifecycle_of(&snapshot, "app-runtime"),
        Some(LifecycleState::Ready),
        "readiness must follow the handshake, not merely the spawn"
    );

    handle.shutdown(ShutdownReason::Test);
    let outcome = tokio::time::timeout(DEADLINE, runner).await;
    assert!(
        outcome.is_ok(),
        "the supervisor must shut down once the manager child is closed"
    );
    // `timeout` wraps the `JoinHandle`, which wraps the supervisor result:
    // Ok(Ok(Ok(..))) means it ran, joined, and returned Ok.
    assert!(
        matches!(outcome, Ok(Ok(Ok(_)))),
        "the supervisor must report a clean shutdown, got {outcome:?}"
    );
    if let Ok(Ok(Ok(report))) = outcome {
        // Plan 369 §B requires the direct child to be terminated and reaped:
        // nothing may still be owned, and no cleanup invariant may have failed.
        assert_eq!(report.remaining_tasks(), 0, "a task outlived shutdown");
        assert_eq!(
            report.remaining_child_tasks(),
            0,
            "a child task outlived shutdown"
        );
        assert_eq!(
            report.cleanup_failures(),
            0,
            "manager cleanup violated a shutdown invariant"
        );
    }
}

/// A manager that speaks the wrong magic must never reach ready.
///
/// **Known gap (tracked by corrective Plan 371).** The supervisor awaits
/// initial readiness for every registered service and aborts router startup if
/// one never arrives, whatever its classification. A manager that is spawned
/// but whose greeting is rejected therefore fails *router startup*, rather
/// than degrading only the app runtime the way Plan 369 invariant 1 requires.
///
/// This test asserts the **current** behaviour deliberately: it is the
/// executable record of the gap, and it will fail loudly if Plan 371 changes
/// it, at which point this assertion must be replaced by the degradation
/// assertion Plan 369 §5 asks for. Do not "fix" it by weakening the assertion.
#[tokio::test]
async fn a_manager_that_sends_the_wrong_magic_never_becomes_ready() {
    let scratch = Scratch::new("wrongmagic");
    // Stays alive, so only the handshake can reject it. This proves readiness
    // is gated on the handshake rather than on the process merely existing.
    let path = fixture(
        &scratch.0,
        "wrong-magic-manager",
        "printf 'XXXX\\001\\000\\001\\000\\000'; sleep 60",
    );
    set_manager_path_override_for_tests(Some(path));
    let config = enabled_config(&scratch.0);

    let graph = i2pr_daemon::build_daemon_graph(&config).expect("graph builds");
    let supervisor = i2pr_runtime::Supervisor::new(graph, SHUTDOWN_GRACE).expect("supervisor");
    let handle = supervisor.handle();
    let runner = tokio::spawn(async move { supervisor.run().await });

    // The greeting is rejected immediately, so the bounded restart budget is
    // spent well inside the readiness deadline.
    let outcome = tokio::time::timeout(DEADLINE, runner).await;
    let snapshot = handle.snapshot();
    set_manager_path_override_for_tests(None);

    if outcome.is_err() {
        panic!(
            "a rejected handshake must fail fast rather than hang; snapshot={snapshot:?} \
             app_runtime={:?}",
            lifecycle_of(&snapshot, "app-runtime")
        );
    }
    assert!(
        !snapshot.ready,
        "a wrong manager magic must never reach ready; snapshot={snapshot:?}"
    );
    let app_runtime = lifecycle_of(&snapshot, "app-runtime");
    assert_ne!(
        app_runtime,
        Some(LifecycleState::Ready),
        "readiness must follow a valid handshake, not merely a live process"
    );
}

/// A missing sibling manager is refused at composition, not at startup.
///
/// The supervisor's startup gate cannot express "this service is optional", so
/// Plan 369 preflights the sibling in the composition root: an operator who
/// enables the app runtime without the manager binary gets an actionable
/// configuration error immediately, instead of an opaque readiness failure
/// seconds later with no indication that the manager was the cause.
#[test]
fn a_missing_manager_binary_is_refused_at_composition_with_an_actionable_message() {
    let scratch = Scratch::new("missing");
    set_manager_path_override_for_tests(Some(scratch.0.join("not-installed")));
    let config = enabled_config(&scratch.0);

    let outcome = i2pr_daemon::build_daemon_graph(&config);
    set_manager_path_override_for_tests(None);

    let rendered = match outcome {
        Ok(_) => panic!("a missing sibling manager must not build a service graph"),
        Err(error) => format!("{error:?}"),
    };
    assert!(
        rendered.contains("i2pr-appd") && rendered.contains("managed-application runtime"),
        "the diagnostic must name both the feature and the missing binary: {rendered}"
    );
}
