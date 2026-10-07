//! Plan 369 §C — the manager's launch and session registry.
//!
//! # Bounded by construction
//!
//! The global instance ceiling is [`MAX_MANAGER_SESSIONS`], the same number the
//! daemon enforces and the same number the manager protocol defines. Using one
//! constant rather than three independently chosen ones is deliberate: a
//! manager that thought it could hold 33 sessions while the daemon refuses 32
//! would fail late, at the daemon, with a rejection that looks like a bug.
//!
//! Per-session, the manager protocol's stream and in-flight ceilings apply on
//! top of this, and the private SAM/I2CP gateway owner applies its own as a
//! further independent check. None of them is derived from another.
//!
//! # No relaunch
//!
//! An application that ends is gone. Plan 369 makes no autostart or restart claim
//! ("Out of scope: autostart/restart-request semantics"), so this module never
//! re-runs a launch: a catalog entry that produced a session produces exactly one
//! session, and the authority is consumed whether or not the launch succeeded.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use i2pr_app_manager_proto::{
    DaemonToManagerMessage, MAX_MANAGER_SESSIONS, ManagerSessionId, ManagerToDaemonMessage,
};
use i2pr_app_proto::AppInstanceId;
use tokio::sync::{Mutex, mpsc};
use tokio::task::JoinHandle;

use crate::apphost_launch::{LaunchedApphost, launch_apphost, sibling_apphost_path};
use crate::authority::LaunchAuthority;
use crate::manager_link::{MAX_SESSION_EVENT_QUEUE, ManagerLink};
use crate::session::{AppSession, SessionEnd};
use crate::{AppdError, LaunchFailure};

/// Global ceiling on concurrently live application instances.
pub const MAX_APP_INSTANCES: usize = MAX_MANAGER_SESSIONS;

/// One live session and the task driving it.
struct Entry {
    task: JoinHandle<SessionEnd>,
    /// The daemon session handle, kept so shutdown can end this session's route
    /// rather than only dropping its task handle.
    session: ManagerSessionId,
}

/// A resolved apphost path, or the reason there is not one yet.
enum ApphostPath {
    Resolved(PathBuf),
    /// The first launch failed to resolve the sibling. Remembered so the failure
    /// is reported once per process rather than once per attempt, which would
    /// turn a configuration error into a spawn loop.
    Unavailable,
}

/// The manager's live session registry.
pub struct AppdRuntime {
    link: Arc<ManagerLink>,
    entries: Mutex<BTreeMap<AppInstanceId, Entry>>,
    apphost: Mutex<ApphostPath>,
    /// Live instance count, maintained separately so the ceiling check does not
    /// need the registry lock's await point.
    live: std::sync::atomic::AtomicUsize,
}

impl AppdRuntime {
    pub fn new(link: Arc<ManagerLink>) -> Self {
        Self {
            link,
            entries: Mutex::new(BTreeMap::new()),
            apphost: Mutex::new(ApphostPath::Unavailable),
            live: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// Live application instances.
    pub fn live_instances(&self) -> usize {
        self.live.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Launches one authority and binds it to a daemon session.
    ///
    /// The order is load-bearing: the ceiling is claimed before any process is
    /// spawned, the apphost is started before the daemon session exists (so a
    /// refused launch never leaves a gateway session behind), and the session is
    /// released from the registry if anything after that fails.
    pub async fn launch(&self, authority: LaunchAuthority) -> Result<AppInstanceId, AppdError> {
        let instance = authority.instance_id().clone();

        {
            let entries = self.entries.lock().await;
            if entries.contains_key(&instance) {
                return Err(AppdError::App("instance id is already live"));
            }
        }
        // Reserve the ceiling slot before spawning anything, so two concurrent
        // launches cannot both observe room for the last instance.
        if self
            .live
            .fetch_update(
                std::sync::atomic::Ordering::SeqCst,
                std::sync::atomic::Ordering::SeqCst,
                |live| (live < MAX_APP_INSTANCES).then_some(live + 1),
            )
            .is_err()
        {
            return Err(AppdError::InstanceCeiling);
        }

        let outcome = self.launch_inner(authority, instance.clone()).await;
        if outcome.is_err() {
            self.live.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
        }
        outcome
    }

    async fn launch_inner(
        &self,
        authority: LaunchAuthority,
        instance: AppInstanceId,
    ) -> Result<AppInstanceId, AppdError> {
        let request = authority.to_launch_request();
        let executable = self.apphost_executable().await?;

        // The process first. `launch_apphost` terminates and reaps on refusal,
        // so a `Secured` or escaping launch never leaves a child behind.
        let apphost = launch_apphost(&executable, &request)
            .await
            .map_err(|error| AppdError::Launch(error.into()))?;

        // Then the daemon session. A refusal here terminates the apphost, so no
        // application is left running with no router authority behind it.
        let (events_tx, events_rx) = mpsc::channel(MAX_SESSION_EVENT_QUEUE);
        let request_id = self.link.allocate_request_id();
        let reply = self
            .link
            .open_session(
                ManagerToDaemonMessage::CreateSession {
                    request_id,
                    principal: authority.principal().clone(),
                    effective_capabilities: authority.effective_grants(),
                    limits: authority.gateway_limits(),
                },
                events_tx.clone(),
            )
            .await;

        let session = match reply {
            Ok(i2pr_app_manager_proto::DaemonToManagerMessage::SessionOpened {
                session, ..
            }) => session,
            Ok(other) => {
                terminate(apphost).await;
                return Err(manager_refusal(other));
            }
            Err(error) => {
                terminate(apphost).await;
                return Err(error);
            }
        };

        let live_session = match AppSession::start(
            authority,
            session,
            Arc::clone(&self.link),
            apphost,
            events_rx,
        ) {
            Ok(session) => session,
            Err(error) => {
                // The daemon session was allocated, so it must be released even
                // though no application session exists to release it.
                let request_id = self.link.allocate_request_id();
                let _ = self
                    .link
                    .request_within(
                        ManagerToDaemonMessage::CloseSession {
                            request_id,
                            session,
                        },
                        crate::session::SESSION_CLOSE_GRACE,
                    )
                    .await;
                self.link.drop_session(session).await;
                return Err(error);
            }
        };

        let task = tokio::spawn(async move { live_session.run().await });
        self.entries
            .lock()
            .await
            .insert(instance.clone(), Entry { task, session });
        Ok(instance)
    }

    /// Closes every session and waits for its task to finish.
    ///
    /// The session task owns the release of its own daemon route, its streams,
    /// and the apphost child, so this only has to stop them and wait — which is
    /// what makes "no application outlives the manager" checkable.
    pub async fn shutdown(&self) -> Vec<SessionEnd> {
        let drained: BTreeMap<AppInstanceId, Entry> = {
            let mut guard = self.entries.lock().await;
            std::mem::take(&mut *guard)
        };
        let entries: Vec<Entry> = drained.into_values().collect();
        let mut ends = Vec::with_capacity(entries.len());
        for entry in entries {
            // Dropping the route empties the session's manager-event channel,
            // which is how a session learns the daemon side is gone: its loop
            // treats the channel's end as a manager-side end rather than waiting
            // for a notification that will never arrive.
            self.link.drop_session(entry.session).await;
            match entry.task.await {
                Ok(end) => ends.push(end),
                Err(_) => ends.push(SessionEnd::Refused(AppdError::TransportClosed)),
            }
        }
        self.live.store(0, std::sync::atomic::Ordering::SeqCst);
        ends
    }

    /// Resolves the sibling apphost once per process.
    async fn apphost_executable(&self) -> Result<PathBuf, AppdError> {
        let mut guard = self.apphost.lock().await;
        if let ApphostPath::Resolved(path) = &*guard {
            return Ok(path.clone());
        }
        match sibling_apphost_path() {
            Ok(path) => {
                *guard = ApphostPath::Resolved(path.clone());
                Ok(path)
            }
            Err(_) => {
                // The reason is deliberately dropped here. `AppdError` stays
                // comparable so a manager failure can be asserted in a test, and
                // the underlying OS detail is available on the launcher itself.
                *guard = ApphostPath::Unavailable;
                Err(AppdError::Launch(LaunchFailure::SiblingUnavailable))
            }
        }
    }
}

/// Terminates an apphost whose launch must not proceed.
async fn terminate(apphost: LaunchedApphost) {
    let _ = apphost.finish().await;
}

fn manager_refusal(message: DaemonToManagerMessage) -> AppdError {
    use i2pr_app_manager_proto::{DaemonToManagerMessage as D, ManagerErrorCode};
    match message {
        D::Rejected { error, .. } => AppdError::App(match error.code {
            ManagerErrorCode::PermissionDenied => {
                "daemon refused the launch: capability not granted"
            }
            ManagerErrorCode::ResourceLimit => "daemon refused the launch: resource limit reached",
            _ => "daemon refused the launch",
        }),
        _ => AppdError::App("daemon answered a launch with an unrelated message"),
    }
}
