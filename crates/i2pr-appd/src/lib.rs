//! Plans 369/374 — `i2pr-appd`, the trusted managed-application runtime manager.
//!
//! This crate is a **separate runtime trust zone** (Plan 369 §6). It may depend
//! on the managed-app contracts and on reviewed process/async primitives. It
//! must not depend on `i2pr-daemon`, `i2pr-runtime`, `i2pr-client`, `i2pr-api`,
//! `i2pr-i2pcontrol`, NetDB, tunnel, or transport crates, and it owns no router
//! protocol internals, no NetDB/tunnel/transport state, no Proposal-170
//! administrator credential or sandbox enforcement (the apphost owns it).
//!
//! # What it owns
//!
//! - [`authority`]: manager-created launch authority, and nothing that can
//!   decode one from wire bytes;
//! - [`catalog`]: the trusted source of that authority, backed by validated
//!   offline policy and reverified installed packages in production;
//! - [`manager_link`]: the manager side of the Plan 368 private protocol;
//! - [`apphost_launch`]: process lifecycle for `i2pr-apphost` children;
//! - [`runtime`]: the bounded instance registry and launch pipeline;
//! - [`session`]: managed-app v1 for one application, and the mapping of its
//!   logical stream ids to daemon manager-protocol handles;
//! - bounded diagnostics.
//!
//! # The direction of the manager protocol
//!
//! The two control vocabularies are disjoint by direction, and getting it wrong
//! is a protocol failure rather than a mis-parse. WP2 decoded inbound frames with
//! the *outbound* vocabulary while its test peer also sent outbound-vocabulary
//! frames, so the test asserted the inverted contract and the two mistakes
//! cancelled. [`manager_link`] decodes inbound frames as daemon-to-manager, and
//! `tests/manager_contract.rs` drives its peer in the daemon's direction. This is
//! recorded because it is the single most expensive mistake available here: it
//! only surfaces against the real daemon, after everything else works.
//!
//! # Security posture
//!
//! - No listener, no socket, no discoverable endpoint. The transport is the two
//!   inherited anonymous pipes in [`transport`], and its possession is the
//!   authentication fact (ADR 0035).
//! - No sandbox attestation is fabricated here. A secured apphost must return a
//!   complete validated attestation before the manager accepts readiness.
//! - No decoder from application protocol messages, manifest bytes, or any other
//!   peer-supplied bytes into authority.
//! - Nothing in the manager protocol can make the manager launch an application.
//!   Every request in the vocabulary travels manager → daemon; the inbound
//!   direction carries only replies and notifications. There is deliberately no
//!   manager-receivable launch request.

pub mod apphost_launch;
pub mod authority;
pub mod catalog;
pub mod manager_link;
pub mod runtime;
pub mod session;
pub mod transport;

use std::sync::Arc;

use i2pr_app_manager_proto::apphost::{ApphostBootstrapError, ApphostFailureReason};
use i2pr_app_manager_proto::{
    Handshake, MANAGER_PROTOCOL_MAJOR, MANAGER_PROTOCOL_MINOR, ManagerProtocolError, ManagerRole,
};
use i2pr_app_proto::ContractError;
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};

pub use catalog::{EmptyCatalog, LaunchCatalog, PersistentLaunchCatalog};
pub use transport::{DuplexTransport, inherited};

/// Why an apphost launch produced no transport.
///
/// [`apphost_launch::ApphostLaunchError`] carries OS error strings, which
/// [`AppdError`] deliberately does not: a manager failure has to stay comparable
/// in a test without comparing message text. The strings remain available on the
/// launcher itself, so nothing is lost to a human reading a real failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LaunchFailure {
    /// The sibling `i2pr-apphost` could not be resolved as an absolute regular
    /// file next to this process.
    SiblingUnavailable,
    /// The apphost process could not be started.
    Spawn,
    /// The apphost never produced a typed reply.
    Bootstrap,
    /// The apphost refused the launch, with its own reason.
    Refused(ApphostFailureReason),
    /// The apphost could not be terminated or reaped.
    Teardown,
}

impl std::fmt::Display for LaunchFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SiblingUnavailable => write!(formatter, "sibling i2pr-apphost is unavailable"),
            Self::Spawn => write!(formatter, "the apphost process could not be started"),
            Self::Bootstrap => write!(formatter, "the apphost never produced a reply"),
            Self::Refused(reason) => write!(formatter, "the apphost refused: {reason:?}"),
            Self::Teardown => write!(formatter, "the apphost could not be terminated or reaped"),
        }
    }
}

/// Typed manager failures. No variant carries payload bytes or secrets, so the
/// whole type stays `Clone + Eq` and a failure can be asserted without matching
/// on message text.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum AppdError {
    #[error("manager transport closed")]
    TransportClosed,
    #[error("manager reply did not arrive within the bound")]
    ManagerTimeout,
    #[error("manager protocol violation: {0}")]
    Protocol(#[from] ManagerProtocolError),
    #[error("apphost bootstrap refused: {0}")]
    Bootstrap(#[from] ApphostBootstrapError),
    #[error("application launch refused: {0}")]
    Launch(LaunchFailure),
    #[error("launch authority refused: {0}")]
    Authority(ContractError),
    #[error("application contract violation: {0}")]
    Contract(ContractError),
    /// A launch or session the manager refused on its own terms. These strings
    /// are chosen, not derived from peer input, so they carry no attacker
    /// controlled text.
    #[error("managed application refused: {0}")]
    App(&'static str),
    #[error("application instance ceiling reached")]
    InstanceCeiling,
    #[error("persistent managed-application catalog could not be loaded")]
    Catalog,
    #[error("invalid lifecycle transition: {0} -> {1}")]
    InvalidTransition(&'static str, &'static str),
}

/// The lifecycle states Plan 369 §C defines.
///
/// The manager itself occupies `Starting`, `ConnectedToRouter`, `Idle`, and
/// `Closed`. `Launching`, `AwaitingHello`, `Running`, and `Stopping` belong to a
/// single application session — several may hold them concurrently — so
/// [`session::AppSession`] owns them and walks the *same* [`transition`] machine.
/// One frozen table, used by both levels, is what stops the two from disagreeing
/// about what a legal lifecycle is.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AppdState {
    Starting,
    ConnectedToRouter,
    Idle,
    Launching,
    AwaitingHello,
    Running,
    Stopping,
    Closed,
}

impl AppdState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::ConnectedToRouter => "connected_to_router",
            Self::Idle => "idle",
            Self::Launching => "launching",
            Self::AwaitingHello => "awaiting_hello",
            Self::Running => "running",
            Self::Stopping => "stopping",
            Self::Closed => "closed",
        }
    }
}

/// Plan 369 §C state machine. Invalid transitions fail closed.
pub fn transition(from: AppdState, to: AppdState) -> Result<AppdState, AppdError> {
    let legal = matches!(
        (from, to),
        (AppdState::Starting, AppdState::ConnectedToRouter)
            | (AppdState::ConnectedToRouter, AppdState::Idle)
            | (AppdState::Idle, AppdState::Launching)
            | (AppdState::Launching, AppdState::AwaitingHello)
            | (AppdState::AwaitingHello, AppdState::Running)
            | (AppdState::Running, AppdState::Stopping)
            | (AppdState::Launching, AppdState::Stopping)
            | (AppdState::AwaitingHello, AppdState::Stopping)
            | (AppdState::Running, AppdState::Idle)
            | (AppdState::Stopping, AppdState::Closed)
    );
    if legal {
        Ok(to)
    } else {
        Err(AppdError::InvalidTransition(from.as_str(), to.as_str()))
    }
}

/// The trusted manager process body.
///
/// It owns one transport, one state machine, one launch catalog, and the bounded
/// instance registry. The shipped binary supplies `PersistentLaunchCatalog`;
/// tests and fixture tooling may supply `EmptyCatalog` or an explicit test
/// catalog.
pub struct Appd {
    state: AppdState,
    catalog: Box<dyn LaunchCatalog>,
    launched: u64,
}

impl Default for Appd {
    fn default() -> Self {
        Self::new()
    }
}

impl Appd {
    pub fn new() -> Self {
        Self::with_catalog(EmptyCatalog)
    }

    /// A manager with an explicit authority source.
    ///
    /// Tests and fixture tooling use this to supply an explicit authority
    /// source. The shipped binary wires the trusted persistent catalog through
    /// `serve_with_catalog`; no configuration, flag, or wire message can replace
    /// that catalog.
    pub fn with_catalog(catalog: impl LaunchCatalog + 'static) -> Self {
        Self {
            state: AppdState::Starting,
            catalog: Box::new(catalog),
            launched: 0,
        }
    }

    pub const fn state(&self) -> AppdState {
        self.state
    }

    /// Authorities this manager has spent. An authority is consumed whether or
    /// not the launch succeeded; app exit or failure does not trigger a retry.
    pub const fn launched(&self) -> u64 {
        self.launched
    }

    /// Writes the frozen 9-byte manager handshake.
    ///
    /// The daemon reads exactly these bytes before any frame. There is no
    /// credential here on purpose: possession of the inherited transport is the
    /// authentication fact, and a credential would only add a value that could
    /// leak while proving nothing extra.
    pub async fn write_handshake<W>(&mut self, writer: &mut W) -> Result<(), AppdError>
    where
        W: AsyncWrite + Unpin,
    {
        let handshake = Handshake {
            role: ManagerRole::Manager,
            major: MANAGER_PROTOCOL_MAJOR,
            minor: MANAGER_PROTOCOL_MINOR,
        };
        writer
            .write_all(&handshake.encode())
            .await
            .map_err(|_| AppdError::TransportClosed)?;
        writer
            .flush()
            .await
            .map_err(|_| AppdError::TransportClosed)?;
        self.state = transition(self.state, AppdState::ConnectedToRouter)?;
        self.state = transition(self.state, AppdState::Idle)?;
        Ok(())
    }

    /// Drives one transport to EOF or to a protocol violation.
    ///
    /// The order is deliberate: greet, start the link, launch whatever authority
    /// owner exists, then wait for the transport to end. On return every session
    /// has been torn down, so no application outlives the manager.
    pub async fn run<T>(&mut self, mut transport: T) -> Result<(), AppdError>
    where
        T: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        self.write_handshake(&mut transport).await?;

        let (read, write) = tokio::io::split(transport);
        let (link, driver) = manager_link::ManagerLink::drive(read, write);
        let link = Arc::new(link);
        let runtime = runtime::AppdRuntime::new(Arc::clone(&link));

        let mut catalog_error = None;
        loop {
            let authority = match self.catalog.next_launch() {
                Ok(Some(authority)) => authority,
                Ok(None) => break,
                Err(error) => {
                    catalog_error = Some(error);
                    break;
                }
            };
            self.launched += 1;
            if let Err(error) = runtime.launch(authority).await {
                // A refused launch is a spent launch, not a fatal manager error:
                // one application failing must not take the manager, or the
                // router, down with it. The typed reason goes to stderr, which is
                // diagnostics and never protocol.
                eprintln!("i2pr-appd launch refused: {error}");
            }
        }

        if catalog_error.is_some() {
            link.close().await;
        }

        let outcome = driver
            .await
            .map_err(|_| AppdError::TransportClosed)?
            .and(Ok(()));

        runtime.shutdown().await;
        link.close().await;
        self.state = AppdState::Closed;
        if let Some(error) = catalog_error {
            return Err(error);
        }
        outcome
    }
}

/// Bounded accounting of one in-flight request ledger.
///
/// A manager never emits an uncorrelated reply, so every request id is recorded on
/// arrival and retired exactly once. The bound keeps a hostile peer from growing
/// this set without limit.
///
/// This is the manager's *own* ledger over requests it has sent, distinct from
/// [`manager_link`]'s, which is keyed the same way but lives inside the reader
/// task so a cancelled request can be retired from there.
#[derive(Debug, Default)]
pub struct RequestLedger {
    in_flight: std::collections::BTreeSet<u32>,
}

impl RequestLedger {
    /// Admits one request id, refusing zero and duplicates.
    pub fn admit(&mut self, request_id: i2pr_app_proto::RequestId) -> Result<(), AppdError> {
        let raw = u32::from(request_id);
        if raw == 0 || !self.in_flight.insert(raw) {
            return Err(AppdError::Protocol(ManagerProtocolError::InvalidHandle));
        }
        Ok(())
    }

    /// Retires one request id, refusing an unknown or already-retired id.
    pub fn retire(&mut self, request_id: i2pr_app_proto::RequestId) -> Result<(), AppdError> {
        if !self.in_flight.remove(&u32::from(request_id)) {
            return Err(AppdError::Protocol(ManagerProtocolError::UnmatchedRequest));
        }
        Ok(())
    }

    pub fn len(&self) -> usize {
        self.in_flight.len()
    }

    pub fn is_empty(&self) -> bool {
        self.in_flight.is_empty()
    }
}

/// Convenience for a process body that owns an inherited transport.
pub async fn serve<T>(transport: T) -> Result<(), AppdError>
where
    T: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    Appd::new().run(transport).await
}

/// Serves with an explicit authority source. Production uses persistent local
/// administrator policy; fixtures provide their private test catalog.
pub async fn serve_with_catalog<T>(
    transport: T,
    catalog: impl LaunchCatalog + 'static,
) -> Result<(), AppdError>
where
    T: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    Appd::with_catalog(catalog).run(transport).await
}
