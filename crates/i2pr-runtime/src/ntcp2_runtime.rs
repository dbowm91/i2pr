//! Bounded runtime-owned NTCP2 socket and link lifecycle helpers.
//!
//! This module is deliberately an adapter boundary. It owns Tokio TCP
//! objects, admission counters, deadlines, bounded queues, and supervised
//! reader/writer children; protocol codecs and handshake/data state remain in
//! `i2pr-transport-ntcp2`.

use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use i2pr_core::CancellationReason;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;

use crate::{CancellationToken, ChildScope, ChildScopeError, ChildTaskFailure};

/// Hard maximum for one Plan 035 runtime duration.
pub const MAX_NTCP2_RUNTIME_DURATION: Duration = Duration::from_secs(3_600);
/// Maximum accepted inbound-stream queue retained by one listener.
pub const MAX_NTCP2_ACCEPT_QUEUE: usize = 256;
/// Maximum bytes retained by one link queue item.
pub const MAX_NTCP2_LINK_MESSAGE_BYTES: usize = 65_535;

/// Local link identifier type used by runtime-only correlation.
pub type LinkId = u64;

/// A bounded category for runtime limit validation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeLimitKind {
    /// All pending inbound handshakes.
    PendingInbound,
    /// Pending attempts from one exact address.
    PendingPerIp,
    /// Pending attempts from one subnet prefix.
    PendingPerSubnet,
    /// Authenticated links owned by the runtime.
    ActiveLinks,
    /// Entries in the replay owner.
    ReplayEntries,
    /// Expiring dial backoff records.
    BackoffEntries,
    /// Entries waiting for one link writer.
    LinkQueueItems,
    /// Bytes waiting for one link writer.
    LinkQueueBytes,
}

/// Runtime resource limits for controlled NTCP2 services.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Ntcp2RuntimeLimits {
    /// Maximum pending inbound handshakes.
    pub max_pending_inbound: usize,
    /// Maximum pending handshakes for one exact IP.
    pub max_pending_per_ip: usize,
    /// Maximum pending handshakes for one subnet prefix.
    pub max_pending_per_subnet: usize,
    /// Maximum active links.
    pub max_active_links: usize,
    /// Maximum replay entries.
    pub max_replay_entries: usize,
    /// Maximum expiring dial-backoff entries.
    pub max_backoff_entries: usize,
    /// Maximum queued writer items per link.
    pub max_link_queue_items: usize,
    /// Maximum queued writer bytes per link.
    pub max_link_queue_bytes: usize,
    /// Maximum accepted sockets waiting for the manager.
    pub max_accept_queue: usize,
}

impl Default for Ntcp2RuntimeLimits {
    fn default() -> Self {
        Self {
            max_pending_inbound: 64,
            max_pending_per_ip: 4,
            max_pending_per_subnet: 16,
            max_active_links: 128,
            max_replay_entries: 256,
            max_backoff_entries: 256,
            max_link_queue_items: 32,
            max_link_queue_bytes: 1 << 20,
            max_accept_queue: 32,
        }
    }
}

impl Ntcp2RuntimeLimits {
    /// Validates all nonzero, ordered limits.
    pub fn validate(self) -> Result<Self, Ntcp2RuntimeConfigError> {
        let values = [
            (RuntimeLimitKind::PendingInbound, self.max_pending_inbound),
            (RuntimeLimitKind::PendingPerIp, self.max_pending_per_ip),
            (
                RuntimeLimitKind::PendingPerSubnet,
                self.max_pending_per_subnet,
            ),
            (RuntimeLimitKind::ActiveLinks, self.max_active_links),
            (RuntimeLimitKind::ReplayEntries, self.max_replay_entries),
            (RuntimeLimitKind::BackoffEntries, self.max_backoff_entries),
            (RuntimeLimitKind::LinkQueueItems, self.max_link_queue_items),
            (RuntimeLimitKind::LinkQueueBytes, self.max_link_queue_bytes),
            (RuntimeLimitKind::PendingInbound, self.max_accept_queue),
        ];
        for (kind, value) in values {
            if value == 0 {
                return Err(Ntcp2RuntimeConfigError::ZeroLimit { kind });
            }
        }
        if self.max_accept_queue > MAX_NTCP2_ACCEPT_QUEUE {
            return Err(Ntcp2RuntimeConfigError::LimitTooLarge {
                kind: RuntimeLimitKind::PendingInbound,
                maximum: MAX_NTCP2_ACCEPT_QUEUE,
            });
        }
        if self.max_pending_per_ip > self.max_pending_inbound
            || self.max_pending_per_subnet > self.max_pending_inbound
        {
            return Err(Ntcp2RuntimeConfigError::InconsistentLimits);
        }
        Ok(self)
    }
}

/// Bounded timing policy for connect, handshake, I/O, queue, and drain work.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Ntcp2RuntimeDeadlines {
    /// TCP connect timeout.
    pub connect: Duration,
    /// Total handshake timeout.
    pub handshake: Duration,
    /// Read-idle timeout.
    pub read_idle: Duration,
    /// Write timeout.
    pub write: Duration,
    /// Queue admission timeout.
    pub queue_wait: Duration,
    /// Graceful duplicate/link drain timeout.
    pub drain: Duration,
}

impl Default for Ntcp2RuntimeDeadlines {
    fn default() -> Self {
        Self {
            connect: Duration::from_secs(5),
            handshake: Duration::from_secs(30),
            read_idle: Duration::from_secs(120),
            write: Duration::from_secs(30),
            queue_wait: Duration::from_secs(5),
            drain: Duration::from_secs(5),
        }
    }
}

impl Ntcp2RuntimeDeadlines {
    /// Validates all configured durations.
    pub fn validate(self) -> Result<Self, Ntcp2RuntimeConfigError> {
        let values = [
            ("connect", self.connect),
            ("handshake", self.handshake),
            ("read_idle", self.read_idle),
            ("write", self.write),
            ("queue_wait", self.queue_wait),
            ("drain", self.drain),
        ];
        for (field, value) in values {
            if value.is_zero() {
                return Err(Ntcp2RuntimeConfigError::ZeroDeadline { field });
            }
            if value > MAX_NTCP2_RUNTIME_DURATION {
                return Err(Ntcp2RuntimeConfigError::DeadlineTooLong { field });
            }
        }
        Ok(self)
    }
}

/// IPv4/IPv6 subnet prefixes used only for bounded admission accounting.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IpPrefixPolicy {
    /// IPv4 prefix width, normally `/24`.
    pub ipv4_prefix: u8,
    /// IPv6 prefix width, normally `/64`.
    pub ipv6_prefix: u8,
}

impl Default for IpPrefixPolicy {
    fn default() -> Self {
        Self {
            ipv4_prefix: 24,
            ipv6_prefix: 64,
        }
    }
}

impl IpPrefixPolicy {
    /// Validates family-specific prefix widths.
    pub const fn new(ipv4_prefix: u8, ipv6_prefix: u8) -> Result<Self, Ntcp2RuntimeConfigError> {
        if ipv4_prefix > 32 || ipv6_prefix > 128 {
            Err(Ntcp2RuntimeConfigError::InvalidPrefix)
        } else {
            Ok(Self {
                ipv4_prefix,
                ipv6_prefix,
            })
        }
    }

    fn key(self, ip: IpAddr) -> PrefixKey {
        match ip {
            IpAddr::V4(value) => PrefixKey::V4(mask_v4(value, self.ipv4_prefix)),
            IpAddr::V6(value) => PrefixKey::V6(mask_v6(value, self.ipv6_prefix)),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
enum PrefixKey {
    V4(Ipv4Addr),
    V6(Ipv6Addr),
}

fn mask_v4(value: Ipv4Addr, prefix: u8) -> Ipv4Addr {
    let bits = u32::from(value);
    let mask = if prefix == 0 {
        0
    } else {
        u32::MAX << (32 - prefix)
    };
    Ipv4Addr::from(bits & mask)
}

fn mask_v6(value: Ipv6Addr, prefix: u8) -> Ipv6Addr {
    let bits = u128::from(value);
    let mask = if prefix == 0 {
        0
    } else {
        u128::MAX << (128 - prefix)
    };
    Ipv6Addr::from(bits & mask)
}

/// Configuration validation failure for the runtime NTCP2 adapter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Ntcp2RuntimeConfigError {
    /// A limit was zero.
    ZeroLimit { kind: RuntimeLimitKind },
    /// A limit exceeded an infrastructure ceiling.
    LimitTooLarge {
        /// Limit category.
        kind: RuntimeLimitKind,
        /// Maximum permitted value.
        maximum: usize,
    },
    /// Per-scope values cannot be satisfied by their global ceiling.
    InconsistentLimits,
    /// A deadline was zero.
    ZeroDeadline { field: &'static str },
    /// A deadline exceeded the runtime horizon.
    DeadlineTooLong { field: &'static str },
    /// A prefix was outside its address-family width.
    InvalidPrefix,
}

impl fmt::Display for Ntcp2RuntimeConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroLimit { kind } => write!(formatter, "zero NTCP2 runtime limit: {kind:?}"),
            Self::LimitTooLarge { kind, maximum } => {
                write!(formatter, "NTCP2 runtime limit {kind:?} exceeds {maximum}")
            }
            Self::InconsistentLimits => formatter.write_str("inconsistent NTCP2 runtime limits"),
            Self::ZeroDeadline { field } => write!(formatter, "zero NTCP2 deadline: {field}"),
            Self::DeadlineTooLong { field } => {
                write!(formatter, "NTCP2 deadline exceeds its bound: {field}")
            }
            Self::InvalidPrefix => formatter.write_str("invalid IP prefix width"),
        }
    }
}

impl std::error::Error for Ntcp2RuntimeConfigError {}

/// Complete validated runtime policy.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Ntcp2RuntimeConfig {
    /// Resource limits.
    pub limits: Ntcp2RuntimeLimits,
    /// Timing limits.
    pub deadlines: Ntcp2RuntimeDeadlines,
    /// Subnet accounting policy.
    pub prefixes: IpPrefixPolicy,
}

impl Ntcp2RuntimeConfig {
    /// Validates and returns this configuration.
    pub fn validate(self) -> Result<Self, Ntcp2RuntimeConfigError> {
        self.limits.validate()?;
        self.deadlines.validate()?;
        let _ = IpPrefixPolicy::new(self.prefixes.ipv4_prefix, self.prefixes.ipv6_prefix)?;
        Ok(self)
    }
}

/// Absolute monotonic runtime deadline.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Ntcp2Deadline(tokio::time::Instant);

impl Ntcp2Deadline {
    /// Creates a deadline after a bounded nonzero duration.
    pub fn after(duration: Duration) -> Result<Self, Ntcp2DeadlineError> {
        if duration.is_zero() {
            return Err(Ntcp2DeadlineError::Zero);
        }
        if duration > MAX_NTCP2_RUNTIME_DURATION {
            return Err(Ntcp2DeadlineError::TooLong);
        }
        Ok(Self(tokio::time::Instant::now() + duration))
    }

    /// Returns the remaining duration, or zero after expiry.
    pub fn remaining(self) -> Duration {
        self.0
            .saturating_duration_since(tokio::time::Instant::now())
    }
}

/// Deadline construction failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Ntcp2DeadlineError {
    /// Zero durations are not useful for an I/O operation.
    Zero,
    /// Duration exceeds the runtime bound.
    TooLong,
}

impl fmt::Display for Ntcp2DeadlineError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Zero => "NTCP2 deadline must be nonzero",
            Self::TooLong => "NTCP2 deadline exceeds its bound",
        })
    }
}

impl std::error::Error for Ntcp2DeadlineError {}

/// Privacy-safe I/O categories.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IoErrorKind {
    /// The peer closed the stream.
    Closed,
    /// The operation exceeded its deadline.
    Deadline,
    /// The owner cancelled the operation.
    Cancelled,
    /// The operating system rejected the operation.
    Failed,
}

/// Exact read/write failure without retained OS error text.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExactIoError {
    /// Fixed category of failure.
    pub kind: IoErrorKind,
}

impl fmt::Display for ExactIoError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "bounded NTCP2 I/O failed: {:?}", self.kind)
    }
}

impl std::error::Error for ExactIoError {}

/// Reads exactly the requested bytes under cancellation and a deadline.
pub async fn read_exact<R>(
    reader: &mut R,
    buffer: &mut [u8],
    deadline: Ntcp2Deadline,
    cancellation: &CancellationToken,
) -> Result<(), ExactIoError>
where
    R: AsyncRead + Unpin,
{
    if buffer.is_empty() {
        return Ok(());
    }
    tokio::select! {
        _ = cancellation.cancelled() => Err(ExactIoError { kind: IoErrorKind::Cancelled }),
        result = tokio::time::timeout(deadline.remaining(), reader.read_exact(buffer)) => {
            match result {
                Ok(Ok(_)) => Ok(()),
                Ok(Err(error)) if error.kind() == std::io::ErrorKind::UnexpectedEof => {
                    Err(ExactIoError { kind: IoErrorKind::Closed })
                }
                Ok(Err(_)) => Err(ExactIoError { kind: IoErrorKind::Failed }),
                Err(_) => Err(ExactIoError { kind: IoErrorKind::Deadline }),
            }
        }
    }
}

/// Writes all requested bytes under cancellation and a deadline.
pub async fn write_all_exact<W>(
    writer: &mut W,
    buffer: &[u8],
    deadline: Ntcp2Deadline,
    cancellation: &CancellationToken,
) -> Result<(), ExactIoError>
where
    W: AsyncWrite + Unpin,
{
    if buffer.is_empty() {
        return Ok(());
    }
    tokio::select! {
        _ = cancellation.cancelled() => Err(ExactIoError { kind: IoErrorKind::Cancelled }),
        result = tokio::time::timeout(deadline.remaining(), writer.write_all(buffer)) => {
            match result {
                Ok(Ok(())) => Ok(()),
                Ok(Err(error)) if error.kind() == std::io::ErrorKind::UnexpectedEof => {
                    Err(ExactIoError { kind: IoErrorKind::Closed })
                }
                Ok(Err(_)) => Err(ExactIoError { kind: IoErrorKind::Failed }),
                Err(_) => Err(ExactIoError { kind: IoErrorKind::Deadline }),
            }
        }
    }
}

/// Address-family category for privacy-safe observations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AddressFamily {
    /// IPv4.
    Ipv4,
    /// IPv6.
    Ipv6,
}

impl AddressFamily {
    fn of(address: IpAddr) -> Self {
        match address {
            IpAddr::V4(_) => Self::Ipv4,
            IpAddr::V6(_) => Self::Ipv6,
        }
    }
}

#[derive(Default)]
struct AdmissionState {
    total: usize,
    ips: HashMap<IpAddr, usize>,
    subnets: HashMap<PrefixKey, usize>,
}

/// A bounded pre-cryptography inbound admission owner.
#[derive(Clone)]
pub struct InboundAdmission {
    limits: Ntcp2RuntimeLimits,
    prefixes: IpPrefixPolicy,
    state: Arc<Mutex<AdmissionState>>,
}

impl fmt::Debug for InboundAdmission {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("InboundAdmission(..)")
    }
}

impl InboundAdmission {
    /// Creates a bounded admission owner.
    pub fn new(config: Ntcp2RuntimeConfig) -> Result<Self, Ntcp2RuntimeConfigError> {
        config.validate()?;
        Ok(Self {
            limits: config.limits,
            prefixes: config.prefixes,
            state: Arc::new(Mutex::new(AdmissionState::default())),
        })
    }

    /// Grants one exact inbound attempt or returns a typed denial.
    pub fn admit(&self, address: SocketAddr) -> Result<InboundPermit, AdmissionDenied> {
        let ip = address.ip();
        let subnet = self.prefixes.key(ip);
        let mut state = self
            .state
            .lock()
            .map_err(|_| AdmissionDenied::new(AdmissionRejection::StatePoisoned))?;
        if state.total >= self.limits.max_pending_inbound {
            return Err(AdmissionDenied::new(AdmissionRejection::GlobalLimit));
        }
        if state.ips.get(&ip).copied().unwrap_or(0) >= self.limits.max_pending_per_ip {
            return Err(AdmissionDenied::new(AdmissionRejection::IpLimit));
        }
        if state.subnets.get(&subnet).copied().unwrap_or(0) >= self.limits.max_pending_per_subnet {
            return Err(AdmissionDenied::new(AdmissionRejection::SubnetLimit));
        }
        state.total += 1;
        *state.ips.entry(ip).or_default() += 1;
        *state.subnets.entry(subnet).or_default() += 1;
        Ok(InboundPermit {
            state: Arc::clone(&self.state),
            ip,
            subnet,
        })
    }

    /// Returns privacy-safe admission counters.
    pub fn snapshot(&self) -> AdmissionSnapshot {
        let Ok(state) = self.state.lock() else {
            return AdmissionSnapshot {
                pending: 0,
                distinct_ips: 0,
                distinct_subnets: 0,
            };
        };
        AdmissionSnapshot {
            pending: state.total,
            distinct_ips: state.ips.len(),
            distinct_subnets: state.subnets.len(),
        }
    }
}

/// Typed admission rejection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdmissionRejection {
    /// The global pending limit is full.
    GlobalLimit,
    /// The exact-IP limit is full.
    IpLimit,
    /// The subnet limit is full.
    SubnetLimit,
    /// State could not be inspected safely.
    StatePoisoned,
}

/// Privacy-safe admission denial.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdmissionDenied {
    /// Typed reason for denial.
    pub rejection: AdmissionRejection,
}

impl AdmissionDenied {
    const fn new(rejection: AdmissionRejection) -> Self {
        Self { rejection }
    }
}

impl fmt::Display for AdmissionDenied {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "NTCP2 inbound admission denied: {:?}",
            self.rejection
        )
    }
}

impl std::error::Error for AdmissionDenied {}

/// Exact inbound admission counters.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdmissionSnapshot {
    /// Current pending attempts.
    pub pending: usize,
    /// Number of exact IP buckets currently occupied.
    pub distinct_ips: usize,
    /// Number of subnet buckets currently occupied.
    pub distinct_subnets: usize,
}

/// One exact inbound admission lease.
pub struct InboundPermit {
    state: Arc<Mutex<AdmissionState>>,
    ip: IpAddr,
    subnet: PrefixKey,
}

impl fmt::Debug for InboundPermit {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("InboundPermit(..)")
    }
}

impl Drop for InboundPermit {
    fn drop(&mut self) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        state.total = state.total.saturating_sub(1);
        decrement(&mut state.ips, self.ip);
        decrement(&mut state.subnets, self.subnet);
    }
}

fn decrement<K: Eq + std::hash::Hash>(map: &mut HashMap<K, usize>, key: K) {
    if let Some(value) = map.get_mut(&key) {
        *value = value.saturating_sub(1);
        if *value == 0 {
            map.remove(&key);
        }
    }
}

/// A received stream waiting for a supervised link handoff.
pub struct InboundChunk {
    stream: TcpStream,
    permit: Option<InboundPermit>,
    family: AddressFamily,
}

impl fmt::Debug for InboundChunk {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("InboundChunk")
            .field("family", &self.family)
            .field("stream", &"<owned>")
            .finish()
    }
}

impl InboundChunk {
    /// Returns its coarse address family.
    pub const fn family(&self) -> AddressFamily {
        self.family
    }

    /// Transfers the accepted stream to a link owner.
    pub fn into_stream(mut self) -> TcpStream {
        let _ = self.permit.take();
        self.stream
    }
}

/// A bounded listener handle receiving admitted streams.
pub struct ListenerHandle {
    receiver: mpsc::Receiver<InboundChunk>,
    cancellation: CancellationToken,
    local_addr: SocketAddr,
}

/// Privacy-safe listener counters.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ListenerSnapshot {
    /// Number of queued stream slots represented by the handle.
    pub queued: usize,
    /// Bound address family.
    pub family: AddressFamily,
}

impl fmt::Debug for ListenerHandle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ListenerHandle")
            .field("local_addr", &"<redacted>")
            .finish()
    }
}

impl ListenerHandle {
    /// Receives the next admitted stream, or `None` after shutdown.
    pub async fn next(&mut self) -> Option<InboundChunk> {
        self.receiver.recv().await
    }

    /// Requests listener shutdown. The owning service scope still joins the
    /// accept task.
    pub fn shutdown(&self) {
        let _ = self
            .cancellation
            .cancel(CancellationReason::OperatorRequest);
    }

    /// Returns the bound address for controlled test setup.
    pub const fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }
}

impl Drop for ListenerHandle {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// A listener before it is attached to a supervised child scope.
pub struct BoundNtcp2Listener {
    listener: Arc<TcpListener>,
    admission: InboundAdmission,
    queue: usize,
}

impl fmt::Debug for BoundNtcp2Listener {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("BoundNtcp2Listener(..)")
    }
}

impl BoundNtcp2Listener {
    /// Binds a listener; this is the only socket-opening constructor.
    pub async fn bind(
        address: SocketAddr,
        admission: InboundAdmission,
    ) -> Result<Self, IoErrorKind> {
        let listener = TcpListener::bind(address)
            .await
            .map_err(|_| IoErrorKind::Failed)?;
        let queue = admission.limits.max_accept_queue;
        Ok(Self {
            listener: Arc::new(listener),
            admission,
            queue,
        })
    }

    /// Attaches the accept loop to an existing supervised child scope.
    pub fn start(self, scope: &ChildScope) -> Result<ListenerHandle, ChildScopeError> {
        let (sender, receiver) = mpsc::channel(self.queue);
        let cancellation = CancellationToken::new();
        let task_cancellation = cancellation.clone();
        let listener = Arc::clone(&self.listener);
        let admission = self.admission.clone();
        let local_addr = listener.local_addr().map_err(|_| ChildScopeError::Closed)?;
        scope.spawn(move |child| async move {
            loop {
                tokio::select! {
                    _ = child.cancelled() => return Ok(()),
                    _ = task_cancellation.cancelled() => return Ok(()),
                    accepted = listener.accept() => {
                        let (stream, address) = match accepted {
                            Ok(value) => value,
                            Err(_) => return Err(ChildTaskFailure::Explicit),
                        };
                        let Ok(permit) = admission.admit(address) else {
                            drop(stream);
                            continue;
                        };
                        let chunk = InboundChunk {
                            stream,
                            permit: Some(permit),
                            family: AddressFamily::of(address.ip()),
                        };
                        if sender.send(chunk).await.is_err() {
                            return Ok(());
                        }
                    }
                }
            }
        })?;
        Ok(ListenerHandle {
            receiver,
            cancellation,
            local_addr,
        })
    }
}

/// A bounded replay-cache decision.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReplayCacheDecision {
    /// The token was absent and has been retained.
    Fresh,
    /// The token was already present and has been rejected.
    Replayed,
    /// The cache was full and fails closed.
    Full,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ReplayEntry {
    expires_at: u64,
}

/// Bounded runtime owner for handshake replay tokens.
#[derive(Clone)]
pub struct ReplayCache {
    maximum: usize,
    entries: Arc<Mutex<BTreeMap<[u8; 32], ReplayEntry>>>,
}

impl fmt::Debug for ReplayCache {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ReplayCache(..)")
    }
}

impl ReplayCache {
    /// Creates an empty cache with a fixed positive capacity.
    pub fn new(maximum: usize) -> Result<Self, Ntcp2RuntimeConfigError> {
        if maximum == 0 {
            return Err(Ntcp2RuntimeConfigError::ZeroLimit {
                kind: RuntimeLimitKind::ReplayEntries,
            });
        }
        Ok(Self {
            maximum,
            entries: Arc::new(Mutex::new(BTreeMap::new())),
        })
    }

    /// Checks and records a fixed-size token, expiring old entries first.
    pub fn check_and_record(
        &self,
        token: [u8; 32],
        now: u64,
        retention: u64,
    ) -> ReplayCacheDecision {
        let Ok(mut entries) = self.entries.lock() else {
            return ReplayCacheDecision::Full;
        };
        entries.retain(|_, entry| entry.expires_at > now);
        if entries.contains_key(&token) {
            return ReplayCacheDecision::Replayed;
        }
        if entries.len() >= self.maximum {
            return ReplayCacheDecision::Full;
        }
        entries.insert(
            token,
            ReplayEntry {
                expires_at: now.saturating_add(retention),
            },
        );
        ReplayCacheDecision::Fresh
    }

    /// Removes all entries during explicit runtime teardown.
    pub fn clear(&self) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.clear();
        }
    }

    /// Returns a redacted cache snapshot.
    pub fn snapshot(&self) -> ReplayCacheSnapshot {
        let len = self
            .entries
            .lock()
            .map(|entries| entries.len())
            .unwrap_or(0);
        ReplayCacheSnapshot {
            entries: len,
            capacity: self.maximum,
        }
    }
}

/// Redacted replay-cache counters.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReplayCacheSnapshot {
    /// Current entries.
    pub entries: usize,
    /// Fixed capacity.
    pub capacity: usize,
}

/// Stable, redacted key for dial backoff.
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct DialKey([u8; 32]);

/// Dial-key construction failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DialKeyError {
    /// A zero digest is reserved as an invalid sentinel.
    Zero,
}

impl DialKey {
    /// Creates a key from an opaque peer/address digest.
    pub const fn new(value: [u8; 32]) -> Self {
        Self(value)
    }

    /// Creates a key while rejecting the reserved all-zero value.
    pub fn try_new(value: [u8; 32]) -> Result<Self, DialKeyError> {
        if value == [0; 32] {
            Err(DialKeyError::Zero)
        } else {
            Ok(Self(value))
        }
    }
}

impl fmt::Debug for DialKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DialKey(<redacted>)")
    }
}

/// Dial-backoff policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DialBackoffConfig {
    /// Initial delay.
    pub initial: Duration,
    /// Maximum delay.
    pub maximum: Duration,
    /// Maximum attempts retained per key.
    pub max_attempts: u16,
}

impl Default for DialBackoffConfig {
    fn default() -> Self {
        Self {
            initial: Duration::from_millis(100),
            maximum: Duration::from_secs(60),
            max_attempts: 8,
        }
    }
}

/// Result of consulting bounded backoff state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DialBackoffDecision {
    /// A dial may be attempted immediately.
    Allowed,
    /// A retry must wait for this bounded duration.
    Wait(Duration),
    /// No further retry is admitted for this key.
    Exhausted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct BackoffEntry {
    until: tokio::time::Instant,
    attempts: u16,
}

/// A bounded, expiring dial-backoff owner.
#[derive(Clone)]
pub struct DialAdmission {
    config: DialBackoffConfig,
    maximum: usize,
    entries: Arc<Mutex<HashMap<DialKey, BackoffEntry>>>,
}

impl fmt::Debug for DialAdmission {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DialAdmission(..)")
    }
}

impl DialAdmission {
    /// Creates a bounded backoff owner.
    pub fn new(config: DialBackoffConfig, maximum: usize) -> Result<Self, Ntcp2RuntimeConfigError> {
        if config.initial.is_zero() || config.maximum.is_zero() || config.initial > config.maximum {
            return Err(Ntcp2RuntimeConfigError::ZeroDeadline { field: "backoff" });
        }
        if config.maximum > MAX_NTCP2_RUNTIME_DURATION || config.max_attempts == 0 {
            return Err(Ntcp2RuntimeConfigError::DeadlineTooLong { field: "backoff" });
        }
        if maximum == 0 {
            return Err(Ntcp2RuntimeConfigError::ZeroLimit {
                kind: RuntimeLimitKind::BackoffEntries,
            });
        }
        Ok(Self {
            config,
            maximum,
            entries: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    /// Checks whether a key may be attempted now.
    pub fn check(&self, key: DialKey) -> DialBackoffDecision {
        let Ok(mut entries) = self.entries.lock() else {
            return DialBackoffDecision::Exhausted;
        };
        let now = tokio::time::Instant::now();
        entries.retain(|_, entry| entry.until > now);
        let Some(entry) = entries.get(&key) else {
            return DialBackoffDecision::Allowed;
        };
        if entry.attempts >= self.config.max_attempts {
            DialBackoffDecision::Exhausted
        } else {
            DialBackoffDecision::Wait(entry.until.saturating_duration_since(now))
        }
    }

    /// Records a failed attempt with bounded exponential delay.
    pub fn record_failure(&self, key: DialKey) -> DialBackoffDecision {
        let Ok(mut entries) = self.entries.lock() else {
            return DialBackoffDecision::Exhausted;
        };
        let now = tokio::time::Instant::now();
        entries.retain(|_, entry| entry.until > now);
        if !entries.contains_key(&key) && entries.len() >= self.maximum {
            return DialBackoffDecision::Exhausted;
        }
        let entry = entries.entry(key).or_insert(BackoffEntry {
            until: now,
            attempts: 0,
        });
        entry.attempts = entry.attempts.saturating_add(1);
        let exponent = u32::from(entry.attempts.saturating_sub(1).min(15));
        let delay = self
            .config
            .initial
            .checked_mul(1_u32 << exponent)
            .unwrap_or(self.config.maximum)
            .min(self.config.maximum);
        entry.until = now + delay;
        DialBackoffDecision::Wait(delay)
    }

    /// Clears a successful key.
    pub fn clear(&self, key: DialKey) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.remove(&key);
        }
    }

    /// Returns bounded backoff entry count.
    pub fn snapshot(&self) -> DialBackoffSnapshot {
        let entries = self.entries.lock().map(|value| value.len()).unwrap_or(0);
        DialBackoffSnapshot {
            entries,
            capacity: self.maximum,
        }
    }
}

/// Redacted backoff counters.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DialBackoffSnapshot {
    /// Current entries.
    pub entries: usize,
    /// Capacity.
    pub capacity: usize,
}

/// Typed result of an outbound dial.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DialOutcome {
    /// A TCP stream was connected.
    Connected,
    /// Backoff or global admission rejected the attempt.
    ResourceDenied,
    /// The caller cancelled.
    Cancelled,
    /// The deadline elapsed.
    Deadline,
    /// The socket operation failed.
    Failed,
}

/// An admitted outbound TCP attempt.
pub struct DialAttempt {
    stream: TcpStream,
    family: AddressFamily,
}

impl fmt::Debug for DialAttempt {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DialAttempt")
            .field("family", &self.family)
            .field("stream", &"<owned>")
            .finish()
    }
}

impl DialAttempt {
    /// Returns the coarse family of the connected target.
    pub const fn family(&self) -> AddressFamily {
        self.family
    }

    /// Transfers the socket to a link owner.
    pub fn into_stream(self) -> TcpStream {
        self.stream
    }
}

/// A bounded outcome for link writes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WriteOutcome {
    /// The bytes entered the bounded writer queue.
    Accepted,
    /// The link queue is full.
    QueueFull,
    /// The caller cancelled or exceeded its deadline.
    Cancelled,
    /// The link is closed.
    Closed,
}

/// Typed link send failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LinkSendError {
    /// Message exceeded the fixed runtime bound.
    TooLarge,
    /// The link queue was full at its deadline.
    QueueFull,
    /// Caller cancellation won.
    Cancelled,
    /// Link was closed.
    Closed,
}

/// Terminal link category.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LinkTermination {
    /// Owner requested shutdown.
    Cancelled,
    /// Reader observed EOF.
    PeerClosed,
    /// A bounded I/O operation failed.
    IoFailure,
    /// A sibling task failed.
    SiblingFailure,
}

/// Redacted link counters.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LinkSnapshot {
    /// Local link identifier.
    pub id: u64,
    /// Coarse family.
    pub family: AddressFamily,
    /// Current queued item count.
    pub queued_items: usize,
    /// Current queued bytes.
    pub queued_bytes: usize,
    /// Bytes read by the supervised reader.
    pub read_bytes: u64,
    /// Bytes written by the supervised writer.
    pub written_bytes: u64,
    /// Whether link teardown has started.
    pub closed: bool,
}

struct LinkState {
    closed: AtomicBool,
    queued_items: AtomicUsize,
    queued_bytes: AtomicUsize,
    read_bytes: AtomicU64,
    written_bytes: AtomicU64,
}

/// An owned link façade backed by supervised reader/writer children.
pub struct LinkHandle {
    id: u64,
    family: AddressFamily,
    cancellation: CancellationToken,
    sender: mpsc::Sender<Vec<u8>>,
    state: Arc<LinkState>,
    maximum_items: usize,
    maximum_bytes: usize,
}

impl fmt::Debug for LinkHandle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LinkHandle")
            .field("id", &self.id)
            .field("family", &self.family)
            .field("snapshot", &self.snapshot())
            .finish()
    }
}

impl LinkHandle {
    /// Starts a reader and writer child for one connected stream.
    pub fn start(
        scope: &ChildScope,
        stream: TcpStream,
        family: AddressFamily,
        id: u64,
        limits: Ntcp2RuntimeLimits,
    ) -> Result<Self, ChildScopeError> {
        let (sender, mut receiver): (mpsc::Sender<Vec<u8>>, mpsc::Receiver<Vec<u8>>) =
            mpsc::channel(limits.max_link_queue_items);
        let cancellation = CancellationToken::new();
        let shared = Arc::new(LinkState {
            closed: AtomicBool::new(false),
            queued_items: AtomicUsize::new(0),
            queued_bytes: AtomicUsize::new(0),
            read_bytes: AtomicU64::new(0),
            written_bytes: AtomicU64::new(0),
        });
        let (mut reader, mut writer) = stream.into_split();
        let reader_cancel = cancellation.clone();
        let reader_state = Arc::clone(&shared);
        let writer_cancel = cancellation.clone();
        let writer_state = Arc::clone(&shared);
        scope.spawn(move |child| async move {
            let mut buffer = [0_u8; 4096];
            loop {
                tokio::select! {
                    _ = child.cancelled() => break,
                    _ = reader_cancel.cancelled() => break,
                    result = reader.read(&mut buffer) => match result {
                        Ok(0) => {
                            reader_state.closed.store(true, Ordering::Release);
                            let _ = reader_cancel.cancel(CancellationReason::ParentScope);
                            break;
                        }
                        Ok(length) => {
                            reader_state.read_bytes.fetch_add(length as u64, Ordering::Relaxed);
                        }
                        Err(_) => {
                            reader_state.closed.store(true, Ordering::Release);
                            let _ = reader_cancel.cancel(CancellationReason::ParentScope);
                            break;
                        }
                    }
                }
            }
            Ok(())
        })?;
        scope.spawn(move |child| async move {
            loop {
                tokio::select! {
                    _ = child.cancelled() => break,
                    _ = writer_cancel.cancelled() => break,
                    item = receiver.recv() => match item {
                        Some(bytes) => {
                            if writer.write_all(&bytes).await.is_err() {
                                writer_state.closed.store(true, Ordering::Release);
                                let _ = writer_cancel.cancel(CancellationReason::ParentScope);
                                break;
                            }
                            writer_state.written_bytes.fetch_add(bytes.len() as u64, Ordering::Relaxed);
                            writer_state.queued_items.fetch_sub(1, Ordering::Relaxed);
                            writer_state.queued_bytes.fetch_sub(bytes.len(), Ordering::Relaxed);
                        }
                        None => break,
                    }
                }
            }
            Ok(())
        })?;
        Ok(Self {
            id,
            family,
            cancellation,
            sender,
            state: shared,
            maximum_items: limits.max_link_queue_items,
            maximum_bytes: limits.max_link_queue_bytes,
        })
    }

    /// Attempts to queue one bounded write under a caller deadline.
    pub async fn send(
        &self,
        bytes: Vec<u8>,
        deadline: Ntcp2Deadline,
        cancellation: &CancellationToken,
    ) -> Result<WriteOutcome, LinkSendError> {
        if bytes.is_empty() || bytes.len() > MAX_NTCP2_LINK_MESSAGE_BYTES {
            return Err(LinkSendError::TooLarge);
        }
        if self.state.closed.load(Ordering::Acquire) {
            return Err(LinkSendError::Closed);
        }
        let length = bytes.len();
        let previous_items = self.state.queued_items.fetch_add(1, Ordering::AcqRel);
        let previous_bytes = self.state.queued_bytes.fetch_add(length, Ordering::AcqRel);
        if previous_items >= self.maximum_items
            || previous_bytes.saturating_add(length) > self.maximum_bytes
        {
            self.state.queued_items.fetch_sub(1, Ordering::AcqRel);
            self.state.queued_bytes.fetch_sub(length, Ordering::AcqRel);
            return Err(LinkSendError::QueueFull);
        }
        let send = tokio::select! {
            _ = cancellation.cancelled() => Err(LinkSendError::Cancelled),
            _ = self.cancellation.cancelled() => Err(LinkSendError::Closed),
            result = tokio::time::timeout(deadline.remaining(), self.sender.send(bytes)) => {
                match result {
                    Ok(Ok(())) => Ok(WriteOutcome::Accepted),
                    Ok(Err(_)) => Err(LinkSendError::Closed),
                    Err(_) => Err(LinkSendError::QueueFull),
                }
            }
        };
        if send.is_err() {
            self.state.queued_items.fetch_sub(1, Ordering::AcqRel);
            self.state.queued_bytes.fetch_sub(length, Ordering::AcqRel);
        }
        send
    }

    /// Requests cancellation; the service scope remains responsible for join.
    pub fn close(&self) {
        let _ = self
            .cancellation
            .cancel(CancellationReason::OperatorRequest);
        self.state.closed.store(true, Ordering::Release);
    }

    /// Returns privacy-safe counters.
    pub fn snapshot(&self) -> LinkSnapshot {
        LinkSnapshot {
            id: self.id,
            family: self.family,
            queued_items: self.state.queued_items.load(Ordering::Acquire),
            queued_bytes: self.state.queued_bytes.load(Ordering::Acquire),
            read_bytes: self.state.read_bytes.load(Ordering::Acquire),
            written_bytes: self.state.written_bytes.load(Ordering::Acquire),
            closed: self.state.closed.load(Ordering::Acquire),
        }
    }
}

impl Drop for LinkHandle {
    fn drop(&mut self) {
        self.close();
    }
}

/// Fixed runtime event categories.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Ntcp2EventKind {
    /// Listener accepted an admitted stream.
    Accepted,
    /// Admission rejected a stream.
    AdmissionDenied,
    /// Dial completed.
    DialCompleted,
    /// Link was replaced or drained.
    LinkReplaced,
    /// Link closed.
    LinkClosed,
}

/// Privacy-safe runtime event.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Ntcp2Event {
    /// Fixed category.
    pub kind: Ntcp2EventKind,
    /// Optional local link ID.
    pub link_id: Option<u64>,
    /// Coarse address family.
    pub family: Option<AddressFamily>,
}

/// Aggregate runtime snapshot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RuntimeSnapshot {
    /// Admission counters.
    pub admission: AdmissionSnapshot,
    /// Replay counters.
    pub replay: ReplayCacheSnapshot,
    /// Backoff counters.
    pub backoff: DialBackoffSnapshot,
}

/// Small runtime service owner for controlled NTCP2 socket setup.
#[derive(Clone)]
pub struct Ntcp2RuntimeService {
    config: Ntcp2RuntimeConfig,
    admission: InboundAdmission,
    replay: ReplayCache,
    backoff: DialAdmission,
}

impl fmt::Debug for Ntcp2RuntimeService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Ntcp2RuntimeService(..)")
    }
}

impl Ntcp2RuntimeService {
    /// Creates a bounded runtime service without opening a socket.
    pub fn new(config: Ntcp2RuntimeConfig) -> Result<Self, Ntcp2RuntimeConfigError> {
        config.validate()?;
        Ok(Self {
            admission: InboundAdmission::new(config)?,
            replay: ReplayCache::new(config.limits.max_replay_entries)?,
            backoff: DialAdmission::new(
                DialBackoffConfig::default(),
                config.limits.max_backoff_entries,
            )?,
            config,
        })
    }

    /// Starts a bounded listener under a caller-owned service scope.
    pub async fn listen(
        &self,
        address: SocketAddr,
        scope: &ChildScope,
    ) -> Result<ListenerHandle, IoErrorKind> {
        BoundNtcp2Listener::bind(address, self.admission.clone())
            .await?
            .start(scope)
            .map_err(|_| IoErrorKind::Failed)
    }

    /// Dials one resolved literal target under the configured deadline.
    pub async fn dial(
        &self,
        address: SocketAddr,
        cancellation: &CancellationToken,
    ) -> Result<DialAttempt, DialOutcome> {
        let deadline = Ntcp2Deadline::after(self.config.deadlines.connect)
            .map_err(|_| DialOutcome::Deadline)?;
        tokio::select! {
            _ = cancellation.cancelled() => Err(DialOutcome::Cancelled),
            result = tokio::time::timeout(deadline.remaining(), TcpStream::connect(address)) => {
                match result {
                    Ok(Ok(stream)) => Ok(DialAttempt { stream, family: AddressFamily::of(address.ip()) }),
                    Ok(Err(_)) => Err(DialOutcome::Failed),
                    Err(_) => Err(DialOutcome::Deadline),
                }
            }
        }
    }

    /// Returns the admission owner for callers that perform explicit accept.
    pub fn admission(&self) -> &InboundAdmission {
        &self.admission
    }

    /// Returns the replay owner.
    pub fn replay_cache(&self) -> &ReplayCache {
        &self.replay
    }

    /// Returns the backoff owner.
    pub fn dial_admission(&self) -> &DialAdmission {
        &self.backoff
    }

    /// Returns aggregate privacy-safe counters.
    pub fn snapshot(&self) -> RuntimeSnapshot {
        RuntimeSnapshot {
            admission: self.admission.snapshot(),
            replay: self.replay.snapshot(),
            backoff: self.backoff.snapshot(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limits_and_prefixes_are_bounded() {
        assert!(IpPrefixPolicy::new(33, 64).is_err());
        assert!(Ntcp2RuntimeConfig::default().validate().is_ok());
        let limits = Ntcp2RuntimeLimits {
            max_pending_per_ip: 0,
            ..Default::default()
        };
        assert!(matches!(
            limits.validate(),
            Err(Ntcp2RuntimeConfigError::ZeroLimit { .. })
        ));
    }

    #[test]
    fn admission_is_global_ip_and_subnet_bounded_and_releases() {
        let limits = Ntcp2RuntimeLimits {
            max_pending_inbound: 2,
            max_pending_per_ip: 1,
            max_pending_per_subnet: 1,
            ..Default::default()
        };
        let admission = InboundAdmission::new(Ntcp2RuntimeConfig {
            limits,
            ..Default::default()
        })
        .expect("admission");
        let first = admission
            .admit("192.0.2.1:12345".parse().unwrap())
            .expect("first");
        assert!(matches!(
            admission.admit("192.0.2.1:12346".parse().unwrap()),
            Err(AdmissionDenied {
                rejection: AdmissionRejection::IpLimit
            })
        ));
        assert!(matches!(
            admission.admit("192.0.2.2:12345".parse().unwrap()),
            Err(AdmissionDenied {
                rejection: AdmissionRejection::SubnetLimit
            })
        ));
        assert_eq!(admission.snapshot().pending, 1);
        drop(first);
        assert_eq!(admission.snapshot().pending, 0);
    }

    #[test]
    fn replay_cache_fails_closed_and_expires_deterministically() {
        let cache = ReplayCache::new(1).expect("cache");
        assert_eq!(
            cache.check_and_record([1; 32], 10, 5),
            ReplayCacheDecision::Fresh
        );
        assert_eq!(
            cache.check_and_record([1; 32], 11, 5),
            ReplayCacheDecision::Replayed
        );
        assert_eq!(
            cache.check_and_record([2; 32], 11, 5),
            ReplayCacheDecision::Full
        );
        assert_eq!(
            cache.check_and_record([2; 32], 15, 5),
            ReplayCacheDecision::Fresh
        );
        assert_eq!(cache.snapshot().entries, 1);
    }

    #[tokio::test(start_paused = true)]
    async fn loopback_listener_and_exact_io_use_supervised_scope() {
        let service = Ntcp2RuntimeService::new(Ntcp2RuntimeConfig::default()).expect("service");
        let token = CancellationToken::new();
        let children = ChildScope::new(
            &token,
            crate::ChildFailurePolicy::FailParent,
            crate::observability::TaskCounters::new(),
        );
        let mut listener = service
            .listen("127.0.0.1:0".parse().unwrap(), &children)
            .await
            .expect("listener");
        let address = listener.local_addr();
        let client = TcpStream::connect(address);
        let (client, _) = tokio::join!(client, async { tokio::task::yield_now().await });
        let mut client = client.expect("connect");
        let incoming = listener.next().await.expect("incoming");
        let mut server = incoming.into_stream();
        let deadline = Ntcp2Deadline::after(Duration::from_secs(5)).expect("deadline");
        client.write_all(b"ok").await.expect("write");
        let mut bytes = [0; 2];
        read_exact(&mut server, &mut bytes, deadline, &CancellationToken::new())
            .await
            .expect("read");
        assert_eq!(&bytes, b"ok");
        listener.shutdown();
        let _ = children.shutdown().await;
        assert_eq!(service.snapshot().admission.pending, 0);
    }

    #[tokio::test(start_paused = true)]
    async fn link_reader_and_writer_are_joined_after_close() {
        let token = CancellationToken::new();
        let children = ChildScope::new(
            &token,
            crate::ChildFailurePolicy::FailParent,
            crate::observability::TaskCounters::new(),
        );
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let address = listener.local_addr().expect("local address");
        let (client, accepted) = tokio::join!(TcpStream::connect(address), listener.accept());
        let mut client = client.expect("connect");
        let (server, _) = accepted.expect("accept");
        let link = LinkHandle::start(
            &children,
            server,
            AddressFamily::Ipv4,
            7,
            Ntcp2RuntimeLimits::default(),
        )
        .expect("link");
        let deadline = Ntcp2Deadline::after(Duration::from_secs(5)).expect("deadline");
        link.send(b"ok".to_vec(), deadline, &CancellationToken::new())
            .await
            .expect("queue write");
        let mut bytes = [0_u8; 2];
        read_exact(&mut client, &mut bytes, deadline, &CancellationToken::new())
            .await
            .expect("read queued bytes");
        assert_eq!(&bytes, b"ok");

        client.write_all(b"peer").await.expect("peer write");
        for _ in 0..3 {
            tokio::task::yield_now().await;
        }
        assert!(link.snapshot().read_bytes >= 4);
        assert!(link.snapshot().written_bytes >= 2);

        link.close();
        let report = children.shutdown().await;
        assert_eq!(report.joined(), 2);
        assert!(link.snapshot().closed);
    }
}
