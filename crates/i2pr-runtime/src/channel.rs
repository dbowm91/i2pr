//! Bounded service communication with typed overload and cleanup outcomes.
//!
//! The wrappers in this module deliberately expose no deadline-free service
//! send operation.  A sender reserves queue capacity before acquiring an
//! optional resource lease, so a task waiting for a slot cannot retain a
//! lease indefinitely.  Once accepted, the queue entry owns its lease until
//! the receiver takes ownership or the entry is dropped during shutdown.

use std::fmt;
use std::ops::Deref;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

use i2pr_core::{
    ResourceBudget, ResourceClass, ResourceError, ResourceLease, ResourceRequest, ServiceName,
};
use tokio::sync::{mpsc, oneshot, watch};

use crate::CancellationToken;

/// Hard ceiling for one infrastructure channel.
pub const MAX_CHANNEL_CAPACITY: usize = 4_096;
/// Maximum UTF-8 byte length of a channel identifier.
pub const MAX_CHANNEL_NAME_BYTES: usize = 64;
/// Hard ceiling for a caller-provided byte estimate.
pub const MAX_QUEUE_ITEM_BYTES: u64 = 1 << 20;

/// Bounded identifier used in channel metadata and diagnostics.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ChannelName(String);

impl ChannelName {
    /// Creates an identifier with stable, metric-safe characters.
    pub fn new(value: impl Into<String>) -> Result<Self, ChannelNameError> {
        let value = value.into();
        if value.is_empty() {
            return Err(ChannelNameError::Empty);
        }
        if value.len() > MAX_CHANNEL_NAME_BYTES {
            return Err(ChannelNameError::TooLong {
                maximum: MAX_CHANNEL_NAME_BYTES,
            });
        }
        if !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
        {
            return Err(ChannelNameError::InvalidCharacter);
        }
        Ok(Self(value))
    }

    /// Returns the validated identifier.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for ChannelName {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for ChannelName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Errors returned while validating a channel identifier.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChannelNameError {
    /// The identifier was empty.
    Empty,
    /// The identifier exceeded the byte limit.
    TooLong { maximum: usize },
    /// The identifier contained a character outside the metric-safe set.
    InvalidCharacter,
}

impl fmt::Display for ChannelNameError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("channel name must not be empty"),
            Self::TooLong { maximum } => {
                write!(formatter, "channel name exceeds the {maximum}-byte limit")
            }
            Self::InvalidCharacter => {
                formatter.write_str("channel name contains an invalid character")
            }
        }
    }
}

impl std::error::Error for ChannelNameError {}

/// Communication taxonomy used by service plans.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CommunicationClass {
    /// Ordered point-to-point instructions.
    Command,
    /// A command with one bounded response path.
    Request,
    /// Bounded single-consumer observations with an explicit drop policy.
    Event,
    /// A latest-value snapshot with no historical delivery guarantee.
    LatestState,
}

/// Explicit policy for a channel when consumers cannot keep up.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum OverflowPolicy {
    /// Wait only when the caller supplies a deadline and cancellation scope.
    WaitUntilDeadline,
    /// Reject immediately when no slot is available.
    RejectWhenFull,
    /// Discard the new observation and count the drop.
    DropNewest,
    /// Keep only the newest value.
    CoalesceLatest,
}

/// Optional resource charge attached to each accepted queue item.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum QueueCharge {
    /// Charge a fixed amount for each item.
    PerItem { class: ResourceClass, units: u64 },
    /// Charge the caller-provided byte estimate, capped by `maximum`.
    PerBytes { class: ResourceClass, maximum: u64 },
}

impl QueueCharge {
    /// Constructs a fixed per-item charge.
    pub const fn per_item(class: ResourceClass, units: u64) -> Result<Self, ChannelConfigError> {
        if units == 0 {
            Err(ChannelConfigError::ZeroCharge)
        } else {
            Ok(Self::PerItem { class, units })
        }
    }

    /// Constructs a bounded byte-estimate charge.
    pub const fn per_bytes(class: ResourceClass, maximum: u64) -> Result<Self, ChannelConfigError> {
        if maximum == 0 {
            return Err(ChannelConfigError::ZeroCharge);
        }
        if maximum > MAX_QUEUE_ITEM_BYTES {
            return Err(ChannelConfigError::ItemEstimateTooLarge {
                maximum: MAX_QUEUE_ITEM_BYTES,
            });
        }
        Ok(Self::PerBytes { class, maximum })
    }
}

/// Validated metadata used to construct one bounded channel.
#[derive(Clone)]
pub struct ChannelSpec {
    name: ChannelName,
    owner: ServiceName,
    capacity: usize,
    class: CommunicationClass,
    overflow: OverflowPolicy,
    charge: Option<QueueCharge>,
    budget: Option<ResourceBudget>,
}

impl fmt::Debug for ChannelSpec {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ChannelSpec")
            .field("name", &self.name)
            .field("owner", &self.owner)
            .field("capacity", &self.capacity)
            .field("class", &self.class)
            .field("overflow", &self.overflow)
            .field("charge", &self.charge)
            .field("has_budget", &self.budget.is_some())
            .finish()
    }
}

impl ChannelSpec {
    /// Constructs metadata after validating capacity and policy.
    pub fn new(
        name: impl Into<String>,
        owner: ServiceName,
        capacity: usize,
        class: CommunicationClass,
        overflow: OverflowPolicy,
    ) -> Result<Self, ChannelConfigError> {
        let name = ChannelName::new(name).map_err(ChannelConfigError::InvalidName)?;
        if capacity == 0 {
            return Err(ChannelConfigError::ZeroCapacity);
        }
        if capacity > MAX_CHANNEL_CAPACITY {
            return Err(ChannelConfigError::CapacityTooLarge {
                maximum: MAX_CHANNEL_CAPACITY,
            });
        }
        let expected = match class {
            CommunicationClass::Command | CommunicationClass::Request => {
                OverflowPolicy::WaitUntilDeadline
            }
            CommunicationClass::Event => OverflowPolicy::DropNewest,
            CommunicationClass::LatestState => OverflowPolicy::CoalesceLatest,
        };
        if overflow != expected {
            return Err(ChannelConfigError::InvalidOverflowPolicy { class, expected });
        }
        if class == CommunicationClass::LatestState && capacity != 1 {
            return Err(ChannelConfigError::LatestStateCapacity);
        }
        Ok(Self {
            name,
            owner,
            capacity,
            class,
            overflow,
            charge: None,
            budget: None,
        })
    }

    /// Constructs a bounded command channel specification.
    pub fn command(
        name: impl Into<String>,
        owner: ServiceName,
        capacity: usize,
    ) -> Result<Self, ChannelConfigError> {
        Self::new(
            name,
            owner,
            capacity,
            CommunicationClass::Command,
            OverflowPolicy::WaitUntilDeadline,
        )
    }

    /// Constructs a bounded request channel specification.
    pub fn request(
        name: impl Into<String>,
        owner: ServiceName,
        capacity: usize,
    ) -> Result<Self, ChannelConfigError> {
        Self::new(
            name,
            owner,
            capacity,
            CommunicationClass::Request,
            OverflowPolicy::WaitUntilDeadline,
        )
    }

    /// Constructs a bounded single-consumer, drop-newest event specification.
    pub fn event(
        name: impl Into<String>,
        owner: ServiceName,
        capacity: usize,
    ) -> Result<Self, ChannelConfigError> {
        Self::new(
            name,
            owner,
            capacity,
            CommunicationClass::Event,
            OverflowPolicy::DropNewest,
        )
    }

    /// Constructs a latest-state specification with one current value slot.
    pub fn latest_state(
        name: impl Into<String>,
        owner: ServiceName,
    ) -> Result<Self, ChannelConfigError> {
        Self::new(
            name,
            owner,
            1,
            CommunicationClass::LatestState,
            OverflowPolicy::CoalesceLatest,
        )
    }

    /// Adds a fixed resource charge to accepted items.
    pub fn with_item_charge(
        mut self,
        class: ResourceClass,
        units: u64,
    ) -> Result<Self, ChannelConfigError> {
        self.charge = Some(QueueCharge::per_item(class, units)?);
        Ok(self)
    }

    /// Adds a caller-estimated byte charge to accepted items.
    pub fn with_byte_charge(
        mut self,
        class: ResourceClass,
        maximum: u64,
    ) -> Result<Self, ChannelConfigError> {
        self.charge = Some(QueueCharge::per_bytes(class, maximum)?);
        Ok(self)
    }

    /// Supplies the immutable budget used for queue admission.
    pub fn with_budget(mut self, budget: ResourceBudget) -> Self {
        self.budget = Some(budget);
        self
    }

    /// Channel identifier.
    pub fn name(&self) -> &ChannelName {
        &self.name
    }

    /// Owning service identifier.
    pub fn owner(&self) -> &ServiceName {
        &self.owner
    }

    /// Configured queue capacity.
    pub const fn capacity(&self) -> usize {
        self.capacity
    }

    /// Communication taxonomy.
    pub const fn class(&self) -> CommunicationClass {
        self.class
    }

    /// Explicit overload policy.
    pub const fn overflow(&self) -> OverflowPolicy {
        self.overflow
    }

    /// Optional charge attached to accepted items.
    pub const fn charge(&self) -> Option<QueueCharge> {
        self.charge
    }
}

/// Errors returned while constructing channel metadata or wrappers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChannelConfigError {
    /// The channel name was invalid.
    InvalidName(ChannelNameError),
    /// Capacity was zero.
    ZeroCapacity,
    /// Capacity exceeded the hard infrastructure ceiling.
    CapacityTooLarge { maximum: usize },
    /// The selected class requires a different explicit policy.
    InvalidOverflowPolicy {
        class: CommunicationClass,
        expected: OverflowPolicy,
    },
    /// Latest-state channels have exactly one current-value slot.
    LatestStateCapacity,
    /// A resource charge was zero.
    ZeroCharge,
    /// A byte estimate ceiling exceeded the hard limit.
    ItemEstimateTooLarge { maximum: u64 },
    /// A charged channel was built without a budget.
    MissingBudget,
    /// A constructor was given the wrong communication class.
    WrongCommunicationClass {
        expected: CommunicationClass,
        actual: CommunicationClass,
    },
}

impl fmt::Display for ChannelConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidName(error) => error.fmt(formatter),
            Self::ZeroCapacity => formatter.write_str("channel capacity must be nonzero"),
            Self::CapacityTooLarge { maximum } => {
                write!(
                    formatter,
                    "channel capacity exceeds the {maximum}-slot limit"
                )
            }
            Self::InvalidOverflowPolicy { class, expected } => {
                write!(formatter, "{class:?} requires {expected:?} overflow policy")
            }
            Self::LatestStateCapacity => formatter.write_str("latest-state capacity must be one"),
            Self::ZeroCharge => formatter.write_str("resource charge must be nonzero"),
            Self::ItemEstimateTooLarge { maximum } => {
                write!(formatter, "item estimate exceeds the {maximum}-byte limit")
            }
            Self::MissingBudget => {
                formatter.write_str("charged channel requires a resource budget")
            }
            Self::WrongCommunicationClass { expected, actual } => {
                write!(
                    formatter,
                    "channel class is {actual:?}, expected {expected:?}"
                )
            }
        }
    }
}

impl std::error::Error for ChannelConfigError {}

#[derive(Debug, Default)]
struct ChannelCounters {
    queued: AtomicUsize,
    accepted: AtomicU64,
    rejected_full: AtomicU64,
    deadlines: AtomicU64,
    cancellations: AtomicU64,
    closed: AtomicU64,
    dropped: AtomicU64,
    resource_denied: AtomicU64,
}

fn increment(counter: &AtomicU64) {
    let _ = counter.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
        Some(value.saturating_add(1))
    });
}

fn decrement(counter: &AtomicUsize) {
    let _ = counter.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
        Some(value.saturating_sub(1))
    });
}

#[derive(Debug)]
struct ChannelState {
    spec: ChannelSpec,
    counters: ChannelCounters,
    receiver_closed: AtomicBool,
    next_version: AtomicU64,
}

impl ChannelState {
    fn new(spec: ChannelSpec) -> Result<Arc<Self>, ChannelConfigError> {
        if spec.charge.is_some() && spec.budget.is_none() {
            return Err(ChannelConfigError::MissingBudget);
        }
        Ok(Arc::new(Self {
            spec,
            counters: ChannelCounters::default(),
            receiver_closed: AtomicBool::new(false),
            next_version: AtomicU64::new(0),
        }))
    }

    fn snapshot(&self) -> ChannelSnapshot {
        ChannelSnapshot {
            name: self.spec.name.clone(),
            owner: self.spec.owner.clone(),
            class: self.spec.class,
            overflow: self.spec.overflow,
            capacity: self.spec.capacity,
            queued: self.counters.queued.load(Ordering::Relaxed),
            accepted: self.counters.accepted.load(Ordering::Relaxed),
            rejected_full: self.counters.rejected_full.load(Ordering::Relaxed),
            deadlines: self.counters.deadlines.load(Ordering::Relaxed),
            cancellations: self.counters.cancellations.load(Ordering::Relaxed),
            closed: self.counters.closed.load(Ordering::Relaxed),
            dropped: self.counters.dropped.load(Ordering::Relaxed),
            resource_denied: self.counters.resource_denied.load(Ordering::Relaxed),
        }
    }

    fn next_version(&self) -> u64 {
        let previous = self
            .next_version
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                Some(value.saturating_add(1))
            })
            .unwrap_or_else(|_| self.next_version.load(Ordering::Relaxed));
        previous.saturating_add(1)
    }

    fn admit(&self, estimate: Option<u64>) -> Result<Option<ResourceLease>, AdmissionError> {
        let Some(charge) = self.spec.charge else {
            if let Some(estimate) = estimate {
                if estimate > MAX_QUEUE_ITEM_BYTES {
                    return Err(AdmissionError::EstimateTooLarge {
                        estimate,
                        maximum: MAX_QUEUE_ITEM_BYTES,
                    });
                }
            }
            return Ok(None);
        };
        let amount = match charge {
            QueueCharge::PerItem { units, .. } => units,
            QueueCharge::PerBytes { maximum, .. } => {
                let Some(estimate) = estimate else {
                    return Err(AdmissionError::EstimateRequired);
                };
                if estimate == 0 || estimate > maximum {
                    return Err(AdmissionError::EstimateOutOfRange { estimate, maximum });
                }
                estimate
            }
        };
        let class = match charge {
            QueueCharge::PerItem { class, .. } | QueueCharge::PerBytes { class, .. } => class,
        };
        let budget = self
            .spec
            .budget
            .as_ref()
            .expect("validated charged channel");
        ResourceRequest::new(class, amount)
            .map_err(AdmissionError::Resource)
            .and_then(|request| {
                budget
                    .try_acquire(request)
                    .map(Some)
                    .map_err(AdmissionError::Resource)
            })
    }

    fn accepted(&self) {
        increment(&self.counters.accepted);
        self.counters.queued.fetch_add(1, Ordering::Relaxed);
    }

    fn queued_taken(&self) {
        decrement(&self.counters.queued);
    }

    fn queued_dropped(&self) {
        decrement(&self.counters.queued);
        increment(&self.counters.dropped);
    }
}

/// Privacy-safe, bounded channel counters and metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChannelSnapshot {
    /// Static channel identifier.
    pub name: ChannelName,
    /// Owning service identifier.
    pub owner: ServiceName,
    /// Communication taxonomy.
    pub class: CommunicationClass,
    /// Explicit overflow policy.
    pub overflow: OverflowPolicy,
    /// Configured queue capacity.
    pub capacity: usize,
    /// Current queued item count.
    pub queued: usize,
    /// Accepted item count.
    pub accepted: u64,
    /// Immediate full rejections.
    pub rejected_full: u64,
    /// Deadline expirations while reserving capacity.
    pub deadlines: u64,
    /// Cancellation outcomes while reserving capacity or receiving.
    pub cancellations: u64,
    /// Closed-channel outcomes.
    pub closed: u64,
    /// Items dropped during receiver shutdown.
    pub dropped: u64,
    /// Resource admission denials.
    pub resource_denied: u64,
}

#[derive(Debug)]
enum AdmissionError {
    EstimateRequired,
    EstimateTooLarge { estimate: u64, maximum: u64 },
    EstimateOutOfRange { estimate: u64, maximum: u64 },
    Resource(ResourceError),
}

/// Error returned by a bounded command or event send.
pub enum SendError<T> {
    /// The queue had no immediately available slot.
    Full(T),
    /// The supplied deadline elapsed before a slot opened.
    DeadlineElapsed(T),
    /// Cancellation was requested before acceptance.
    Cancelled(T),
    /// The receiver is closed.
    Closed(T),
    /// Resource admission denied before enqueue.
    ResourceDenied { payload: T, error: ResourceError },
    /// A byte-charged send did not provide an estimate.
    PayloadEstimateRequired(T),
    /// The caller-provided estimate exceeded the hard ceiling.
    PayloadEstimateTooLarge {
        payload: T,
        estimate: u64,
        maximum: u64,
    },
    /// The estimate was outside the charge's configured range.
    PayloadEstimateOutOfRange {
        payload: T,
        estimate: u64,
        maximum: u64,
    },
}

impl<T: fmt::Debug> fmt::Debug for SendError<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Full(_) => formatter.write_str("SendError::Full(..)"),
            Self::DeadlineElapsed(_) => formatter.write_str("SendError::DeadlineElapsed(..)"),
            Self::Cancelled(_) => formatter.write_str("SendError::Cancelled(..)"),
            Self::Closed(_) => formatter.write_str("SendError::Closed(..)"),
            Self::ResourceDenied { error, .. } => formatter
                .debug_struct("SendError::ResourceDenied")
                .field("error", error)
                .finish(),
            Self::PayloadEstimateRequired(_) => {
                formatter.write_str("SendError::PayloadEstimateRequired(..)")
            }
            Self::PayloadEstimateTooLarge {
                estimate, maximum, ..
            } => formatter
                .debug_struct("SendError::PayloadEstimateTooLarge")
                .field("estimate", estimate)
                .field("maximum", maximum)
                .finish(),
            Self::PayloadEstimateOutOfRange {
                estimate, maximum, ..
            } => formatter
                .debug_struct("SendError::PayloadEstimateOutOfRange")
                .field("estimate", estimate)
                .field("maximum", maximum)
                .finish(),
        }
    }
}

/// Error returned by a cancellation-aware receive.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReceiveError {
    /// Cancellation was requested before an item was received.
    Cancelled,
    /// The deadline elapsed before an item was received.
    DeadlineElapsed,
    /// All senders were dropped or the receiver was closed.
    Closed,
}

/// Error returned by a nonblocking receive.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TryReceiveError {
    /// No item is available yet.
    Empty,
    /// The channel is closed and empty.
    Closed,
}

struct Queued<T> {
    value: Option<T>,
    lease: Option<ResourceLease>,
    state: Arc<ChannelState>,
}

impl<T> Queued<T> {
    fn into_parts(mut self) -> (T, Option<ResourceLease>) {
        self.state.queued_taken();
        let value = self.value.take().expect("queued item contains a value");
        let lease = self.lease.take();
        (value, lease)
    }
}

impl<T> Drop for Queued<T> {
    fn drop(&mut self) {
        if self.value.is_some() {
            self.state.queued_dropped();
        }
    }
}

/// An item received from a charged queue. Dropping it releases its lease.
pub struct Received<T> {
    value: T,
    lease: Option<ResourceLease>,
}

impl<T> Received<T> {
    fn from_parts((value, lease): (T, Option<ResourceLease>)) -> Self {
        Self { value, lease }
    }

    /// Returns the payload and releases any charge after the move.
    pub fn into_inner(self) -> T {
        self.value
    }

    /// Whether this item holds a resource lease during processing.
    pub const fn is_charged(&self) -> bool {
        self.lease.is_some()
    }
}

impl<T> AsRef<T> for Received<T> {
    fn as_ref(&self) -> &T {
        &self.value
    }
}

impl<T> Deref for Received<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.as_ref()
    }
}

struct CommandSenderInner<T> {
    sender: mpsc::Sender<Queued<T>>,
    state: Arc<ChannelState>,
}

impl<T> Clone for CommandSenderInner<T> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
            state: Arc::clone(&self.state),
        }
    }
}

impl<T> CommandSenderInner<T> {
    fn try_send(&self, value: T, estimate: Option<u64>) -> Result<(), SendError<T>> {
        if self.state.receiver_closed.load(Ordering::Acquire) {
            increment(&self.state.counters.closed);
            return Err(SendError::Closed(value));
        }
        let permit = match self.sender.try_reserve() {
            Ok(permit) => permit,
            Err(mpsc::error::TrySendError::Full(())) => {
                increment(&self.state.counters.rejected_full);
                return Err(SendError::Full(value));
            }
            Err(mpsc::error::TrySendError::Closed(())) => {
                increment(&self.state.counters.closed);
                return Err(SendError::Closed(value));
            }
        };
        if self.state.receiver_closed.load(Ordering::Acquire) {
            drop(permit);
            increment(&self.state.counters.closed);
            return Err(SendError::Closed(value));
        }
        let lease = match self.state.admit(estimate) {
            Ok(lease) => lease,
            Err(error) => {
                increment(&self.state.counters.resource_denied);
                return Err(map_admission_error(value, error));
            }
        };
        self.state.accepted();
        permit.send(Queued {
            value: Some(value),
            lease,
            state: Arc::clone(&self.state),
        });
        Ok(())
    }

    async fn send_until(
        &self,
        value: T,
        estimate: Option<u64>,
        deadline: tokio::time::Instant,
        cancellation: &CancellationToken,
    ) -> Result<(), SendError<T>> {
        if cancellation.is_cancelled() {
            increment(&self.state.counters.cancellations);
            return Err(SendError::Cancelled(value));
        }
        if self.state.receiver_closed.load(Ordering::Acquire) {
            increment(&self.state.counters.closed);
            return Err(SendError::Closed(value));
        }
        let sender = self.sender.clone();
        let permit = tokio::select! {
            biased;
            _ = cancellation.cancelled() => {
                increment(&self.state.counters.cancellations);
                return Err(SendError::Cancelled(value));
            }
            _ = tokio::time::sleep_until(deadline) => {
                increment(&self.state.counters.deadlines);
                return Err(SendError::DeadlineElapsed(value));
            }
            result = sender.reserve_owned() => match result {
                Ok(permit) => permit,
                Err(_) => {
                    increment(&self.state.counters.closed);
                    return Err(SendError::Closed(value));
                }
            }
        };
        if self.state.receiver_closed.load(Ordering::Acquire) {
            drop(permit);
            increment(&self.state.counters.closed);
            return Err(SendError::Closed(value));
        }
        let lease = match self.state.admit(estimate) {
            Ok(lease) => lease,
            Err(error) => {
                increment(&self.state.counters.resource_denied);
                return Err(map_admission_error(value, error));
            }
        };
        self.state.accepted();
        permit.send(Queued {
            value: Some(value),
            lease,
            state: Arc::clone(&self.state),
        });
        Ok(())
    }
}

fn map_admission_error<T>(value: T, error: AdmissionError) -> SendError<T> {
    match error {
        AdmissionError::EstimateRequired => SendError::PayloadEstimateRequired(value),
        AdmissionError::EstimateTooLarge { estimate, maximum }
        | AdmissionError::EstimateOutOfRange { estimate, maximum } => {
            if estimate > maximum {
                SendError::PayloadEstimateTooLarge {
                    payload: value,
                    estimate,
                    maximum,
                }
            } else {
                SendError::PayloadEstimateOutOfRange {
                    payload: value,
                    estimate,
                    maximum,
                }
            }
        }
        AdmissionError::Resource(error) => SendError::ResourceDenied {
            payload: value,
            error,
        },
    }
}

fn build_mpsc<T>(
    spec: ChannelSpec,
    expected: CommunicationClass,
) -> Result<(CommandSenderInner<T>, mpsc::Receiver<Queued<T>>), ChannelConfigError> {
    if spec.class != expected {
        return Err(ChannelConfigError::WrongCommunicationClass {
            expected,
            actual: spec.class,
        });
    }
    let state = ChannelState::new(spec.clone())?;
    let (sender, receiver) = mpsc::channel(spec.capacity);
    Ok((CommandSenderInner { sender, state }, receiver))
}

/// Bounded ordered point-to-point command sender.
#[derive(Clone)]
pub struct CommandSender<T> {
    inner: CommandSenderInner<T>,
}

impl<T> fmt::Debug for CommandSender<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CommandSender")
            .field("snapshot", &self.inner.state.snapshot())
            .finish()
    }
}

/// Bounded command receiver.
pub struct CommandReceiver<T> {
    receiver: mpsc::Receiver<Queued<T>>,
    state: Arc<ChannelState>,
}

impl<T> fmt::Debug for CommandReceiver<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CommandReceiver")
            .field("snapshot", &self.state.snapshot())
            .finish()
    }
}

/// Creates a bounded command channel.
pub fn command_channel<T>(
    spec: ChannelSpec,
) -> Result<(CommandSender<T>, CommandReceiver<T>), ChannelConfigError> {
    let (inner, receiver) = build_mpsc(spec, CommunicationClass::Command)?;
    let state = Arc::clone(&inner.state);
    Ok((CommandSender { inner }, CommandReceiver { receiver, state }))
}

impl<T> CommandSender<T> {
    /// Attempts an immediate send without waiting.
    pub fn try_send(&self, value: T) -> Result<(), SendError<T>> {
        self.inner.try_send(value, None)
    }

    /// Attempts an immediate send using a caller-provided byte estimate.
    pub fn try_send_with_bytes(&self, value: T, estimate: u64) -> Result<(), SendError<T>> {
        self.inner.try_send(value, Some(estimate))
    }

    /// Waits for capacity until the deadline or cancellation is observed.
    pub async fn send_until(
        &self,
        value: T,
        deadline: tokio::time::Instant,
        cancellation: &CancellationToken,
    ) -> Result<(), SendError<T>> {
        self.inner
            .send_until(value, None, deadline, cancellation)
            .await
    }

    /// Waits for capacity using a caller-provided byte estimate.
    pub async fn send_until_with_bytes(
        &self,
        value: T,
        estimate: u64,
        deadline: tokio::time::Instant,
        cancellation: &CancellationToken,
    ) -> Result<(), SendError<T>> {
        self.inner
            .send_until(value, Some(estimate), deadline, cancellation)
            .await
    }

    /// Returns privacy-safe metadata and counters.
    pub fn snapshot(&self) -> ChannelSnapshot {
        self.inner.state.snapshot()
    }
}

impl<T> CommandReceiver<T> {
    /// Receives the next item, waking promptly on cancellation or closure.
    pub async fn recv(
        &mut self,
        cancellation: &CancellationToken,
    ) -> Result<Received<T>, ReceiveError> {
        if cancellation.is_cancelled() {
            increment(&self.state.counters.cancellations);
            return Err(ReceiveError::Cancelled);
        }
        tokio::select! {
            biased;
            _ = cancellation.cancelled() => {
                increment(&self.state.counters.cancellations);
                Err(ReceiveError::Cancelled)
            }
            item = self.receiver.recv() => match item {
                Some(item) => Ok(Received::from_parts(item.into_parts())),
                None => {
                    increment(&self.state.counters.closed);
                    Err(ReceiveError::Closed)
                }
            }
        }
    }

    /// Receives the next item until a deadline, cancellation, or closure.
    pub async fn recv_until(
        &mut self,
        deadline: tokio::time::Instant,
        cancellation: &CancellationToken,
    ) -> Result<Received<T>, ReceiveError> {
        if cancellation.is_cancelled() {
            increment(&self.state.counters.cancellations);
            return Err(ReceiveError::Cancelled);
        }
        tokio::select! {
            biased;
            _ = cancellation.cancelled() => {
                increment(&self.state.counters.cancellations);
                Err(ReceiveError::Cancelled)
            }
            _ = tokio::time::sleep_until(deadline) => {
                increment(&self.state.counters.deadlines);
                Err(ReceiveError::DeadlineElapsed)
            }
            item = self.receiver.recv() => match item {
                Some(item) => Ok(Received::from_parts(item.into_parts())),
                None => {
                    increment(&self.state.counters.closed);
                    Err(ReceiveError::Closed)
                }
            }
        }
    }

    /// Attempts a nonblocking receive.
    pub fn try_recv(&mut self) -> Result<Received<T>, TryReceiveError> {
        match self.receiver.try_recv() {
            Ok(item) => Ok(Received::from_parts(item.into_parts())),
            Err(mpsc::error::TryRecvError::Empty) => Err(TryReceiveError::Empty),
            Err(mpsc::error::TryRecvError::Disconnected) => {
                increment(&self.state.counters.closed);
                Err(TryReceiveError::Closed)
            }
        }
    }

    /// Stops new sends while retaining queued items for deterministic draining.
    pub fn close(&mut self) {
        self.state.receiver_closed.store(true, Ordering::Release);
        self.receiver.close();
    }

    /// Returns privacy-safe metadata and counters.
    pub fn snapshot(&self) -> ChannelSnapshot {
        self.state.snapshot()
    }
}

impl<T> Drop for CommandReceiver<T> {
    fn drop(&mut self) {
        self.state.receiver_closed.store(true, Ordering::Release);
    }
}

/// Bounded single-consumer event sender. Full queues use drop-newest policy.
#[derive(Clone)]
pub struct EventSender<T> {
    inner: CommandSenderInner<T>,
}

/// Bounded event receiver.
pub struct EventReceiver<T> {
    receiver: CommandReceiver<T>,
}

/// Event send errors are the same typed outcomes as command sends.
pub type EventSendError<T> = SendError<T>;

/// Creates a bounded, single-consumer, drop-newest event stream.
pub fn event_channel<T>(
    spec: ChannelSpec,
) -> Result<(EventSender<T>, EventReceiver<T>), ChannelConfigError> {
    let (inner, receiver) = build_mpsc(spec, CommunicationClass::Event)?;
    let state = Arc::clone(&inner.state);
    Ok((
        EventSender { inner },
        EventReceiver {
            receiver: CommandReceiver { receiver, state },
        },
    ))
}

impl<T> EventSender<T> {
    /// Delivers an event immediately or reports a counted drop/closure.
    pub fn try_send(&self, value: T) -> Result<(), EventSendError<T>> {
        self.inner.try_send(value, None)
    }

    /// Delivers an event with a caller-provided byte estimate.
    pub fn try_send_with_bytes(&self, value: T, estimate: u64) -> Result<(), EventSendError<T>> {
        self.inner.try_send(value, Some(estimate))
    }

    /// Returns privacy-safe metadata and counters.
    pub fn snapshot(&self) -> ChannelSnapshot {
        self.inner.state.snapshot()
    }
}

impl<T> EventReceiver<T> {
    /// Receives an event with cancellation-aware waiting.
    pub async fn recv(
        &mut self,
        cancellation: &CancellationToken,
    ) -> Result<Received<T>, ReceiveError> {
        self.receiver.recv(cancellation).await
    }

    /// Receives an event until a deadline, cancellation, or closure.
    pub async fn recv_until(
        &mut self,
        deadline: tokio::time::Instant,
        cancellation: &CancellationToken,
    ) -> Result<Received<T>, ReceiveError> {
        self.receiver.recv_until(deadline, cancellation).await
    }

    /// Attempts a nonblocking receive.
    pub fn try_recv(&mut self) -> Result<Received<T>, TryReceiveError> {
        self.receiver.try_recv()
    }

    /// Stops new sends while retaining queued events for draining.
    pub fn close(&mut self) {
        self.receiver.close();
    }

    /// Returns privacy-safe metadata and counters.
    pub fn snapshot(&self) -> ChannelSnapshot {
        self.receiver.snapshot()
    }
}

struct RequestEnvelope<Req, Resp> {
    request: Option<Req>,
    response: Option<oneshot::Sender<Resp>>,
}

/// Error returned by a request enqueue or its bounded response path.
pub enum RequestError<T> {
    /// The request queue was full before acceptance.
    Full(T),
    /// The request deadline elapsed before enqueue.
    DeadlineElapsed(T),
    /// Cancellation was requested before enqueue.
    Cancelled(T),
    /// The request receiver is closed.
    Closed(T),
    /// Resource admission denied before enqueue.
    ResourceDenied { request: T, error: ResourceError },
    /// A byte-charged request did not provide an estimate.
    PayloadEstimateRequired(T),
    /// The caller-provided estimate exceeded the hard ceiling.
    PayloadEstimateTooLarge {
        request: T,
        estimate: u64,
        maximum: u64,
    },
    /// The estimate was outside the charge's configured range.
    PayloadEstimateOutOfRange {
        request: T,
        estimate: u64,
        maximum: u64,
    },
    /// The service dropped the response sender.
    ResponseClosed,
    /// Cancellation occurred while awaiting the response.
    ResponseCancelled,
    /// The response deadline elapsed.
    ResponseDeadlineElapsed,
}

impl<T: fmt::Debug> fmt::Debug for RequestError<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Full(_) => formatter.write_str("RequestError::Full(..)"),
            Self::DeadlineElapsed(_) => formatter.write_str("RequestError::DeadlineElapsed(..)"),
            Self::Cancelled(_) => formatter.write_str("RequestError::Cancelled(..)"),
            Self::Closed(_) => formatter.write_str("RequestError::Closed(..)"),
            Self::ResourceDenied { error, .. } => formatter
                .debug_struct("RequestError::ResourceDenied")
                .field("error", error)
                .finish(),
            Self::PayloadEstimateRequired(_) => {
                formatter.write_str("RequestError::PayloadEstimateRequired(..)")
            }
            Self::PayloadEstimateTooLarge {
                estimate, maximum, ..
            } => formatter
                .debug_struct("RequestError::PayloadEstimateTooLarge")
                .field("estimate", estimate)
                .field("maximum", maximum)
                .finish(),
            Self::PayloadEstimateOutOfRange {
                estimate, maximum, ..
            } => formatter
                .debug_struct("RequestError::PayloadEstimateOutOfRange")
                .field("estimate", estimate)
                .field("maximum", maximum)
                .finish(),
            Self::ResponseClosed => formatter.write_str("RequestError::ResponseClosed"),
            Self::ResponseCancelled => formatter.write_str("RequestError::ResponseCancelled"),
            Self::ResponseDeadlineElapsed => {
                formatter.write_str("RequestError::ResponseDeadlineElapsed")
            }
        }
    }
}

fn map_request_send_error<Req, Resp>(
    error: SendError<RequestEnvelope<Req, Resp>>,
) -> RequestError<Req> {
    match error {
        SendError::Full(mut envelope) => {
            RequestError::Full(envelope.request.take().expect("request retained"))
        }
        SendError::DeadlineElapsed(mut envelope) => {
            RequestError::DeadlineElapsed(envelope.request.take().expect("request retained"))
        }
        SendError::Cancelled(mut envelope) => {
            RequestError::Cancelled(envelope.request.take().expect("request retained"))
        }
        SendError::Closed(mut envelope) => {
            RequestError::Closed(envelope.request.take().expect("request retained"))
        }
        SendError::ResourceDenied { mut payload, error } => RequestError::ResourceDenied {
            request: payload.request.take().expect("request retained"),
            error,
        },
        SendError::PayloadEstimateRequired(mut envelope) => RequestError::PayloadEstimateRequired(
            envelope.request.take().expect("request retained"),
        ),
        SendError::PayloadEstimateTooLarge {
            mut payload,
            estimate,
            maximum,
        } => RequestError::PayloadEstimateTooLarge {
            request: payload.request.take().expect("request retained"),
            estimate,
            maximum,
        },
        SendError::PayloadEstimateOutOfRange {
            mut payload,
            estimate,
            maximum,
        } => RequestError::PayloadEstimateOutOfRange {
            request: payload.request.take().expect("request retained"),
            estimate,
            maximum,
        },
    }
}

/// Bounded request sender with one response waiter per accepted request.
#[derive(Clone)]
pub struct RequestSender<Req, Resp> {
    inner: CommandSenderInner<RequestEnvelope<Req, Resp>>,
}

/// Request receiver; each received item owns its response sender and charge.
pub struct RequestReceiver<Req, Resp> {
    receiver: CommandReceiver<RequestEnvelope<Req, Resp>>,
}

/// The two owned halves returned by [`request_channel`].
pub type RequestChannelParts<Req, Resp> = (RequestSender<Req, Resp>, RequestReceiver<Req, Resp>);

/// A received request with an explicit one-shot response path.
pub struct ReceivedRequest<Req, Resp> {
    request: Option<Req>,
    response: Option<oneshot::Sender<Resp>>,
    lease: Option<ResourceLease>,
}

impl<Req, Resp> ReceivedRequest<Req, Resp> {
    /// Returns the request by reference.
    pub fn request(&self) -> &Req {
        self.request.as_ref().expect("received request retained")
    }

    /// Sends the one response, or returns the response when the requester is gone.
    pub fn respond(mut self, response: Resp) -> Result<(), Resp> {
        self.response
            .take()
            .expect("response sender retained")
            .send(response)
    }

    /// Whether this request still holds its queue resource charge.
    pub const fn is_charged(&self) -> bool {
        self.lease.is_some()
    }

    /// Takes ownership of the request while dropping the response path.
    pub fn into_request(mut self) -> Req {
        self.request.take().expect("received request retained")
    }
}

/// Creates a bounded request/response channel.
pub fn request_channel<Req, Resp>(
    spec: ChannelSpec,
) -> Result<RequestChannelParts<Req, Resp>, ChannelConfigError> {
    let (inner, receiver) = build_mpsc(spec, CommunicationClass::Request)?;
    let state = Arc::clone(&inner.state);
    Ok((
        RequestSender { inner },
        RequestReceiver {
            receiver: CommandReceiver { receiver, state },
        },
    ))
}

impl<Req, Resp> RequestSender<Req, Resp> {
    async fn send_request_inner(
        &self,
        request: Req,
        estimate: Option<u64>,
        deadline: tokio::time::Instant,
        cancellation: &CancellationToken,
    ) -> Result<Resp, RequestError<Req>> {
        let (sender, response) = oneshot::channel();
        let envelope = RequestEnvelope {
            request: Some(request),
            response: Some(sender),
        };
        if let Err(error) = self
            .inner
            .send_until(envelope, estimate, deadline, cancellation)
            .await
        {
            return Err(map_request_send_error(error));
        }
        tokio::select! {
            biased;
            _ = cancellation.cancelled() => {
                increment(&self.inner.state.counters.cancellations);
                Err(RequestError::ResponseCancelled)
            }
            _ = tokio::time::sleep_until(deadline) => {
                increment(&self.inner.state.counters.deadlines);
                Err(RequestError::ResponseDeadlineElapsed)
            }
            response = response => match response {
                Ok(response) => Ok(response),
                Err(_) => {
                    increment(&self.inner.state.counters.closed);
                    Err(RequestError::ResponseClosed)
                }
            }
        }
    }

    /// Enqueues a request and awaits one response under the same scope.
    pub async fn send_request(
        &self,
        request: Req,
        deadline: tokio::time::Instant,
        cancellation: &CancellationToken,
    ) -> Result<Resp, RequestError<Req>> {
        self.send_request_inner(request, None, deadline, cancellation)
            .await
    }

    /// Enqueues a request with a caller-provided byte estimate.
    pub async fn send_request_with_bytes(
        &self,
        request: Req,
        estimate: u64,
        deadline: tokio::time::Instant,
        cancellation: &CancellationToken,
    ) -> Result<Resp, RequestError<Req>> {
        self.send_request_inner(request, Some(estimate), deadline, cancellation)
            .await
    }

    /// Returns privacy-safe metadata and counters.
    pub fn snapshot(&self) -> ChannelSnapshot {
        self.inner.state.snapshot()
    }
}

impl<Req, Resp> RequestReceiver<Req, Resp> {
    async fn recv_inner(
        &mut self,
        deadline: Option<tokio::time::Instant>,
        cancellation: &CancellationToken,
    ) -> Result<ReceivedRequest<Req, Resp>, ReceiveError> {
        let envelope = match deadline {
            Some(deadline) => self.receiver.recv_until(deadline, cancellation).await?,
            None => self.receiver.recv(cancellation).await?,
        };
        let Received {
            value: envelope,
            lease,
        } = envelope;
        Ok(ReceivedRequest {
            request: envelope.request,
            response: envelope.response,
            lease,
        })
    }

    /// Receives a request with cancellation-aware waiting.
    pub async fn recv(
        &mut self,
        cancellation: &CancellationToken,
    ) -> Result<ReceivedRequest<Req, Resp>, ReceiveError> {
        self.recv_inner(None, cancellation).await
    }

    /// Receives a request until a deadline, cancellation, or closure.
    pub async fn recv_until(
        &mut self,
        deadline: tokio::time::Instant,
        cancellation: &CancellationToken,
    ) -> Result<ReceivedRequest<Req, Resp>, ReceiveError> {
        self.recv_inner(Some(deadline), cancellation).await
    }

    /// Stops new sends while retaining queued requests for draining.
    pub fn close(&mut self) {
        self.receiver.close();
    }

    /// Returns privacy-safe metadata and counters.
    pub fn snapshot(&self) -> ChannelSnapshot {
        self.receiver.snapshot()
    }
}

/// A latest-state value with an explicit monotonic version.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LatestState<T> {
    version: u64,
    value: Option<T>,
}

impl<T> LatestState<T> {
    /// Current version, starting at zero for the initial absence.
    pub const fn version(&self) -> u64 {
        self.version
    }

    /// Current value, if one has been published.
    pub fn value(&self) -> Option<&T> {
        self.value.as_ref()
    }

    /// Takes the current value.
    pub fn into_value(self) -> Option<T> {
        self.value
    }
}

/// Error returned when a latest-state update has no receivers.
pub enum StateUpdateError<T> {
    /// The value remains available to the caller.
    Closed(T),
}

impl<T: fmt::Debug> fmt::Debug for StateUpdateError<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("StateUpdateError::Closed(..)")
    }
}

/// Latest-state publisher; updates coalesce by design.
#[derive(Clone)]
pub struct LatestStateSender<T> {
    sender: watch::Sender<LatestState<T>>,
    state: Arc<ChannelState>,
}

/// Latest-state subscriber.
pub struct LatestStateReceiver<T> {
    receiver: watch::Receiver<LatestState<T>>,
    state: Arc<ChannelState>,
}

/// Creates a latest-state channel with initial absence and version zero.
pub fn latest_state_channel<T: Clone>(
    spec: ChannelSpec,
) -> Result<(LatestStateSender<T>, LatestStateReceiver<T>), ChannelConfigError> {
    if spec.class != CommunicationClass::LatestState {
        return Err(ChannelConfigError::WrongCommunicationClass {
            expected: CommunicationClass::LatestState,
            actual: spec.class,
        });
    }
    if spec.charge.is_some() {
        return Err(ChannelConfigError::InvalidOverflowPolicy {
            class: CommunicationClass::LatestState,
            expected: OverflowPolicy::CoalesceLatest,
        });
    }
    let state = ChannelState::new(spec)?;
    let (sender, receiver) = watch::channel(LatestState {
        version: 0,
        value: None,
    });
    Ok((
        LatestStateSender {
            sender,
            state: Arc::clone(&state),
        },
        LatestStateReceiver { receiver, state },
    ))
}

impl<T: Clone> LatestStateSender<T> {
    /// Publishes a new value and returns its version.
    pub fn set(&self, value: T) -> Result<u64, StateUpdateError<T>> {
        let version = self.state.next_version();
        match self.sender.send(LatestState {
            version,
            value: Some(value),
        }) {
            Ok(()) => {
                increment(&self.state.counters.accepted);
                Ok(version)
            }
            Err(error) => {
                increment(&self.state.counters.closed);
                Err(StateUpdateError::Closed(
                    error.0.into_value().expect("value retained"),
                ))
            }
        }
    }

    /// Clears the current value and advances the version.
    pub fn clear(&self) -> Result<u64, StateUpdateError<()>> {
        let version = self.state.next_version();
        match self.sender.send(LatestState {
            version,
            value: None,
        }) {
            Ok(()) => {
                increment(&self.state.counters.accepted);
                Ok(version)
            }
            Err(_) => {
                increment(&self.state.counters.closed);
                Err(StateUpdateError::Closed(()))
            }
        }
    }

    /// Returns privacy-safe metadata and counters.
    pub fn snapshot(&self) -> ChannelSnapshot {
        self.state.snapshot()
    }
}

impl<T: Clone> LatestStateReceiver<T> {
    /// Returns the current value, including initial absence and version zero.
    pub fn current(&self) -> LatestState<T> {
        self.receiver.borrow().clone()
    }

    /// Waits for the newest version, cancellation, or sender closure.
    pub async fn changed(
        &mut self,
        cancellation: &CancellationToken,
    ) -> Result<LatestState<T>, ReceiveError> {
        if cancellation.is_cancelled() {
            increment(&self.state.counters.cancellations);
            return Err(ReceiveError::Cancelled);
        }
        tokio::select! {
            biased;
            _ = cancellation.cancelled() => {
                increment(&self.state.counters.cancellations);
                Err(ReceiveError::Cancelled)
            }
            changed = self.receiver.changed() => match changed {
                Ok(()) => Ok(self.receiver.borrow().clone()),
                Err(_) => {
                    increment(&self.state.counters.closed);
                    Err(ReceiveError::Closed)
                }
            }
        }
    }

    /// Returns privacy-safe metadata and counters.
    pub fn snapshot(&self) -> ChannelSnapshot {
        self.state.snapshot()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use i2pr_core::ResourceLimit;
    use std::time::Duration;

    fn owner() -> ServiceName {
        ServiceName::new("test-service").expect("valid owner")
    }

    fn budget(class: ResourceClass, maximum: u64) -> ResourceBudget {
        ResourceBudget::new([ResourceLimit::new(class, maximum).expect("limit")]).expect("budget")
    }

    fn deadline() -> tokio::time::Instant {
        tokio::time::Instant::now() + Duration::from_secs(30)
    }

    #[tokio::test(start_paused = true)]
    async fn commands_are_ordered_and_resource_charged_until_processing_finishes() {
        let budget = budget(ResourceClass::CommandQueueItems, 2);
        let spec = ChannelSpec::command("commands", owner(), 2)
            .expect("spec")
            .with_item_charge(ResourceClass::CommandQueueItems, 1)
            .expect("charge")
            .with_budget(budget.clone());
        let (sender, mut receiver) = command_channel::<u8>(spec).expect("channel");
        let cancellation = CancellationToken::new();
        sender.try_send(1).expect("first");
        sender.try_send(2).expect("second");
        assert!(matches!(sender.try_send(3), Err(SendError::Full(3))));
        assert_eq!(
            budget
                .usage(ResourceClass::CommandQueueItems)
                .expect("usage")
                .used,
            2
        );
        assert_eq!(
            receiver
                .recv(&cancellation)
                .await
                .expect("first")
                .into_inner(),
            1
        );
        assert_eq!(
            receiver
                .recv(&cancellation)
                .await
                .expect("second")
                .into_inner(),
            2
        );
        assert_eq!(
            budget
                .usage(ResourceClass::CommandQueueItems)
                .expect("usage")
                .used,
            0
        );
        assert_eq!(sender.snapshot().queued, 0);
    }

    #[tokio::test(start_paused = true)]
    async fn send_deadline_and_cancellation_are_typed() {
        let spec = ChannelSpec::command("commands", owner(), 1).expect("spec");
        let (sender, mut receiver) = command_channel::<u8>(spec).expect("channel");
        let cancellation = CancellationToken::new();
        sender.try_send(1).expect("fill");
        let task = tokio::spawn({
            let sender = sender.clone();
            let cancellation = cancellation.clone();
            async move { sender.send_until(2, deadline(), &cancellation).await }
        });
        tokio::task::yield_now().await;
        cancellation.cancel(i2pr_core::CancellationReason::TestHarnessTeardown);
        assert!(matches!(
            task.await.expect("join"),
            Err(SendError::Cancelled(2))
        ));
        let _ = receiver
            .recv(&CancellationToken::new())
            .await
            .expect("fill");
    }

    #[tokio::test(start_paused = true)]
    async fn send_deadline_expires_without_retaining_a_queue_charge() {
        let budget = budget(ResourceClass::CommandQueueItems, 1);
        let spec = ChannelSpec::command("commands", owner(), 1)
            .expect("spec")
            .with_item_charge(ResourceClass::CommandQueueItems, 1)
            .expect("charge")
            .with_budget(budget.clone());
        let (sender, _receiver) = command_channel::<u8>(spec).expect("channel");
        let cancellation = CancellationToken::new();
        sender.try_send(1).expect("fill");
        let task = tokio::spawn({
            let sender = sender.clone();
            let cancellation = cancellation.clone();
            async move {
                sender
                    .send_until(
                        2,
                        tokio::time::Instant::now() + Duration::from_secs(5),
                        &cancellation,
                    )
                    .await
            }
        });
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_secs(5)).await;
        assert!(matches!(
            task.await.expect("join"),
            Err(SendError::DeadlineElapsed(2))
        ));
        assert_eq!(
            budget
                .usage(ResourceClass::CommandQueueItems)
                .expect("usage")
                .used,
            1
        );
    }

    #[tokio::test(start_paused = true)]
    async fn dropped_receiver_releases_queued_charge() {
        let budget = budget(ResourceClass::EventQueueItems, 2);
        let spec = ChannelSpec::event("events", owner(), 2)
            .expect("spec")
            .with_item_charge(ResourceClass::EventQueueItems, 1)
            .expect("charge")
            .with_budget(budget.clone());
        let (sender, receiver) = event_channel::<u8>(spec).expect("channel");
        sender.try_send(1).expect("first");
        sender.try_send(2).expect("second");
        drop(receiver);
        assert_eq!(
            budget
                .usage(ResourceClass::EventQueueItems)
                .expect("usage")
                .used,
            0
        );
        assert_eq!(sender.snapshot().dropped, 2);
    }

    #[test]
    fn event_full_uses_explicit_drop_newest_counter() {
        let spec = ChannelSpec::event("events", owner(), 1).expect("spec");
        let (sender, _receiver) = event_channel::<u8>(spec).expect("channel");
        sender.try_send(1).expect("first");
        assert!(matches!(sender.try_send(2), Err(SendError::Full(2))));
        assert_eq!(sender.snapshot().rejected_full, 1);
    }

    #[test]
    fn resource_denial_retains_payload_and_does_not_enter_queue() {
        let budget = budget(ResourceClass::CommandQueueItems, 1);
        let spec = ChannelSpec::command("commands", owner(), 2)
            .expect("spec")
            .with_item_charge(ResourceClass::CommandQueueItems, 1)
            .expect("charge")
            .with_budget(budget.clone());
        let (sender, _receiver) = command_channel::<u8>(spec).expect("channel");
        sender.try_send(1).expect("first");
        assert!(matches!(
            sender.try_send(2),
            Err(SendError::ResourceDenied {
                payload: 2,
                error: i2pr_core::ResourceError::Exhausted { .. }
            })
        ));
        assert_eq!(sender.snapshot().queued, 1);
        assert_eq!(sender.snapshot().resource_denied, 1);
    }

    #[tokio::test(start_paused = true)]
    async fn requests_do_not_leave_response_waiters() {
        let spec = ChannelSpec::request("requests", owner(), 1).expect("spec");
        let (sender, mut receiver) = request_channel::<u8, u8>(spec).expect("channel");
        let cancellation = CancellationToken::new();
        let task = tokio::spawn({
            let sender = sender.clone();
            let cancellation = cancellation.clone();
            async move { sender.send_request(7, deadline(), &cancellation).await }
        });
        let request = receiver
            .recv(&CancellationToken::new())
            .await
            .expect("request");
        assert_eq!(*request.request(), 7);
        request.respond(8).expect("response");
        assert_eq!(task.await.expect("join").expect("result"), 8);
        drop(receiver);
        assert_eq!(sender.snapshot().queued, 0);
    }

    #[tokio::test(start_paused = true)]
    async fn dropped_response_sender_is_a_typed_request_failure() {
        let spec = ChannelSpec::request("requests", owner(), 1).expect("spec");
        let (sender, mut receiver) = request_channel::<u8, u8>(spec).expect("spec");
        let cancellation = CancellationToken::new();
        let task = tokio::spawn({
            let sender = sender.clone();
            let cancellation = cancellation.clone();
            async move { sender.send_request(7, deadline(), &cancellation).await }
        });
        let request = receiver
            .recv(&CancellationToken::new())
            .await
            .expect("request");
        drop(request);
        assert!(matches!(
            task.await.expect("join"),
            Err(RequestError::ResponseClosed)
        ));
    }

    #[tokio::test(start_paused = true)]
    async fn latest_state_reports_initial_absence_and_versions() {
        let spec = ChannelSpec::latest_state("health", owner()).expect("spec");
        let (sender, mut receiver) = latest_state_channel::<u8>(spec).expect("channel");
        assert_eq!(receiver.current().version(), 0);
        assert!(receiver.current().value().is_none());
        sender.set(4).expect("publish");
        let current = receiver
            .changed(&CancellationToken::new())
            .await
            .expect("change");
        assert_eq!(current.version(), 1);
        assert_eq!(current.value(), Some(&4));
    }

    #[tokio::test(start_paused = true)]
    async fn synthetic_overload_graph_drains_and_shuts_down_without_usage_or_tasks() {
        use crate::{ServiceClassification, ServiceGraph, ServiceResult, ServiceSpec, Supervisor};
        use std::sync::Mutex;

        let budget = budget(ResourceClass::CommandQueueItems, 1);
        let spec = ChannelSpec::command("synthetic.commands", owner(), 1)
            .expect("spec")
            .with_item_charge(ResourceClass::CommandQueueItems, 1)
            .expect("charge")
            .with_budget(budget.clone());
        let (sender, receiver) = command_channel::<u8>(spec).expect("channel");
        let receiver = Arc::new(Mutex::new(Some(receiver)));
        let producer_name = ServiceName::new("producer").expect("name");
        let worker_name = ServiceName::new("worker").expect("name");

        let producer_sender = sender.clone();
        let producer = ServiceSpec::new(
            producer_name.clone(),
            ServiceClassification::Essential,
            move |context| {
                let producer_sender = producer_sender.clone();
                async move {
                    producer_sender.try_send(1).expect("first item accepted");
                    assert!(matches!(
                        producer_sender.try_send(2),
                        Err(SendError::Full(2))
                    ));
                    context.signal_ready().expect("producer ready");
                    context.cancellation().cancelled().await;
                    ServiceResult::RequestedShutdown
                }
            },
        );

        let worker_receiver = Arc::clone(&receiver);
        let worker = ServiceSpec::new(
            worker_name.clone(),
            ServiceClassification::Essential,
            move |context| {
                let worker_receiver = Arc::clone(&worker_receiver);
                async move {
                    let mut receiver = worker_receiver
                        .lock()
                        .expect("receiver lock")
                        .take()
                        .expect("worker owns receiver");
                    context.signal_ready().expect("worker ready");
                    loop {
                        match receiver.recv(context.cancellation()).await {
                            Ok(item) => drop(item),
                            Err(ReceiveError::Cancelled | ReceiveError::Closed) => {
                                return ServiceResult::RequestedShutdown;
                            }
                            Err(ReceiveError::DeadlineElapsed) => {
                                return ServiceResult::Failed(i2pr_core::ServiceFailure::new(
                                    i2pr_core::ServiceFailureCategory::InvalidState,
                                    None,
                                ));
                            }
                        }
                    }
                }
            },
        )
        .depends_on(producer_name);

        let mut builder = ServiceGraph::builder(2).expect("graph bound");
        builder.register(producer).expect("producer registration");
        builder.register(worker).expect("worker registration");
        let graph = builder.build().expect("graph");
        let supervisor = Supervisor::new(graph, Duration::from_secs(5)).expect("supervisor");
        let handle = supervisor.handle();
        let task = tokio::spawn(supervisor.run());
        tokio::task::yield_now().await;
        assert!(handle.shutdown(i2pr_core::ShutdownReason::Test));
        let report = task
            .await
            .expect("supervisor join")
            .expect("graceful report");
        assert_eq!(report.remaining_tasks(), 0);
        assert_eq!(sender.snapshot().queued, 0);
        assert_eq!(
            budget
                .usage(ResourceClass::CommandQueueItems)
                .expect("usage")
                .used,
            0
        );
    }

    #[test]
    fn invalid_channel_metadata_is_rejected() {
        assert!(ChannelSpec::command("", owner(), 1).is_err());
        assert!(ChannelSpec::command("commands", owner(), 0).is_err());
        assert!(ChannelSpec::command("commands", owner(), MAX_CHANNEL_CAPACITY + 1).is_err());
        assert!(ChannelSpec::command("bad/name", owner(), 1).is_err());
    }
}
