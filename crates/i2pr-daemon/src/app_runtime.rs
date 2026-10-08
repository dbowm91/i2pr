//! Plan 369 §B — the daemon's supervised managed-application manager child.
//!
//! This module is the **only** place the router creates an application-runtime
//! process. It does four things and refuses to do a fifth:
//!
//! 1. resolves `i2pr-appd` as a **distribution-owned sibling** of the daemon's
//!    own executable (§4, invariant 9) — never from configuration, never
//!    through `PATH`, never through a shell;
//! 2. creates two anonymous pipes and hands the child the read end of one and
//!    the write end of the other as its stdin/stdout (§3, §8);
//! 3. drives the Plan-368 `AppManagerBridge` over those pipes and reports
//!    readiness **only after** the manager handshake succeeds;
//! 4. owns the direct child's whole lifetime: bounded drain of its stderr,
//!    bounded graceful exit on shutdown, then a forced kill and a reap.
//!
//! What it deliberately does **not** do is open a listener. There is no port,
//! no loopback bind, and no discoverable endpoint of any kind; possession of
//! the inherited pipe ends is the entire manager authentication fact (ADR 0035).
//!
//! # Why the daemon does not depend on `i2pr-appd`
//!
//! `i2pr-appd` is a separate trust zone (Plan 369 §6). The dependency runs one
//! way, appd → app protocols. Linking the manager's implementation into the
//! router binary would put the manager's state machine in the same address
//! space as the router, which is the opposite of what §6 asks for. The small
//! duplex wrapper below is deliberately duplicated from `i2pr_appd::transport`
//! for that reason: it is 30 lines, and the alternative is a trust-zone
//! violation that no test would catch.

use std::io;
use std::os::fd::OwnedFd;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::process::Stdio;
use std::task::{Context, Poll};
use std::time::Duration;

use i2pr_core::{HealthDetail, ServiceFailure, ServiceFailureCategory};
use i2pr_runtime::{
    CancellationToken, ChildFailurePolicy, ChildScope, ServiceContext, ServiceResult,
};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, ReadBuf};
use tokio::net::unix::pipe::{Receiver, Sender};
use tokio::process::{Child, ChildStderr, Command};
use tokio::sync::oneshot;
use tokio::time::timeout;

use crate::app_manager_bridge::{AppManagerBridge, AppManagerBridgeError, AppManagerComposition};
use crate::config::{I2cpConfig, SamConfig};

/// The distribution-owned manager executable name, resolved next to the
/// daemon. There is no configuration counterpart to this constant by design.
pub(crate) const MANAGER_BINARY_NAME: &str = "i2pr-appd";
pub(crate) const STATE_ROOT_ENV: &str = "I2PR_APP_STATE_ROOT";

/// Platform suffix applied to the sibling name.
///
/// Plan 369 §4 says "with platform suffix rules applied", so `i2pr.exe` is the
/// Windows sibling and the bare name elsewhere. A name that would otherwise
/// need a shell to interpret is not a name we are willing to spawn.
#[cfg(windows)]
const MANAGER_FILE_NAME: &str = "i2pr-appd.exe";
#[cfg(not(windows))]
const MANAGER_FILE_NAME: &str = MANAGER_BINARY_NAME;

/// Bytes of manager stderr retained for diagnostics.
///
/// Stderr is drained continuously and never grows without limit: the reader
/// keeps only the first `MAX_MANAGER_STDERR_SNAPSHOT_BYTES` and counts every
/// byte beyond that. The snapshot exists so an operator can see *why* a manager
/// refused to start; it is not a log sink and is never interpreted as protocol.
pub(crate) const MAX_MANAGER_STDERR_SNAPSHOT_BYTES: usize = 8 * 1024;

/// Read granularity for the stderr drain.
const STDERR_CHUNK_BYTES: usize = 1024;

/// Bounded grace between closing the transport and escalating to a kill.
///
/// On shutdown the daemon drops the pipe ends, which is EOF, which is the
/// manager's only shutdown signal. A manager that ignores it must not be able
/// to hold the daemon open indefinitely, so after this grace the direct child
/// is killed and reaped.
pub(crate) const MANAGER_EXIT_GRACE: Duration = Duration::from_secs(5);

/// Bounded grace used when the manager's protocol channel is *already* closed.
///
/// A manager that failed a handshake, violated the framing, or reached EOF has
/// already stopped speaking the protocol. Waiting the full graceful-exit grace
/// there buys nothing and multiplies the restart budget by an order of
/// magnitude, so the reap deadline is short and the kill follows immediately.
const MANAGER_REAP_GRACE: Duration = Duration::from_millis(500);

/// Why the supervised manager could not be started or kept running.
///
/// Every variant is a fail-closed outcome: the service returns a typed failure
/// and the supervisor's bounded restart policy decides whether to try again.
/// None of them may terminate the router, because invariant 1 requires the
/// router to stay usable when the app runtime is broken.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ManagerLaunchError {
    /// The daemon could not determine its own executable path.
    DaemonPathUnknown,
    /// The daemon executable has no parent directory to resolve a sibling in.
    NoSiblingDirectory,
    /// The sibling manager executable does not exist or is not a regular file.
    ManagerMissing,
    /// The resolved manager path was not absolute.
    ManagerPathNotAbsolute,
    /// The pipes could not be created.
    Pipe(io::ErrorKind),
    /// The child process could not be spawned.
    Spawn(io::ErrorKind),
}

impl std::fmt::Display for ManagerLaunchError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DaemonPathUnknown => {
                formatter.write_str("cannot determine the daemon executable path")
            }
            Self::NoSiblingDirectory => {
                formatter.write_str("the daemon executable has no parent directory")
            }
            Self::ManagerMissing => write!(
                formatter,
                "no `{MANAGER_FILE_NAME}` sibling exists next to the daemon executable"
            ),
            Self::ManagerPathNotAbsolute => {
                formatter.write_str("the resolved manager path is not absolute")
            }
            Self::Pipe(kind) => write!(formatter, "manager transport pipe failed: {kind:?}"),
            Self::Spawn(kind) => write!(formatter, "manager process spawn failed: {kind:?}"),
        }
    }
}

impl std::error::Error for ManagerLaunchError {}

/// The daemon's half of the inherited anonymous transport.
///
/// `read` is the daemon→manager pipe's **read** end (manager→daemon bytes) and
/// `write` is the daemon→manager pipe's **write** end. Presenting both in one
/// object is what lets `tokio::io::split` hand the Plan-368 codec a reader and
/// a writer over one logical duplex stream.
pub(crate) struct InheritedTransport {
    read: Receiver,
    write: Sender,
}

impl InheritedTransport {
    fn new(read: OwnedFd, write: OwnedFd) -> io::Result<Self> {
        Ok(Self {
            read: Receiver::from_owned_fd(read)?,
            write: Sender::from_owned_fd(write)?,
        })
    }
}

impl AsyncRead for InheritedTransport {
    fn poll_read(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.read).poll_read(context, buffer)
    }
}

impl AsyncWrite for InheritedTransport {
    fn poll_write(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.write).poll_write(context, bytes)
    }

    fn poll_flush(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.write).poll_flush(context)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.write).poll_shutdown(context)
    }
}

/// Test-only override for the resolved manager executable.
///
/// Plan 369 §4 says the manager is a distribution-owned sibling and that "tests
/// may inject a fixture path through test-only composition seams; production
/// config may not". This is that seam, and it is the *only* way any path other
/// than the sibling can reach a spawn.
///
/// It is a process-global because the composition root builds the service
/// spec, not the service body: the spec is what captures the path, and the
/// spec is built inside `build_daemon_graph`. Production code never calls
/// [`set_manager_path_override_for_tests`], and `app_runtime.enabled` still has
/// to be turned on by an operator before the value is read at all — so the
/// override can change *which* sibling stands in, never *whether* the manager
/// runs.
static MANAGER_PATH_OVERRIDE: std::sync::OnceLock<std::sync::Mutex<Option<PathBuf>>> =
    std::sync::OnceLock::new();

/// Installs (or clears, with `None`) the fixture manager path for tests.
///
/// Deliberately `#[doc(hidden)]` and named as a test seam: the guard against
/// production use is that no production caller may name it, and
/// `scripts/check-managed-app-process-boundary.py` rule 4 asserts exactly that.
/// (This comment previously named a `check-app-runtime-boundaries.sh` that does
/// not exist; rule 4 is the check it was describing.)
#[doc(hidden)]
pub fn set_manager_path_override_for_tests(path: Option<PathBuf>) {
    let cell = MANAGER_PATH_OVERRIDE.get_or_init(|| std::sync::Mutex::new(None));
    let mut guard = cell
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    *guard = path;
}

/// Reads the test-only override, if one is installed.
pub(crate) fn manager_path_override() -> Option<PathBuf> {
    MANAGER_PATH_OVERRIDE
        .get()
        .and_then(|cell| {
            cell.lock()
                .ok()
                .and_then(|guard| guard.as_ref().map(|path| path.to_path_buf()))
        })
        .filter(|path| path.is_absolute())
}

/// Resolves the manager executable as a sibling of the daemon's own executable.
///
/// This is Plan 369 §4's whole rule. There is no configuration input, no
/// `PATH` search, and no user-provided string anywhere in the resolution: the
/// answer is a function of where the router was installed. That is what makes
/// possession of the inherited pipes meaningful — the router chose the binary
/// and handed it a capability, and nothing in the configuration could have
/// substituted a different program for it.
pub(crate) fn sibling_manager_path() -> Result<PathBuf, ManagerLaunchError> {
    let executable = std::env::current_exe().map_err(|_| ManagerLaunchError::DaemonPathUnknown)?;
    let directory = executable
        .parent()
        .ok_or(ManagerLaunchError::NoSiblingDirectory)?;
    let candidate = directory.join(MANAGER_FILE_NAME);
    if !candidate.is_file() {
        return Err(ManagerLaunchError::ManagerMissing);
    }
    Ok(candidate)
}

/// A started manager child plus the two transport ends the daemon keeps.
struct SpawnedManager {
    child: Child,
    transport: InheritedTransport,
    stderr: Option<ChildStderr>,
}

/// Establish the daemon-owned state root before the manager is spawned.
/// Relative router data paths are resolved against this process's startup cwd.
pub(crate) fn prepare_state_root(data_dir: &Path) -> Result<PathBuf, String> {
    let data_dir = if data_dir.is_absolute() {
        data_dir.to_owned()
    } else {
        std::env::current_dir()
            .map_err(|_| "cannot resolve router data directory against cwd".to_owned())?
            .join(data_dir)
    };
    let managed = data_dir.join("managed-apps");
    std::fs::create_dir_all(&managed)
        .map_err(|_| "cannot create managed-apps state root".to_owned())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&managed, std::fs::Permissions::from_mode(0o700))
            .map_err(|_| "cannot make managed-apps state root private".to_owned())?;
    }
    let canonical = std::fs::canonicalize(&managed)
        .map_err(|_| "cannot canonicalize managed-apps state root".to_owned())?;
    if !canonical.is_absolute() || !canonical.is_dir() {
        return Err("managed-apps state root is not an absolute directory".to_owned());
    }
    Ok(canonical)
}

/// Spawns the manager over two anonymous inherited pipes.
///
/// The child receives exactly three descriptors: fd 0 (daemon→manager), fd 1
/// (manager→daemon), and fd 2 (its own diagnostics, drained and bounded by the
/// caller). `Command` marks every other descriptor close-on-exec, so the child
/// cannot reach the router's listeners, its configuration handles, or any other
/// process state it was not given.
///
/// `env_clear` removes the operator's environment. `i2pr-appd` reads no
/// environment variable and takes no argument, so it needs none, and an
/// inherited environment is one more channel through which a future change
/// could accidentally start being load-bearing.
async fn spawn_manager(
    path: &Path,
    state_root: &Path,
) -> Result<SpawnedManager, ManagerLaunchError> {
    if !path.is_absolute() {
        return Err(ManagerLaunchError::ManagerPathNotAbsolute);
    }
    if !state_root.is_absolute() {
        return Err(ManagerLaunchError::ManagerPathNotAbsolute);
    }
    // daemon→manager, then manager→daemon.
    let (to_manager_read, to_manager_write) =
        io::pipe().map_err(|error| ManagerLaunchError::Pipe(error.kind()))?;
    let (from_manager_read, from_manager_write) =
        io::pipe().map_err(|error| ManagerLaunchError::Pipe(error.kind()))?;

    let child_stdin: OwnedFd = to_manager_read.into();
    let child_stdout: OwnedFd = from_manager_write.into();
    let transport = InheritedTransport::new(
        OwnedFd::from(from_manager_read),
        OwnedFd::from(to_manager_write),
    )
    .map_err(|error| ManagerLaunchError::Pipe(error.kind()))?;

    let mut child = Command::new(path)
        .env_clear()
        .env(STATE_ROOT_ENV, state_root)
        .stdin(Stdio::from(child_stdin))
        .stdout(Stdio::from(child_stdout))
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| ManagerLaunchError::Spawn(error.kind()))?;

    let stderr = child.stderr.take();
    Ok(SpawnedManager {
        child,
        transport,
        stderr,
    })
}

/// Bounded result of draining the manager's stderr.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct StderrDrain {
    /// The retained prefix, capped at the snapshot ceiling.
    pub(crate) snapshot: Vec<u8>,
    /// Every byte the manager wrote, including the discarded tail.
    pub(crate) total_bytes: u64,
    /// Bytes seen beyond the snapshot ceiling.
    pub(crate) discarded_bytes: u64,
    /// The drain stopped because the owning service finished, not because the
    /// child closed its stderr. This is the observable form of Plan 369 §12's
    /// "no grandchild containment" non-guarantee.
    pub(crate) truncated_by_cancel: bool,
}

impl StderrDrain {
    /// Whether the manager wrote more than the snapshot retains.
    pub(crate) const fn truncated(&self) -> bool {
        self.discarded_bytes > 0
    }
}

/// Reads a child's stderr to EOF, keeping only a bounded prefix.
///
/// The loop runs to EOF deliberately: a manager that fills its stderr pipe and
/// then blocks would otherwise deadlock against a drain nobody performs. The
/// retained bytes are capped, so a manager that floods stderr costs the router
/// a fixed amount of memory no matter how much it writes.
///
/// Generic over the reader so the ceiling and the accounting are provable with
/// an in-memory duplex rather than only through a real child process.
pub(crate) async fn drain_stderr<R>(mut stderr: R, cancel: &CancellationToken) -> StderrDrain
where
    R: AsyncRead + Unpin,
{
    let mut snapshot =
        Vec::with_capacity(STDERR_CHUNK_BYTES.min(MAX_MANAGER_STDERR_SNAPSHOT_BYTES));
    let mut chunk = vec![0_u8; STDERR_CHUNK_BYTES];
    let mut total_bytes: u64 = 0;
    let mut discarded_bytes: u64 = 0;
    loop {
        // The read is cancellable, not just byte-bounded. Plan 369 §12 makes
        // grandchild containment an explicit non-guarantee, so a manager may
        // leave behind a process that still holds the inherited stderr write
        // end. Without this, that process would keep the pipe open, this task
        // would never see EOF, and the service could not finish shutting down.
        let read = tokio::select! {
            biased;
            _ = cancel.cancelled() => break,
            read = stderr.read(&mut chunk) => match read {
                Ok(0) | Err(_) => break,
                Ok(read) => read,
            },
        };
        let bytes = &chunk[..read];
        total_bytes = total_bytes.saturating_add(read as u64);
        let room = MAX_MANAGER_STDERR_SNAPSHOT_BYTES.saturating_sub(snapshot.len());
        let kept = room.min(read);
        snapshot.extend_from_slice(&bytes[..kept]);
        discarded_bytes = discarded_bytes.saturating_add((read - kept) as u64);
    }
    StderrDrain {
        snapshot,
        total_bytes,
        discarded_bytes,
        truncated_by_cancel: cancel.is_cancelled(),
    }
}

/// Closes the transport, then bounds how long the direct child may live.
///
/// The drop of `transport` is what actually shuts the manager down: both pipe
/// ends close, the manager reads EOF, and EOF is the only teardown signal the
/// protocol has. The child is then awaited for a bounded grace and killed if it
/// has not exited. Returns whether the child exited on its own.
async fn terminate_manager(mut child: Child, grace: Duration) -> bool {
    match timeout(grace, child.wait()).await {
        Ok(Ok(_status)) => true,
        // Timed out, or the wait itself failed: force the direct child down.
        Ok(Err(_)) | Err(_) => {
            let _ = child.start_kill();
            let _ = timeout(MANAGER_REAP_GRACE, child.wait()).await;
            false
        }
    }
}

/// Everything the supervised service needs, captured at composition time.
#[derive(Clone)]
pub(crate) struct AppRuntimeInputs {
    pub(crate) sam: SamConfig,
    pub(crate) i2cp: I2cpConfig,
    pub(crate) addressbook: crate::addressbook::SharedAddressBook,
    /// Canonical daemon-owned `<router.data_dir>/managed-apps` root.
    pub(crate) state_root: PathBuf,
    /// Test-only override for the resolved manager path (Plan 369 §4).
    ///
    /// Production composition never sets this, so the daemon always resolves
    /// its sibling. It exists because an integration test cannot install a
    /// sibling next to `target/debug/deps/...`, and deleting the restriction
    /// would put a configurable executable path back on the trust boundary.
    pub(crate) manager_path_override: Option<PathBuf>,
}

impl AppRuntimeInputs {
    /// Resolves the executable to spawn, applying the same absolute-and-
    /// present rules to a test override as to a production sibling. A fixture
    /// that does not exist must fail the preflight too, or the preflight would
    /// prove nothing about the case it exists to cover.
    pub(crate) fn manager_path(&self) -> Result<PathBuf, ManagerLaunchError> {
        let path = match &self.manager_path_override {
            Some(path) => path.clone(),
            None => sibling_manager_path()?,
        };
        if !path.is_absolute() {
            return Err(ManagerLaunchError::ManagerPathNotAbsolute);
        }
        if !path.is_file() {
            return Err(ManagerLaunchError::ManagerMissing);
        }
        Ok(path)
    }
}

/// How the manager handshake gate resolved.
enum HandshakeOutcome {
    /// The manager completed the manager-protocol handshake.
    Completed,
    /// The bridge ended before announcing a handshake.
    BridgeEnded(Result<(), AppManagerBridgeError>),
    /// The service was cancelled while waiting.
    Cancelled,
}

/// Runs the supervised managed-application manager service.
///
/// Readiness is published **after** the handshake succeeds and never before:
/// the supervisor, `check-config`, and the health surface all treat readiness
/// as "this service is running", and a spawned-but-silent child is not a
/// running manager.
pub(crate) async fn run_manager_service(
    context: ServiceContext,
    inputs: AppRuntimeInputs,
) -> ServiceResult {
    // Every exit path below cancels this token, so the stderr drain can never
    // outlive the body that owns the child it is reading. Doing it here rather
    // than at each `return` means a future exit path cannot forget.
    let drain_cancel = context.cancellation().child_token();
    let outcome = drive_manager(context, inputs, drain_cancel.clone()).await;
    drain_cancel.cancel(i2pr_core::CancellationReason::ParentScope);
    outcome
}

async fn drive_manager(
    context: ServiceContext,
    inputs: AppRuntimeInputs,
    drain_cancel: CancellationToken,
) -> ServiceResult {
    let cancellation = context.cancellation().clone();
    let readiness = context.readiness();

    let path = match inputs.manager_path() {
        Ok(path) => path,
        Err(error) => {
            return failed(ServiceFailureCategory::InvalidState, error.to_string());
        }
    };

    let spawned = match spawn_manager(&path, &inputs.state_root).await {
        Ok(spawned) => spawned,
        Err(error) => {
            return failed(ServiceFailureCategory::InvalidState, error.to_string());
        }
    };
    let SpawnedManager {
        child,
        transport,
        stderr,
    } = spawned;

    // The stderr drain is an owned child task, not a detached one, so daemon
    // shutdown reaps it along with everything else the service created.
    let child_scope = context.children();
    let stderr_scope_rejected = stderr
        .map(|stderr| {
            child_scope
                .spawn(move |_| async move {
                    let drain = drain_stderr(stderr, &drain_cancel).await;
                    if drain.truncated() || drain.truncated_by_cancel {
                        // Deterministic truncation accounting, not a silent drop:
                        // an operator learns the manager wrote past the ceiling,
                        // or that the drain stopped because the owning service
                        // finished, instead of assuming the daemon saw all of it.
                        tracing::warn!(
                            retained_bytes = drain.snapshot.len(),
                            total_bytes = drain.total_bytes,
                            discarded_bytes = drain.discarded_bytes,
                            stopped_by_owner = drain.truncated_by_cancel,
                            "manager stderr drain ended before EOF"
                        );
                    }
                    Ok(())
                })
                .is_err()
        })
        .unwrap_or(false);
    if stderr_scope_rejected {
        // The scope is closed, so this service is already tearing down.
        // Dropping the transport gives the child EOF and we let the
        // supervisor observe the shutdown.
        drop(transport);
        let _ = terminate_manager(child, MANAGER_REAP_GRACE).await;
        return i2pr_runtime::ServiceResult::RequestedShutdown;
    }

    let composition = AppManagerComposition {
        sam: inputs.sam.clone(),
        i2cp: inputs.i2cp.clone(),
        addressbook: inputs.addressbook.clone(),
        children: ChildScope::child_of(&cancellation, ChildFailurePolicy::DegradeParent),
        cancellation: cancellation.child_token(),
    };
    let bridge = std::sync::Arc::new(AppManagerBridge::new(composition));

    let (ready_tx, mut ready_rx) = oneshot::channel();
    let mut bridge_task = Box::pin(bridge.run_announcing_ready(transport, Some(ready_tx)));

    // The handshake outcome gates readiness. The bridge future must be polled
    // *alongside* the readiness channel, not before it: a pinned-but-unpolled
    // future never runs, so waiting on `ready_rx` alone would wait for a
    // handshake that nothing is performing.
    let handshake = tokio::select! {
        outcome = &mut ready_rx => match outcome {
            Ok(Ok(())) => HandshakeOutcome::Completed,
            Ok(Err(error)) => HandshakeOutcome::BridgeEnded(Err(error)),
            // The sender is dropped only when the bridge future is dropped,
            // which cannot happen while it is being polled here; treat it as a
            // closed transport rather than waiting forever.
            Err(_) => HandshakeOutcome::BridgeEnded(Err(AppManagerBridgeError::TransportClosed)),
        },
        // The bridge ended before it announced a handshake — a protocol
        // violation during the greeting, or an immediate EOF.
        result = &mut bridge_task => HandshakeOutcome::BridgeEnded(result),
        _ = cancellation.cancelled() => HandshakeOutcome::Cancelled,
    };
    match handshake {
        HandshakeOutcome::Completed => {
            if let Err(failure) = signal_readiness(&readiness) {
                drop(bridge_task);
                bridge.cancel();
                let _ = terminate_manager(child, MANAGER_REAP_GRACE).await;
                return failure;
            }
        }
        HandshakeOutcome::BridgeEnded(result) => {
            drop(bridge_task);
            bridge.cancel();
            let _ = terminate_manager(child, MANAGER_REAP_GRACE).await;
            return match result {
                Ok(()) => failed(
                    ServiceFailureCategory::InvalidState,
                    "manager transport closed before the handshake completed",
                ),
                Err(error) => failed(ServiceFailureCategory::InvalidState, error.to_string()),
            };
        }
        HandshakeOutcome::Cancelled => {
            drop(bridge_task);
            bridge.cancel();
            let _ = terminate_manager(child, MANAGER_EXIT_GRACE).await;
            return i2pr_runtime::ServiceResult::RequestedShutdown;
        }
    }

    // Steady state: drive the bridge until it ends or the service is cancelled.
    let outcome = tokio::select! {
        result = &mut bridge_task => Some(result),
        _ = cancellation.cancelled() => None,
    };

    // In both branches the bridge owns the transport; dropping it closes the
    // pipes, which is the child's EOF. `cancel` additionally tears down any
    // gateway session state the bridge created, so a manager that died with
    // sessions still open cannot leave router-side handles behind.
    drop(bridge_task);
    bridge.cancel();
    let exited_cleanly = terminate_manager(child, MANAGER_EXIT_GRACE).await;

    match outcome {
        Some(Ok(())) if exited_cleanly => i2pr_runtime::ServiceResult::RequestedShutdown,
        Some(Ok(())) => failed(
            ServiceFailureCategory::InvalidState,
            "manager transport ended but the process did not exit",
        ),
        Some(Err(error)) => failed(ServiceFailureCategory::InvalidState, error.to_string()),
        None => i2pr_runtime::ServiceResult::RequestedShutdown,
    }
}

fn signal_readiness(readiness: &i2pr_runtime::Readiness) -> Result<(), ServiceResult> {
    readiness.signal_ready().map_err(|error| {
        ServiceResult::Failed(ServiceFailure::new(
            ServiceFailureCategory::Internal,
            HealthDetail::new(format!("app runtime could not report readiness: {error}")).ok(),
        ))
    })
}

fn failed(category: ServiceFailureCategory, reason: impl Into<String>) -> ServiceResult {
    ServiceResult::Failed(ServiceFailure::new(
        category,
        HealthDetail::new(reason.into()).ok(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncWriteExt;

    #[tokio::test]
    async fn stderr_drain_retains_a_bounded_prefix_and_accounts_the_tail() {
        // Drive the real drain with an in-memory flood: a manager that writes
        // far past the ceiling must cost a fixed amount of memory.
        let (mut writer, reader) = tokio::io::duplex(64 * 1024);
        let flood_len = MAX_MANAGER_STDERR_SNAPSHOT_BYTES * 2 + 17;
        let feeder = tokio::spawn(async move {
            let block = vec![b'x'; STDERR_CHUNK_BYTES];
            let mut written = 0_usize;
            while written < flood_len {
                let take = STDERR_CHUNK_BYTES.min(flood_len - written);
                if writer.write_all(&block[..take]).await.is_err() {
                    break;
                }
                written += take;
            }
        });
        let drain = drain_stderr(reader, &CancellationToken::new()).await;
        let _ = feeder.await;

        assert_eq!(
            drain.snapshot.len(),
            MAX_MANAGER_STDERR_SNAPSHOT_BYTES,
            "retained stderr must stop at the ceiling"
        );
        assert!(drain.snapshot.iter().all(|byte| *byte == b'x'));
        assert_eq!(drain.total_bytes, flood_len as u64);
        assert_eq!(
            drain.discarded_bytes,
            (MAX_MANAGER_STDERR_SNAPSHOT_BYTES + 17) as u64
        );
        assert!(drain.truncated());
    }

    #[tokio::test]
    async fn stderr_drain_of_a_quiet_manager_is_not_marked_truncated() {
        let (mut writer, reader) = tokio::io::duplex(64);
        writer
            .write_all(b"i2pr-appd: nothing to report\n")
            .await
            .expect("write");
        drop(writer);
        let drain = drain_stderr(reader, &CancellationToken::new()).await;
        assert!(!drain.truncated());
        assert_eq!(drain.total_bytes, 29);
        assert_eq!(
            String::from_utf8_lossy(&drain.snapshot),
            "i2pr-appd: nothing to report\n"
        );
    }

    #[tokio::test]
    async fn a_relative_manager_path_is_refused_before_any_spawn() {
        assert_eq!(
            spawn_manager(Path::new("i2pr-appd"), Path::new("/tmp/managed-apps"))
                .await
                .err(),
            Some(ManagerLaunchError::ManagerPathNotAbsolute)
        );
    }

    #[tokio::test]
    async fn a_missing_sibling_is_a_distinct_typed_failure() {
        // The daemon's own path always has a parent, so this exercises the
        // resolution rule without depending on a built `i2pr-appd` binary.
        match sibling_manager_path() {
            Ok(path) => {
                assert!(path.is_absolute());
                assert_eq!(path.file_name().unwrap(), MANAGER_FILE_NAME);
                assert_eq!(
                    path.parent(),
                    std::env::current_exe().expect("current exe").parent()
                );
            }
            Err(ManagerLaunchError::ManagerMissing) => {}
            Err(other) => panic!("unexpected resolution failure: {other:?}"),
        }
    }
}
