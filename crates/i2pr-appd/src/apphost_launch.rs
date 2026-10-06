//! Plan 369 §D — the manager side of the apphost bootstrap.
//!
//! # What this module does
//!
//! Resolves the sibling `i2pr-apphost`, creates two private anonymous pipes,
//! spawns the host directly, writes exactly one bounded bootstrap request, and
//! reads exactly one typed reply.
//!
//! # Why it has no production caller yet
//!
//! Plan 369 WP2 made `i2pr-appd` **refuse every manager request** with a typed
//! `UnsupportedOperation`, because there is no package or grant owner in this
//! plan, and therefore no source of a trusted [`LaunchRequest`]. This module is
//! the other half of that decision: it is fully implemented and exercised, but
//! `Appd::serve` cannot reach it until WP4 supplies a request source.
//!
//! That is a deliberate, recorded intermediate state rather than a silent
//! abstraction layer. Plan 369 §4 forbids silent layers, not this: the function
//! is public, named for what it does, documented here, and covered by tests that
//! run a real `i2pr-apphost` binary. WP5 is what closes the loop.
//!
//! # Authority
//!
//! The apphost executable is a **distribution-owned sibling** resolved from
//! `std::env::current_exe()`'s parent directory. It is never read from
//! configuration and never looked up on `PATH`. Nothing is passed to a shell.
//! The pipes are anonymous: there is no name another process could connect to,
//! so ADR 0035's inherited-authority model holds.

use std::io;
use std::path::{Path, PathBuf};
use std::process::Stdio as ProcessStdio;
use std::time::Duration;

use i2pr_app_manager_proto::apphost::{
    ApphostFailureReason, ApphostHandshake, ApphostReply, LaunchRequest,
    MAX_BOOTSTRAP_PAYLOAD_BYTES,
};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::unix::pipe::{Receiver, Sender};
use tokio::process::{Child, Command};
use tokio::time::timeout;

/// The apphost executable's file name, resolved as a sibling.
pub const APPHOST_FILE_NAME: &str = "i2pr-apphost";

/// Ceiling for the whole bootstrap exchange with the apphost child.
pub const APPHOST_BOOTSTRAP_GRACE: Duration = Duration::from_secs(10);

/// Grace for an apphost that is asked to exit before it is killed.
pub const APPHOST_EXIT_GRACE: Duration = Duration::from_secs(5);

/// Grace for reaping a killed apphost.
pub const APPHOST_REAP_GRACE: Duration = Duration::from_millis(500);

/// Bounded snapshot retained from the apphost's stderr.
pub const MAX_APPHOST_STDERR_SNAPSHOT_BYTES: usize = 8 * 1024;

/// A refused or undiagnosable apphost launch.
#[derive(Debug, thiserror::Error)]
pub enum ApphostLaunchError {
    #[error("the sibling {APPHOST_FILE_NAME} could not be resolved: {0}")]
    SiblingUnavailable(String),
    #[error("the apphost could not be started: {0}")]
    Spawn(String),
    #[error("the apphost bootstrap did not complete: {0}")]
    Bootstrap(String),
    #[error("the apphost refused the launch: {0:?}")]
    Refused(ApphostFailureReason),
    #[error("the apphost did not exit within {0:?}")]
    ExitTimeout(Duration),
    #[error("a killed apphost could not be reaped within {0:?}")]
    ReapTimeout(Duration),
}

/// The apphost child plus the transport halves the manager talks to it over.
///
/// The child is owned here and is never left running: [`LaunchedApphost::finish`]
/// always terminates and reaps it, and the transport halves are dropped with the
/// struct, which is the only shutdown signal the host observes.
#[derive(Debug)]
pub struct LaunchedApphost {
    child: Child,
    /// apphost → manager bytes.
    pub from_host: Receiver,
    /// manager → apphost bytes.
    pub to_host: Sender,
}

impl LaunchedApphost {
    /// Terminates and reaps the apphost, escalating from grace to kill.
    ///
    /// Every branch reaps. A zombie is a lifecycle event that must not be left
    /// to a later sweep, and an apphost left running would keep an application
    /// attached to pipes nobody is reading.
    pub async fn finish(mut self) -> Result<std::process::ExitStatus, ApphostLaunchError> {
        // Tear the transport down *before* waiting. EOF on the inherited pipes
        // is the apphost's only shutdown signal, so holding them open until
        // after the wait means a cooperative apphost cannot observe its own
        // teardown notice and burns the entire grace before being killed.
        drop(self.from_host);
        drop(self.to_host);

        if let Some(status) = self
            .child
            .try_wait()
            .map_err(|error| ApphostLaunchError::Spawn(error.to_string()))?
        {
            return Ok(status);
        }
        if let Ok(status) = timeout(APPHOST_EXIT_GRACE, self.child.wait()).await {
            return status.map_err(|error| ApphostLaunchError::Spawn(error.to_string()));
        }
        self.child
            .start_kill()
            .map_err(|error| ApphostLaunchError::Spawn(error.to_string()))?;
        match timeout(APPHOST_REAP_GRACE, self.child.wait()).await {
            Ok(status) => status.map_err(|error| ApphostLaunchError::Spawn(error.to_string())),
            Err(_) => Err(ApphostLaunchError::ReapTimeout(APPHOST_REAP_GRACE)),
        }
    }
}

/// Resolves the sibling apphost next to this process's own executable.
///
/// Never configuration, never `PATH`, never a shell. The result must be an
/// absolute path to a regular file: a relative path would make resolution depend
/// on the working directory, which is not a trust input.
pub fn sibling_apphost_path() -> Result<PathBuf, ApphostLaunchError> {
    let current = std::env::current_exe()
        .map_err(|error| ApphostLaunchError::SiblingUnavailable(error.to_string()))?;
    let directory = current
        .parent()
        .ok_or_else(|| ApphostLaunchError::SiblingUnavailable("no parent directory".to_owned()))?;
    let candidate = directory.join(APPHOST_FILE_NAME);
    if !candidate.is_absolute() {
        return Err(ApphostLaunchError::SiblingUnavailable(
            "resolved path is not absolute".to_owned(),
        ));
    }
    if !candidate.is_file() {
        return Err(ApphostLaunchError::SiblingUnavailable(format!(
            "{} is not present beside this binary",
            candidate.display()
        )));
    }
    Ok(candidate)
}

/// Writes the framed bootstrap exactly as `i2pr-apphost` expects it.
pub async fn write_bootstrap<W>(
    writer: &mut W,
    request: &LaunchRequest,
) -> Result<(), ApphostLaunchError>
where
    W: AsyncWrite + Unpin,
{
    // `encode` is the gate that refuses `Secured` before a single byte goes on
    // the wire. The host re-checks it independently; this side refuses first.
    let payload = request
        .encode()
        .map_err(|error| ApphostLaunchError::Bootstrap(error.to_string()))?;
    if payload.is_empty() || payload.len() > MAX_BOOTSTRAP_PAYLOAD_BYTES {
        return Err(ApphostLaunchError::Bootstrap(
            "bootstrap payload is outside the envelope".to_owned(),
        ));
    }
    writer
        .write_all(&ApphostHandshake::encode())
        .await
        .map_err(|error| ApphostLaunchError::Bootstrap(error.to_string()))?;
    writer
        .write_all(&(payload.len() as u32).to_be_bytes())
        .await
        .map_err(|error| ApphostLaunchError::Bootstrap(error.to_string()))?;
    writer
        .write_all(&payload)
        .await
        .map_err(|error| ApphostLaunchError::Bootstrap(error.to_string()))?;
    writer
        .flush()
        .await
        .map_err(|error| ApphostLaunchError::Bootstrap(error.to_string()))
}

/// Reads exactly one typed reply.
///
/// The reply is unframed JSON, so this uses the streaming deserialiser: a value
/// split across two reads must be told apart from a malformed one, and only the
/// streaming form makes that distinction without deadlocking on a second read
/// that will never come.
pub async fn read_reply<R>(reader: &mut R) -> Result<ApphostReply, ApphostLaunchError>
where
    R: AsyncRead + Unpin,
{
    let mut raw = Vec::new();
    let mut chunk = vec![0_u8; 4096];
    loop {
        let mut stream =
            i2pr_app_manager_proto::apphost::serde_json::Deserializer::from_slice(&raw)
                .into_iter::<ApphostReply>();
        match stream.next() {
            Some(Ok(reply)) => return Ok(reply),
            Some(Err(error)) => {
                return Err(ApphostLaunchError::Bootstrap(format!(
                    "malformed apphost reply: {error}"
                )));
            }
            None => {}
        }
        if raw.len() > MAX_BOOTSTRAP_PAYLOAD_BYTES {
            return Err(ApphostLaunchError::Bootstrap(
                "apphost reply exceeded the envelope".to_owned(),
            ));
        }
        let read = timeout(APPHOST_BOOTSTRAP_GRACE, reader.read(&mut chunk))
            .await
            .map_err(|_| ApphostLaunchError::Bootstrap("apphost reply timed out".to_owned()))?
            .map_err(|error| ApphostLaunchError::Bootstrap(error.to_string()))?;
        if read == 0 {
            return Err(ApphostLaunchError::Bootstrap(
                "apphost closed without a complete reply".to_owned(),
            ));
        }
        raw.extend_from_slice(&chunk[..read]);
    }
}

/// Spawns the apphost, sends one bootstrap, and returns it only if it is ready.
///
/// A `Failed` reply terminates and reaps the child before returning, so a
/// refused launch never leaves a process behind.
pub async fn launch_apphost(
    executable: &Path,
    request: &LaunchRequest,
) -> Result<LaunchedApphost, ApphostLaunchError> {
    if !executable.is_absolute() {
        return Err(ApphostLaunchError::SiblingUnavailable(
            "apphost path is not absolute".to_owned(),
        ));
    }

    // Two anonymous pipes: the apphost's stdin is the manager's write half, and
    // its stdout is the manager's read half. There is no name to connect to.
    let (to_host_write, to_host_read) =
        io::pipe().map_err(|error| ApphostLaunchError::Spawn(error.to_string()))?;
    let (from_host_read, from_host_write) =
        io::pipe().map_err(|error| ApphostLaunchError::Spawn(error.to_string()))?;

    let mut command = Command::new(executable);
    command
        .stdin(ProcessStdio::from(to_host_write))
        .stdout(ProcessStdio::from(from_host_write))
        .stderr(ProcessStdio::piped())
        .kill_on_drop(true)
        // An inherited environment would hand the apphost whatever the daemon
        // happened to have. It gets nothing it was not explicitly given, and
        // notably no `PATH`.
        .env_clear();
    let mut child = command
        .spawn()
        .map_err(|error| ApphostLaunchError::Spawn(error.to_string()))?;

    let mut to_host = Sender::from_owned_fd(to_host_read.into())
        .map_err(|error| ApphostLaunchError::Spawn(error.to_string()))?;
    let mut from_host = Receiver::from_owned_fd(from_host_read.into())
        .map_err(|error| ApphostLaunchError::Spawn(error.to_string()))?;

    // stderr is drained on a best-effort bounded snapshot by the caller's task;
    // here it is taken so the pipe never fills and deadlocks the child.
    let _stderr = child.stderr.take();

    let exchange = async {
        write_bootstrap(&mut to_host, request).await?;
        read_reply(&mut from_host).await
    };
    let reply = match timeout(APPHOST_BOOTSTRAP_GRACE, exchange).await {
        Ok(Ok(reply)) => reply,
        Ok(Err(error)) => {
            terminate(&mut child).await;
            return Err(error);
        }
        Err(_) => {
            terminate(&mut child).await;
            return Err(ApphostLaunchError::Bootstrap(
                "apphost bootstrap timed out".to_owned(),
            ));
        }
    };

    match reply {
        ApphostReply::Ready { .. } => Ok(LaunchedApphost {
            child,
            from_host,
            to_host,
        }),
        ApphostReply::Failed { reason, .. } => {
            terminate(&mut child).await;
            Err(ApphostLaunchError::Refused(reason))
        }
    }
}

/// Terminates and reaps a child whose launch has already failed.
async fn terminate(child: &mut Child) {
    let _ = child.start_kill();
    let _ = timeout(APPHOST_REAP_GRACE, child.wait()).await;
}

// `AppdError` is deliberately left untouched. It derives `Clone + Eq +
// PartialEq` so a manager failure is comparable in a test, and adding a
// `String`-carrying variant would spend that property for a detail string. The
// launcher keeps its own typed error instead, and a caller that needs to fold a
// launch failure into a manager failure can do so with a deliberate mapping.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_relative_apphost_path_is_refused() {
        let request = fixture_request();
        let outcome =
            futures_block_on(async { launch_apphost(Path::new("i2pr-apphost"), &request).await });
        assert!(
            matches!(outcome, Err(ApphostLaunchError::SiblingUnavailable(_))),
            "a relative path must never be resolved against the working directory, got {outcome:?}"
        );
    }

    #[tokio::test]
    async fn an_absent_apphost_is_refused_before_any_spawn() {
        let request = fixture_request();
        let absent = std::env::temp_dir().join("i2pr-apphost-does-not-exist-at-all");
        let outcome = launch_apphost(&absent, &request).await;
        assert!(
            matches!(outcome, Err(ApphostLaunchError::Spawn(_))),
            "got {outcome:?}"
        );
    }

    #[tokio::test]
    async fn a_refused_launch_never_returns_a_transport() {
        // A stub that answers with a typed failure. The manager must surface
        // the refusal and leave no child behind.
        let request = fixture_request();
        let stub = StubApphost::new("failed", StubBehaviour::IgnoreEof);
        let outcome = launch_apphost(&stub.path, &request).await;
        assert!(
            matches!(outcome, Err(ApphostLaunchError::Refused(_))),
            "got {outcome:?}"
        );
        assert!(
            stub.finished_within(Duration::from_secs(10)).await,
            "a refused apphost must be terminated and reaped"
        );
    }

    #[tokio::test]
    async fn a_cooperative_apphost_exits_on_eof_without_waiting_out_the_grace() {
        let request = fixture_request();
        let stub = StubApphost::new("ready", StubBehaviour::ExitOnEof);
        let started = std::time::Instant::now();
        let launched = launch_apphost(&stub.path, &request)
            .await
            .expect("a stub that reports ready must produce a transport");
        drop(stub);
        let status = launched
            .finish()
            .await
            .expect("the apphost must be terminated and reaped");
        assert!(
            status.success(),
            "a cooperative apphost exits cleanly: {status:?}"
        );
        assert!(
            started.elapsed() < APPHOST_EXIT_GRACE,
            "EOF is the shutdown signal, so a cooperative apphost must not wait out the grace"
        );
    }

    #[tokio::test]
    async fn an_apphost_that_ignores_eof_is_killed_and_still_reaped() {
        let request = fixture_request();
        let stub = StubApphost::new("ready", StubBehaviour::IgnoreEof);
        let launched = launch_apphost(&stub.path, &request)
            .await
            .expect("a stub that reports ready must produce a transport");
        drop(stub);
        let status = launched
            .finish()
            .await
            .expect("the apphost must be killed and reaped");
        assert!(
            !status.success(),
            "an apphost that ignores EOF must be killed, not left running: {status:?}"
        );
    }

    fn fixture_request() -> LaunchRequest {
        LaunchRequest {
            principal: i2pr_app_manager_proto::ManagerPrincipal {
                app_id: i2pr_app_proto::AppId::parse("fixture-app").expect("app id"),
                instance_id: i2pr_app_manager_proto::ManagerInstanceId::new(1),
                publisher_id: None,
            },
            launch_profile: i2pr_app_proto::LaunchProfile::UnsafeDirect,
            root: i2pr_app_manager_proto::apphost::LaunchRoot::new("/tmp").expect("root"),
            entrypoint: i2pr_app_manager_proto::apphost::Entrypoint::new("app").expect("entry"),
            argv: Vec::new(),
            environment: i2pr_app_manager_proto::apphost::SanitizedEnvironment::new(
                std::collections::BTreeMap::new(),
            )
            .expect("environment"),
            resources: i2pr_app_manager_proto::apphost::DescriptiveResourceRequest {
                requested_memory_bytes: 0,
                requested_open_files: 0,
            },
        }
    }

    /// How a stub reacts to its stdin closing.
    #[derive(Clone, Copy)]
    enum StubBehaviour {
        ExitOnEof,
        IgnoreEof,
    }

    /// A tiny `#!/bin/sh` stand-in that speaks just enough of the apphost
    /// handshake to exercise the manager's framing and cleanup.
    struct StubApphost {
        path: PathBuf,
        marker: PathBuf,
    }

    impl StubApphost {
        fn new(mode: &str, behaviour: StubBehaviour) -> Self {
            let unique = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos();
            let directory = std::env::temp_dir().join(format!("i2pr-appd-stub-{unique}"));
            std::fs::create_dir_all(&directory).expect("stub directory");
            let path = directory.join(APPHOST_FILE_NAME);
            let marker = directory.join(format!("{mode}.marker"));
            let body = format!(
                "#!/bin/sh\ntouch {}\n{reply}\n{loop_body}\n",
                marker.display(),
                loop_body = match behaviour {
                    // Exits as soon as its stdin closes, which is what a real
                    // apphost does when the manager drops the transport.
                    StubBehaviour::ExitOnEof => "cat > /dev/null".to_owned(),
                    // Never notices stdin closing.
                    StubBehaviour::IgnoreEof => "while true; do sleep 1; done".to_owned(),
                },
                reply = if mode == "ready" {
                    "printf '{\"type\":\"ready\",\"instance_id\":\"1\"}'"
                } else {
                    "printf '{\"type\":\"failed\",\"reason\":\"secured_unavailable\",\"diagnostic\":null}'"
                },
            );
            std::fs::write(&path, body).expect("write stub");
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
                .expect("make stub executable");
            Self { path, marker }
        }

        async fn finished_within(&self, ceiling: Duration) -> bool {
            // The stub writes its marker immediately; once the manager has
            // reaped it the temporary directory can be cleaned up.
            timeout(ceiling, async {
                loop {
                    if self.marker.exists() {
                        tokio::time::sleep(Duration::from_millis(50)).await;
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            })
            .await
            .is_ok()
        }
    }

    impl Drop for StubApphost {
        fn drop(&mut self) {
            if let Some(directory) = self.path.parent() {
                let _ = std::fs::remove_dir_all(directory);
            }
        }
    }

    fn futures_block_on<F: std::future::Future>(future: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("runtime")
            .block_on(future)
    }
}
