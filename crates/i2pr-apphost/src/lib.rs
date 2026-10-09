//! Plan 369 §D — `i2pr-apphost`: bounded direct-exec application supervisor.
//!
//! # What this process is
//!
//! A single, one-shot, bounded supervisor. It accepts exactly **one**
//! [`LaunchRequest`] over the inherited anonymous transport from `i2pr-appd`,
//! execs the named application directly, and then becomes a byte-transparent
//! relay between the manager and that application until one side closes. It
//! launches nothing else, ever, and serves nothing after the application exits.
//!
//! # Launch profiles and fail-closed refusals
//!
//! * **`Secured` requires the qualified Linux backend.** It accepts only static
//!   native ELF on Linux x86_64/aarch64, installs filesystem, resource, and
//!   syscall restrictions before readiness, and execs in place. Any unavailable
//!   layer refuses before application exec. Other hosts remain fail-closed.
//! * **There is no shell.** The application is exec'd directly. Nothing is
//!   passed to `sh -c`, and `PATH` is never consulted.
//! * **There is no second authority.** No discovery endpoint, no signal-based
//!   teardown, no control file. EOF on the inherited transport is the only
//!   shutdown signal.
//!
//! # Containment is checked twice
//!
//! [`LaunchRequest::validate`] rejects `..`, `.`, absolute, and backslash forms
//! *structurally*, on strings, so it is testable without a filesystem. That is
//! necessary but not sufficient: a symlink inside the root can still point
//! outside it. [`resolve_command`] therefore canonicalises both paths and
//! re-checks containment on the resolved result. The string check stops the
//! obvious escape; the canonical check stops the disguised one.
//!
//! # Every wait is bounded
//!
//! The bootstrap read, the stderr drain, the close grace, and the forced-kill
//! grace all have explicit ceilings. The direct child is owned by this process
//! from `spawn` until it is reaped, and it is killed rather than leaked if the
//! manager goes away first — Plan 369 §12 disclaims grandchild containment, so
//! "the pipe closed" must not mean "an application keeps running with nobody to
//! talk to".

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::process::Stdio as ProcessStdio;
use std::time::Duration;

use i2pr_app_manager_proto::apphost::{
    APPHOST_BOOTSTRAP_BYTES, ApphostBootstrapError, ApphostFailureReason, ApphostHandshake,
    ApphostReply, LaunchRequest, MAX_APPHOST_IDENTIFIER_BYTES, MAX_ENV_ENTRIES, MAX_ROOT_BYTES,
    serde_json,
};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::process::{Child, Command};
use tokio::time::timeout;

mod sandbox;
pub mod transport;

pub use transport::{DuplexTransport, inherited};

// The framing constants are re-exported, not redefined. `i2pr-apphost` and
// `i2pr-appd` must agree on them exactly, so they live once in the protocol
// crate that both sides already depend on.
pub use i2pr_app_manager_proto::apphost::{
    BOOTSTRAP_LENGTH_PREFIX_BYTES as LENGTH_PREFIX_BYTES, MAX_BOOTSTRAP_PAYLOAD_BYTES,
};

/// Total ceiling for the whole bootstrap exchange: handshake, length prefix,
/// and payload.
pub const APPHOST_BOOTSTRAP_GRACE: Duration = Duration::from_secs(10);

/// Ceiling on one relay hop. 8 KiB keeps syscall counts low without holding a
/// large buffer for a slow application.
pub const RELAY_CHUNK_BYTES: usize = 8 * 1024;

/// Bounded snapshot retained from the application's stderr.
///
/// stderr is diagnostics only: it is never protocol, never forwarded to the
/// manager, and never interpreted as control. The snapshot exists so a failure
/// can be explained after the fact without retaining an unbounded stream.
pub const MAX_APPHOST_STDERR_SNAPSHOT_BYTES: usize = 8 * 1024;

/// Grace given to an application to exit after its stdin is closed.
pub const APPHOST_CLOSE_GRACE: Duration = Duration::from_secs(5);

/// Grace given to reap an application after it has been killed.
pub const APPHOST_FORCED_KILL_GRACE: Duration = Duration::from_millis(500);

/// Everything that can go wrong in this process.
///
/// Every variant is typed. "It stopped working" is not a diagnosis an operator
/// can act on, and Plan 369 requires refusal reasons to be nameable so a test
/// can assert *which* gate rejected a launch.
#[derive(Debug, thiserror::Error)]
pub enum ApphostError {
    #[error("the inherited transport failed: {0}")]
    Transport(io::Error),
    #[error("the bootstrap exchange did not complete: {0}")]
    Bootstrap(#[from] ApphostBootstrapError),
    #[error("the launch root does not resolve to a directory")]
    RootUnresolvable,
    #[error("the entrypoint does not resolve to a file inside the launch root")]
    EntrypointNotFound,
    #[error("the entrypoint resolves outside the launch root")]
    EntrypointEscapesRoot,
    #[error("the application data root does not resolve to a directory")]
    DataRootUnresolvable,
    #[error("the application data root overlaps the package root")]
    InvalidDataRoot,
    #[error("the secured Linux sandbox is unavailable on this host or executable")]
    SandboxUnavailable,
    #[error("the secured Linux sandbox could not be installed")]
    SandboxSetup,
    #[error("the application could not be started: {0}")]
    Spawn(String),
    #[error("the application did not exit within {0:?}")]
    CloseTimeout(Duration),
    #[error("a killed application could not be reaped within {0:?}")]
    ReapTimeout(Duration),
    #[error("the application instance identifier is not usable")]
    InstanceIdInvalid,
}

/// Maps a protocol refusal onto the typed reply reason.
///
/// Deliberately lossy on purpose: a manager has to distinguish "you asked for
/// something this host refuses" from "the request was malformed", and nothing
/// more. Distinguishing a `Truncated` read from a `BadMagic` read would leak how
/// far a parser got to a caller that has already lost the argument.
pub fn failure_reason(error: ApphostBootstrapError) -> ApphostFailureReason {
    match error {
        ApphostBootstrapError::SecuredUnavailable => ApphostFailureReason::SecuredUnavailable,
        ApphostBootstrapError::SandboxSetupFailed => ApphostFailureReason::SandboxSetupFailed,
        ApphostBootstrapError::InvalidRoot => ApphostFailureReason::InvalidRoot,
        ApphostBootstrapError::EntrypointEscapesRoot => ApphostFailureReason::EntrypointEscapesRoot,
        _ => ApphostFailureReason::MalformedBootstrap,
    }
}

/// Reads the one bootstrap request, or fails closed.
///
/// Ordering matters: the handshake is validated **first**, because only a peer
/// that presented valid framing bytes is a manager worth answering in protocol.
/// A bad magic is not answered at all.
pub async fn read_launch_request<R>(
    reader: &mut R,
    grace: Duration,
) -> Result<LaunchRequest, ApphostBootstrapError>
where
    R: AsyncRead + Unpin,
{
    /// One I/O error, named in protocol terms.
    ///
    /// `UnexpectedEof` means the peer stopped mid-frame, which is a truncated
    /// bootstrap. Every other I/O failure is reported as malformed: this host
    /// refuses to distinguish transport faults from protocol faults to a caller
    /// that has already lost the argument, and it must not invent a launch.
    fn classify_io(error: io::Error) -> ApphostBootstrapError {
        if error.kind() == io::ErrorKind::UnexpectedEof {
            ApphostBootstrapError::Truncated
        } else {
            ApphostBootstrapError::Malformed
        }
    }

    let exchange = async {
        let mut handshake = [0_u8; APPHOST_BOOTSTRAP_BYTES];
        reader
            .read_exact(&mut handshake)
            .await
            .map_err(classify_io)?;
        ApphostHandshake::decode(&handshake)?;

        let mut prefix = [0_u8; LENGTH_PREFIX_BYTES];
        reader.read_exact(&mut prefix).await.map_err(classify_io)?;
        let payload_len = u32::from_be_bytes(prefix) as usize;
        if payload_len == 0 || payload_len > MAX_BOOTSTRAP_PAYLOAD_BYTES {
            return Err(ApphostBootstrapError::LimitExceeded("bootstrap payload"));
        }
        // Allocated only after the length has been proven to be within the
        // envelope, so an attacker-chosen length cannot drive an allocation.
        let mut payload = vec![0_u8; payload_len];
        reader.read_exact(&mut payload).await.map_err(classify_io)?;
        LaunchRequest::decode(&payload)
    };

    match timeout(grace, exchange).await {
        Ok(Ok(request)) => Ok(request),
        Ok(Err(error)) => Err(error),
        // The manager sent a partial bootstrap and then stopped, or sent
        // nothing at all. Refusing is the fail-closed answer: there is nothing
        // to launch, and waiting forever would leave a supervisor alive that
        // can never succeed.
        Err(_) => Err(ApphostBootstrapError::Truncated),
    }
}

/// Writes one typed reply and flushes it before the channel becomes transparent.
pub async fn write_reply<W>(writer: &mut W, reply: &ApphostReply) -> Result<(), io::Error>
where
    W: AsyncWrite + Unpin,
{
    let encoded = serde_json::to_vec(reply).map_err(io::Error::other)?;
    writer.write_all(&encoded).await?;
    writer.flush().await
}

/// A validated, fully resolved executable and its arguments.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedLaunch {
    pub profile: i2pr_app_proto::LaunchProfile,
    /// Absolute, canonicalised executable path. It has been proven to sit
    /// inside `root` after symlink resolution.
    pub program: PathBuf,
    /// Absolute, canonicalised root the executable was proven to live under.
    pub root: PathBuf,
    /// Canonical manager-owned per-application data directory.
    pub data_root: PathBuf,
    pub argv: Vec<String>,
    pub environment: BTreeMap<String, String>,
}

/// Resolves a validated request to a concrete executable, or refuses it.
///
/// The order is: canonicalise the root, canonicalise the entrypoint, then
/// re-check containment on the *resolved* paths. Checking the strings alone
/// would accept a root containing a symlink that points anywhere on the
/// filesystem.
pub fn resolve_command(request: &LaunchRequest) -> Result<ResolvedLaunch, ApphostError> {
    // Re-check the Plan 369 gate independently of `LaunchRequest::decode`.
    // The decode path already enforces it; this is the second check that makes
    // the refusal a property of *this* crate rather than an inherited
    // precondition, so a future caller cannot reach exec without passing it.
    request.validate().map_err(ApphostError::Bootstrap)?;

    let root = Path::new(request.root.as_str());
    let canonical_root = root
        .canonicalize()
        .map_err(|_| ApphostError::RootUnresolvable)?;
    if !canonical_root.is_dir() {
        return Err(ApphostError::RootUnresolvable);
    }
    if request.root.as_str().len() > MAX_ROOT_BYTES
        || request.environment.entries().len() > MAX_ENV_ENTRIES
    {
        return Err(ApphostError::Bootstrap(
            ApphostBootstrapError::LimitExceeded("launch root"),
        ));
    }

    let entrypoint = root.join(request.entrypoint.as_str());
    let canonical_entrypoint = entrypoint
        .canonicalize()
        .map_err(|_| ApphostError::EntrypointNotFound)?;
    if !canonical_entrypoint.is_file() {
        return Err(ApphostError::EntrypointNotFound);
    }
    if !is_contained_in(&canonical_root, &canonical_entrypoint) {
        return Err(ApphostError::EntrypointEscapesRoot);
    }
    let data_root = Path::new(request.data_root.as_str());
    let canonical_data_root = data_root
        .canonicalize()
        .map_err(|_| ApphostError::DataRootUnresolvable)?;
    if !canonical_data_root.is_dir()
        || is_contained_in(&canonical_root, &canonical_data_root)
        || is_contained_in(&canonical_data_root, &canonical_root)
    {
        return Err(ApphostError::InvalidDataRoot);
    }

    Ok(ResolvedLaunch {
        profile: request.launch_profile,
        program: canonical_entrypoint,
        root: canonical_root,
        data_root: canonical_data_root,
        argv: request.argv.clone(),
        environment: request.environment.entries().clone(),
    })
}

/// True when `candidate` is `root` itself or sits beneath it.
///
/// Compares whole `Component` values so a sibling directory sharing a textual
/// prefix (`/apps/evil` against a root of `/apps/ev`) cannot pass.
fn is_contained_in(root: &Path, candidate: &Path) -> bool {
    let mut root_parts = root.components();
    for component in candidate.components() {
        // A canonicalised path cannot contain a parent reference. Refusing one
        // explicitly makes this predicate safe to reason about on
        // uncanonicalised input as well, which is what its unit test does.
        if matches!(component, Component::ParentDir) {
            return false;
        }
        match root_parts.next() {
            Some(expected) if expected == component => {}
            // The candidate outlives the root prefix, so it is *deeper* than the
            // root. That is containment, not escape -- returning `false` here
            // would reject every legitimate nested entrypoint.
            None => return true,
            // A differing component is the textual-prefix sibling case
            // (`/apps/evil` against a root of `/apps/ev`).
            Some(_) => return false,
        }
    }
    true
}

/// Spawns the application directly: no shell, no `PATH` lookup, no inherited
/// environment, and no inherited descriptor that carries the manager channel.
pub fn spawn_application(
    launch: &ResolvedLaunch,
    working_directory: &Path,
) -> Result<Child, ApphostError> {
    if launch.profile == i2pr_app_proto::LaunchProfile::Secured {
        return Err(ApphostError::Bootstrap(
            ApphostBootstrapError::SecuredUnavailable,
        ));
    }
    let mut command = Command::new(&launch.program);
    command
        .current_dir(working_directory)
        // The working directory is the canonicalised root, so a relative path
        // inside the application cannot reach outside it by accident either.
        .stdin(ProcessStdio::piped())
        .stdout(ProcessStdio::piped())
        .stderr(ProcessStdio::piped())
        .kill_on_drop(true);
    // `env_clear` before applying the sanitised entries is what makes the
    // environment a *request*, not an inheritance. It also removes `PATH`, so
    // nothing in this process can accidentally resolve a bare program name.
    command.env_clear();
    for (key, value) in &launch.environment {
        command.env(OsString::from(key), OsString::from(value));
    }
    if let Some(data_root) = launch.data_root.to_str() {
        command.env("I2PR_APP_DATA_DIR", data_root);
    }
    for argument in &launch.argv {
        command.arg(OsString::from(argument));
    }
    command
        .spawn()
        .map_err(|error| ApphostError::Spawn(error.to_string()))
}

/// Bounded, cancellable drain of an application's stderr.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct StderrDrain {
    /// The retained prefix, bounded by [`MAX_APPHOST_STDERR_SNAPSHOT_BYTES`].
    pub snapshot: Vec<u8>,
    /// Every byte the application wrote to stderr.
    pub total_bytes: u64,
    /// Bytes seen but not retained, because of the snapshot bound.
    pub discarded_bytes: u64,
    /// True when the drain stopped early because it was cancelled rather than
    /// because the stream reached EOF.
    pub truncated_by_cancel: bool,
}

/// Drains `reader` into a bounded snapshot until EOF or cancellation.
///
/// Both bounds are necessary. The byte bound stops an unbounded stream from
/// consuming memory. The cancellation bound stops the *lifetime* from being
/// unbounded: Plan 369 §12 disclaims grandchild containment, so an application
/// that leaves a child holding stderr open would otherwise keep this drain --
/// and therefore this process -- alive indefinitely.
pub async fn drain_stderr<R>(
    mut reader: R,
    cancel: &tokio_util::sync::CancellationToken,
) -> StderrDrain
where
    R: AsyncRead + Unpin,
{
    let mut drain = StderrDrain::default();
    let mut buffer = vec![0_u8; RELAY_CHUNK_BYTES];
    loop {
        let read = tokio::select! {
            biased;
            () = cancel.cancelled() => {
                drain.truncated_by_cancel = true;
                return drain;
            }
            read = reader.read(&mut buffer) => read,
        };
        match read {
            Ok(0) | Err(_) => return drain,
            Ok(count) => {
                drain.total_bytes = drain.total_bytes.saturating_add(count as u64);
                let remaining =
                    MAX_APPHOST_STDERR_SNAPSHOT_BYTES.saturating_sub(drain.snapshot.len());
                if remaining == 0 {
                    drain.discarded_bytes = drain.discarded_bytes.saturating_add(count as u64);
                    continue;
                }
                let keep = remaining.min(count);
                drain.snapshot.extend_from_slice(&buffer[..keep]);
                drain.discarded_bytes = drain.discarded_bytes.saturating_add((count - keep) as u64);
            }
        }
    }
}

/// How a relay finished.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RelayOutcome {
    /// The application closed its stdout.
    ApplicationClosed,
    /// The manager closed the transport.
    ManagerClosed,
    /// The application exited on its own.
    ApplicationExited,
}

/// Owns the application from spawn to reap, and relays bytes while it runs.
///
/// The application is *never* left running: whichever side closes first, the
/// child's stdin is closed so it can observe EOF, and if it does not exit within
/// [`APPHOST_CLOSE_GRACE`] it is killed and then reaped. Dropping a live child
/// here would be a leak with an application still attached to pipes nobody is
/// reading.
async fn relay<T>(
    mut child: Child,
    manager_read: &mut tokio::io::ReadHalf<T>,
    manager_write: &mut tokio::io::WriteHalf<T>,
) -> Result<(), ApphostError>
where
    T: AsyncRead + AsyncWrite + Unpin,
{
    let Some(mut app_stdin) = child.stdin.take() else {
        return Err(ApphostError::Spawn(
            "application stdin was not piped".to_owned(),
        ));
    };
    let Some(mut app_stdout) = child.stdout.take() else {
        return Err(ApphostError::Spawn(
            "application stdout was not piped".to_owned(),
        ));
    };
    let stderr = child.stderr.take();
    let stderr_cancel = tokio_util::sync::CancellationToken::new();
    let stderr_task = stderr.map(|stream| {
        tokio::spawn({
            let cancel = stderr_cancel.clone();
            async move { drain_stderr(stream, &cancel).await }
        })
    });

    let to_application = tokio::io::copy(manager_read, &mut app_stdin);
    let to_manager = tokio::io::copy(&mut app_stdout, manager_write);
    let outcome = tokio::select! {
        biased;
        // Application exit is checked first: if it is already gone there is
        // nothing left to relay to, and the other two arms would otherwise spin
        // on pipes that will never produce another byte.
        status = child.wait() => {
            if let Err(error) = status {
                return Err(ApphostError::Transport(error));
            }
            RelayOutcome::ApplicationExited
        }
        copied = to_application => {
            let _ = copied;
            RelayOutcome::ManagerClosed
        }
        copied = to_manager => {
            let _ = copied;
            RelayOutcome::ApplicationClosed
        }
    };

    // Whatever ended the relay, the application must not outlive it.
    drop(app_stdin);
    let exit = terminate_application(&mut child).await?;
    stderr_cancel.cancel();
    let drain = match stderr_task {
        Some(handle) => handle.await.unwrap_or_default(),
        None => StderrDrain::default(),
    };

    if outcome == RelayOutcome::ManagerClosed && exit.code().is_some_and(|code| code != 0) {
        // The application was killed because its manager went away, and stderr
        // is the only place that fact is visible. This is a diagnostic, never a
        // protocol message.
        eprintln!(
            "i2pr-apphost: application exited with {:?} after the manager closed ({} stderr bytes, {} discarded)",
            exit.code(),
            drain.total_bytes,
            drain.discarded_bytes
        );
    }
    Ok(())
}

/// Closes the application's stdin, then escalates from grace to kill.
///
/// Every branch reaps the child. A zombie is a lifecycle event that must not be
/// left to a later sweep.
async fn terminate_application(
    child: &mut Child,
) -> Result<std::process::ExitStatus, ApphostError> {
    if let Some(status) = child.try_wait().map_err(ApphostError::Transport)? {
        return Ok(status);
    }
    if let Ok(status) = timeout(APPHOST_CLOSE_GRACE, child.wait()).await {
        return status.map_err(ApphostError::Transport);
    }
    child
        .start_kill()
        .map_err(|error| ApphostError::Spawn(error.to_string()))?;
    match timeout(APPHOST_FORCED_KILL_GRACE, child.wait()).await {
        Ok(status) => status.map_err(ApphostError::Transport),
        Err(_) => Err(ApphostError::ReapTimeout(APPHOST_FORCED_KILL_GRACE)),
    }
}

/// One apphost lifetime: handshake, bootstrap, exec, relay, reap.
pub async fn serve<T>(mut transport: T) -> Result<(), ApphostError>
where
    T: AsyncRead + AsyncWrite + Unpin,
{
    serve_inner(&mut transport).await
}

async fn serve_inner<T>(transport: &mut T) -> Result<(), ApphostError>
where
    T: AsyncRead + AsyncWrite + Unpin,
{
    let request = match read_launch_request(transport, APPHOST_BOOTSTRAP_GRACE).await {
        Ok(request) => request,
        Err(error) => {
            // A rejected bootstrap is answered in protocol, because at this
            // point we know the peer presented valid framing.
            if !matches!(
                error,
                ApphostBootstrapError::BadMagic
                    | ApphostBootstrapError::UnsupportedVersion
                    | ApphostBootstrapError::RoleMismatch
            ) {
                let reply = ApphostReply::Failed {
                    reason: failure_reason(error),
                    diagnostic: Some(error.to_string()),
                };
                let _ = write_reply(transport, &reply).await;
            }
            return Err(ApphostError::Bootstrap(error));
        }
    };

    serve_request(transport, request, false).await
}

async fn serve_request<T>(
    transport: &mut T,
    request: LaunchRequest,
    permit_secured_exec: bool,
) -> Result<(), ApphostError>
where
    T: AsyncRead + AsyncWrite + Unpin,
{
    let launch = match resolve_command(&request) {
        Ok(launch) => launch,
        Err(error) => {
            let reason = match &error {
                ApphostError::RootUnresolvable => ApphostFailureReason::InvalidRoot,
                ApphostError::EntrypointNotFound => ApphostFailureReason::EntrypointNotFound,
                ApphostError::EntrypointEscapesRoot => ApphostFailureReason::EntrypointEscapesRoot,
                ApphostError::Bootstrap(inner) => failure_reason(*inner),
                _ => ApphostFailureReason::ExecFailed,
            };
            let reply = ApphostReply::Failed {
                reason,
                diagnostic: Some(error.to_string()),
            };
            write_reply(transport, &reply)
                .await
                .map_err(ApphostError::Transport)?;
            return Err(error);
        }
    };

    // The generic in-process transport cannot safely prove an in-place exec:
    // replacing the test harness or embedding process would be catastrophic.
    if request.launch_profile == i2pr_app_proto::LaunchProfile::Secured && !permit_secured_exec {
        let error = ApphostError::Bootstrap(ApphostBootstrapError::SecuredUnavailable);
        write_reply(
            transport,
            &ApphostReply::Failed {
                reason: failure_reason(ApphostBootstrapError::SecuredUnavailable),
                diagnostic: Some(ApphostBootstrapError::SecuredUnavailable.to_string()),
            },
        )
        .await
        .map_err(ApphostError::Transport)?;
        return Err(error);
    }

    // The instance identifier is derived before the exec, so a principal this
    // host cannot name is a refusal rather than a running application whose
    // identity the manager never learns.
    let instance_id = instance_identifier(&request)?;

    if request.launch_profile == i2pr_app_proto::LaunchProfile::Secured {
        let attestation = match sandbox::install(
            &launch,
            request.resources.requested_memory_bytes,
            request.resources.requested_open_files,
        ) {
            Ok(attestation) => attestation,
            Err(error) => {
                let (reason, wire_error) = match error {
                    ApphostError::SandboxUnavailable => (
                        ApphostFailureReason::SecuredUnavailable,
                        ApphostBootstrapError::SecuredUnavailable,
                    ),
                    _ => (
                        ApphostFailureReason::SandboxSetupFailed,
                        ApphostBootstrapError::SandboxSetupFailed,
                    ),
                };
                write_reply(
                    transport,
                    &ApphostReply::Failed {
                        reason,
                        diagnostic: Some(wire_error.to_string()),
                    },
                )
                .await
                .map_err(ApphostError::Transport)?;
                return Err(error);
            }
        };

        write_reply(
            transport,
            &ApphostReply::Ready {
                instance_id,
                attestation: Some(attestation),
            },
        )
        .await
        .map_err(ApphostError::Transport)?;
        return exec_in_place(&launch);
    }

    let working_directory = launch.root.clone();
    let child = spawn_application(&launch, &working_directory)?;

    // Split only once the launch is decided: after the reply is written the
    // channel stops being protocol and becomes a transparent relay.
    let (mut manager_read, mut manager_write) = tokio::io::split(transport);
    write_reply(
        &mut manager_write,
        &ApphostReply::Ready {
            instance_id,
            attestation: None,
        },
    )
    .await
    .map_err(ApphostError::Transport)?;

    relay(child, &mut manager_read, &mut manager_write).await
}

#[cfg(unix)]
fn exec_in_place(launch: &ResolvedLaunch) -> Result<(), ApphostError> {
    use std::os::unix::process::CommandExt;
    let mut command = std::process::Command::new(&launch.program);
    command.current_dir(&launch.root).env_clear();
    for (key, value) in &launch.environment {
        command.env(key, value);
    }
    if let Some(data_root) = launch.data_root.to_str() {
        command.env("I2PR_APP_DATA_DIR", data_root);
    }
    for argument in &launch.argv {
        command.arg(argument);
    }
    let error = command.exec();
    Err(ApphostError::Spawn(error.to_string()))
}

#[cfg(not(unix))]
fn exec_in_place(_launch: &ResolvedLaunch) -> Result<(), ApphostError> {
    Err(ApphostError::Bootstrap(
        ApphostBootstrapError::SecuredUnavailable,
    ))
}

/// Process entry point. Tokio is used only to perform the bounded bootstrap
/// read; its runtime is dropped before the secured branch installs restrictions
/// and replaces the process with the application.
pub fn serve_inherited() -> Result<(), ApphostError> {
    let mut output = std::io::stdout().lock();
    let mut input = tokio::io::stdin();
    let bootstrap_runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| ApphostError::SandboxSetup)?;
    let request = match bootstrap_runtime
        .block_on(read_launch_request(&mut input, APPHOST_BOOTSTRAP_GRACE))
    {
        Ok(request) => request,
        Err(error) => {
            if !matches!(
                error,
                ApphostBootstrapError::BadMagic
                    | ApphostBootstrapError::UnsupportedVersion
                    | ApphostBootstrapError::RoleMismatch
            ) {
                write_reply_blocking(
                    &mut output,
                    &ApphostReply::Failed {
                        reason: failure_reason(error),
                        diagnostic: Some(error.to_string()),
                    },
                )?;
            }
            return Err(ApphostError::Bootstrap(error));
        }
    };
    drop(input);
    drop(bootstrap_runtime);
    if request.launch_profile == i2pr_app_proto::LaunchProfile::Secured {
        let launch = resolve_command(&request)?;
        let attestation = match sandbox::install(
            &launch,
            request.resources.requested_memory_bytes,
            request.resources.requested_open_files,
        ) {
            Ok(attestation) => attestation,
            Err(error) => {
                let (reason, wire_error) = match error {
                    ApphostError::SandboxUnavailable => (
                        ApphostFailureReason::SecuredUnavailable,
                        ApphostBootstrapError::SecuredUnavailable,
                    ),
                    _ => (
                        ApphostFailureReason::SandboxSetupFailed,
                        ApphostBootstrapError::SandboxSetupFailed,
                    ),
                };
                write_reply_blocking(
                    &mut output,
                    &ApphostReply::Failed {
                        reason,
                        diagnostic: Some(wire_error.to_string()),
                    },
                )?;
                return Err(error);
            }
        };
        let instance_id = instance_identifier(&request)?;
        write_reply_blocking(
            &mut output,
            &ApphostReply::Ready {
                instance_id,
                attestation: Some(attestation),
            },
        )?;
        drop(output);
        return exec_in_place(&launch);
    }

    drop(output);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| ApphostError::SandboxSetup)?;
    let mut transport = inherited();
    runtime.block_on(serve_request(&mut transport, request, false))
}

fn write_reply_blocking<W: std::io::Write>(
    writer: &mut W,
    reply: &ApphostReply,
) -> Result<(), ApphostError> {
    let encoded = serde_json::to_vec(reply).map_err(|_| ApphostError::SandboxSetup)?;
    writer
        .write_all(&encoded)
        .map_err(ApphostError::Transport)?;
    writer.flush().map_err(ApphostError::Transport)
}

/// Derives the bounded instance identifier echoed back on `Ready`.
///
/// The identifier is the manager's canonical decimal instance id, checked
/// against the same shared ceiling the application protocol uses so a reply can
/// never be larger than a peer is prepared to parse. The id round-trips through
/// `AppInstanceId`, so the two protocols cannot disagree about what a canonical
/// spelling is.
fn instance_identifier(request: &LaunchRequest) -> Result<String, ApphostError> {
    let principal = request
        .principal
        .to_app_principal()
        .map_err(|_| ApphostError::InstanceIdInvalid)?;
    let instance_id = String::from(principal.instance_id);
    if instance_id.is_empty() || instance_id.len() > MAX_APPHOST_IDENTIFIER_BYTES {
        return Err(ApphostError::InstanceIdInvalid);
    }
    Ok(instance_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(
        root: &str,
        entrypoint: &str,
        profile: i2pr_app_proto::LaunchProfile,
    ) -> Result<LaunchRequest, ApphostBootstrapError> {
        Ok(LaunchRequest {
            principal: i2pr_app_manager_proto::ManagerPrincipal {
                app_id: i2pr_app_proto::AppId::parse("fixture-app").expect("app id"),
                instance_id: i2pr_app_manager_proto::ManagerInstanceId::new(1),
                publisher_id: None,
            },
            launch_profile: profile,
            root: i2pr_app_manager_proto::apphost::LaunchRoot::new(root)?,
            data_root: i2pr_app_manager_proto::apphost::AppDataRoot::new("/tmp/app-data")?,
            entrypoint: i2pr_app_manager_proto::apphost::Entrypoint::new(entrypoint)?,
            argv: Vec::new(),
            environment: i2pr_app_manager_proto::apphost::SanitizedEnvironment::new(
                BTreeMap::new(),
            )?,
            resources: i2pr_app_manager_proto::apphost::DescriptiveResourceRequest {
                requested_memory_bytes: 0,
                requested_open_files: 0,
            },
        })
    }

    #[test]
    fn secured_launch_passes_wire_validation_for_platform_backend() {
        let request = request(
            "/apps/fixture",
            "bin/app",
            i2pr_app_proto::LaunchProfile::Secured,
        )
        .expect("request");
        assert!(request.validate().is_ok());
        assert_eq!(
            failure_reason(ApphostBootstrapError::SecuredUnavailable),
            ApphostFailureReason::SecuredUnavailable
        );
    }

    #[test]
    fn containment_compares_whole_components() {
        let root = Path::new("/apps/ev");
        assert!(is_contained_in(root, Path::new("/apps/ev/bin/app")));
        assert!(is_contained_in(root, Path::new("/apps/ev")));
        // The textual-prefix sibling must not pass.
        assert!(!is_contained_in(root, Path::new("/apps/evil/bin/app")));
        assert!(!is_contained_in(
            root,
            Path::new("/apps/ev/../../etc/passwd")
        ));
        assert!(!is_contained_in(root, Path::new("/etc/passwd")));
    }

    #[test]
    fn a_missing_root_is_refused_rather_than_defaulted() {
        let request = request(
            "/definitely/not/here/at/all",
            "bin/app",
            i2pr_app_proto::LaunchProfile::UnsafeDirect,
        )
        .expect("request");
        assert!(matches!(
            resolve_command(&request),
            Err(ApphostError::RootUnresolvable)
        ));
    }

    #[test]
    fn an_entrypoint_outside_the_root_is_refused_after_symlink_resolution() {
        let directory =
            std::env::temp_dir().join(format!("i2pr-apphost-root-{}", std::process::id()));
        let real = directory.join("real");
        let outside = directory
            .parent()
            .expect("temp parent")
            .join(format!("i2pr-apphost-outside-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        let _ = std::fs::remove_file(&outside);
        std::fs::create_dir_all(&real).expect("root");
        std::fs::write(&outside, b"#!/bin/sh\nexit 0\n").expect("outside file");
        // A symlink inside the root pointing outside it. The string-level
        // check accepts `bin/app`; only canonicalisation catches this.
        std::os::unix::fs::symlink(&outside, real.join("app")).expect("symlink");

        let request = request(
            real.to_str().expect("utf-8 root"),
            "app",
            i2pr_app_proto::LaunchProfile::UnsafeDirect,
        )
        .expect("request");
        let outcome = resolve_command(&request);
        let _ = std::fs::remove_dir_all(&directory);
        let _ = std::fs::remove_file(&outside);

        assert!(
            matches!(outcome, Err(ApphostError::EntrypointEscapesRoot)),
            "a symlinked entrypoint must not escape the root, got {outcome:?}"
        );
    }

    #[tokio::test]
    async fn an_oversize_bootstrap_is_refused_before_allocating() {
        let (mut writer, mut reader) = tokio::io::duplex(1024);
        let mut frame = ApphostHandshake::encode().to_vec();
        frame.extend_from_slice(&(MAX_BOOTSTRAP_PAYLOAD_BYTES as u32 + 1).to_be_bytes());
        tokio::spawn(async move {
            let _ = writer.write_all(&frame).await;
        });
        assert_eq!(
            read_launch_request(&mut reader, Duration::from_secs(5)).await,
            Err(ApphostBootstrapError::LimitExceeded("bootstrap payload"))
        );
    }

    #[tokio::test]
    async fn a_truncated_bootstrap_is_refused_without_hanging() {
        let (mut writer, mut reader) = tokio::io::duplex(1024);
        let handshake = ApphostHandshake::encode();
        tokio::spawn(async move {
            let _ = writer.write_all(&handshake[..4]).await;
            // Then stop without sending the rest.
        });
        assert_eq!(
            read_launch_request(&mut reader, Duration::from_secs(5)).await,
            Err(ApphostBootstrapError::Truncated)
        );
    }

    #[tokio::test]
    async fn a_manager_that_never_sends_a_bootstrap_is_bounded_by_the_grace() {
        let (_writer, mut reader) = tokio::io::duplex(1024);
        let result = read_launch_request(&mut reader, Duration::from_millis(50))
            .await
            .expect_err("a silent manager must not hang the host");
        assert_eq!(result, ApphostBootstrapError::Truncated);
    }

    #[tokio::test]
    async fn stderr_drain_is_bounded_in_bytes_and_cancellable() {
        let (mut writer, reader) = tokio::io::duplex(64 * 1024);
        tokio::spawn(async move {
            // Comfortably more than the snapshot bound, then never EOF: the
            // drain must stop on its *byte* ceiling and still be cancellable
            // on its *lifetime* ceiling.
            for _ in 0..8 {
                if writer.write_all(&[b'x'; 8 * 1024]).await.is_err() {
                    return;
                }
            }
            std::future::pending::<()>().await;
        });
        let cancel = tokio_util::sync::CancellationToken::new();
        let token = cancel.clone();
        let handle = tokio::spawn(async move { drain_stderr(reader, &token).await });
        tokio::time::sleep(Duration::from_millis(50)).await;
        cancel.cancel();
        let drain = handle.await.expect("joined");
        assert!(drain.truncated_by_cancel);
        assert!(drain.snapshot.len() <= MAX_APPHOST_STDERR_SNAPSHOT_BYTES);
        assert!(
            drain.total_bytes > MAX_APPHOST_STDERR_SNAPSHOT_BYTES as u64,
            "the fixture must write past the snapshot ceiling to prove the bound bites"
        );
        assert!(
            drain.discarded_bytes > 0,
            "bytes past the snapshot ceiling must be accounted, not silently dropped"
        );
        assert_eq!(
            drain.snapshot.len() + drain.discarded_bytes as usize,
            drain.total_bytes as usize
        );
    }
}
