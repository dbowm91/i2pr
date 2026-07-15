use std::collections::{BTreeMap, VecDeque};
use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use i2pr_core::{
    ResourceBudget, ResourceClass, ResourceLease, ResourceLimit, ResourceRequest, ResourceUsage,
};
use i2pr_runtime::CancellationToken;
use tokio::sync::Notify;

use crate::clock::{ClockError, Deadline, ManualClock, ManualInstant};
use crate::faults::{FaultScript, FaultTerminal, FaultUnitKind, LinkDirection, LinkId};
use crate::rng::ReproducibilitySeed;

/// Maximum synthetic identifier length retained by the network model.
pub const MAX_LINK_ID: usize = 64;
/// Maximum datagram payload accepted by the default model.
pub const MAX_DATAGRAM_SIZE: usize = 65_535;
const MAX_SCHEDULER_PENDING: usize = 4_096;
const MAX_SCHEDULER_BYTES: usize = 1 << 20;
const MAX_LINKS: usize = 1_024;

/// Bounded scheduler limits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SchedulerConfig {
    /// Maximum scheduled delivery units.
    pub max_pending_deliveries: usize,
    /// Maximum bytes retained by scheduled units.
    pub max_buffered_bytes: usize,
}

impl SchedulerConfig {
    /// Validates explicit scheduler limits.
    pub const fn new(
        max_pending_deliveries: usize,
        max_buffered_bytes: usize,
    ) -> Result<Self, SchedulerError> {
        if max_pending_deliveries == 0 || max_pending_deliveries > MAX_SCHEDULER_PENDING {
            return Err(SchedulerError::InvalidLimit);
        }
        if max_buffered_bytes == 0 || max_buffered_bytes > MAX_SCHEDULER_BYTES {
            return Err(SchedulerError::InvalidLimit);
        }
        Ok(Self {
            max_pending_deliveries,
            max_buffered_bytes,
        })
    }
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            max_pending_deliveries: MAX_SCHEDULER_PENDING,
            max_buffered_bytes: MAX_SCHEDULER_BYTES,
        }
    }
}

/// Bounded stream endpoint configuration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StreamConfig {
    /// Maximum bytes queued at one receiver.
    pub receive_capacity: usize,
    /// Maximum bytes accepted by one scheduled write segment.
    pub max_segment_bytes: usize,
}

impl StreamConfig {
    /// Creates a stream configuration.
    pub const fn new(
        receive_capacity: usize,
        max_segment_bytes: usize,
    ) -> Result<Self, SchedulerError> {
        if receive_capacity == 0 || max_segment_bytes == 0 || max_segment_bytes > receive_capacity {
            return Err(SchedulerError::InvalidLimit);
        }
        Ok(Self {
            receive_capacity,
            max_segment_bytes,
        })
    }
}

impl Default for StreamConfig {
    fn default() -> Self {
        Self {
            receive_capacity: 64 * 1024,
            max_segment_bytes: 1024,
        }
    }
}

/// Bounded datagram endpoint configuration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DatagramConfig {
    /// Maximum datagram payload.
    pub max_datagram_size: usize,
    /// Maximum complete datagrams queued at one receiver.
    pub receive_capacity: usize,
}

impl DatagramConfig {
    /// Creates a datagram configuration.
    pub const fn new(
        max_datagram_size: usize,
        receive_capacity: usize,
    ) -> Result<Self, SchedulerError> {
        if max_datagram_size == 0 || max_datagram_size > MAX_DATAGRAM_SIZE || receive_capacity == 0
        {
            return Err(SchedulerError::InvalidLimit);
        }
        Ok(Self {
            max_datagram_size,
            receive_capacity,
        })
    }
}

impl Default for DatagramConfig {
    fn default() -> Self {
        Self {
            max_datagram_size: 1200,
            receive_capacity: 64,
        }
    }
}

/// Scheduler construction and operation errors.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SchedulerError {
    /// A requested limit was outside the hard bound.
    InvalidLimit,
    /// A scheduler or link was closed.
    Closed,
    /// The pending-unit limit was reached.
    PendingLimit,
    /// The buffered-byte limit was reached.
    ByteLimit,
    /// The receiver's bounded queue could not accept the unit.
    ReceiverBackpressure,
    /// Monotonic time overflow or teardown.
    Clock(ClockError),
    /// Resource budget admission failed.
    Resource(i2pr_core::ResourceError),
    /// The fault plan could not be executed within its bounds.
    Fault(crate::FaultError),
}

impl fmt::Display for SchedulerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLimit => formatter.write_str("simulation limit is invalid"),
            Self::Closed => formatter.write_str("simulation scheduler is closed"),
            Self::PendingLimit => formatter.write_str("simulation pending-delivery limit reached"),
            Self::ByteLimit => formatter.write_str("simulation buffered-byte limit reached"),
            Self::ReceiverBackpressure => formatter.write_str("simulation receiver queue is full"),
            Self::Clock(error) => error.fmt(formatter),
            Self::Resource(error) => error.fmt(formatter),
            Self::Fault(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for SchedulerError {}

impl From<ClockError> for SchedulerError {
    fn from(error: ClockError) -> Self {
        Self::Clock(error)
    }
}
impl From<i2pr_core::ResourceError> for SchedulerError {
    fn from(error: i2pr_core::ResourceError) -> Self {
        Self::Resource(error)
    }
}
impl From<crate::FaultError> for SchedulerError {
    fn from(error: crate::FaultError) -> Self {
        Self::Fault(error)
    }
}

/// Errors returned by stream operations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StreamError {
    /// The operation would exceed a bounded queue.
    WouldBlock,
    /// The deadline elapsed before progress.
    Deadline,
    /// The caller cancellation scope was cancelled.
    Cancelled,
    /// The peer closed gracefully.
    Closed,
    /// The peer reset and discarded queued data.
    Reset,
    /// The scheduler could not accept a segment.
    Scheduler(SchedulerError),
}

impl fmt::Display for StreamError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WouldBlock => formatter.write_str("stream queue is full"),
            Self::Deadline => formatter.write_str("stream operation deadline elapsed"),
            Self::Cancelled => formatter.write_str("stream operation cancelled"),
            Self::Closed => formatter.write_str("stream peer is closed"),
            Self::Reset => formatter.write_str("stream peer reset"),
            Self::Scheduler(error) => error.fmt(formatter),
        }
    }
}
impl std::error::Error for StreamError {}
impl From<SchedulerError> for StreamError {
    fn from(error: SchedulerError) -> Self {
        Self::Scheduler(error)
    }
}

/// Errors returned by datagram operations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DatagramError {
    /// The payload exceeds the endpoint maximum.
    TooLarge { maximum: usize },
    /// The queue is full.
    WouldBlock,
    /// The deadline elapsed before progress.
    Deadline,
    /// The caller cancellation scope was cancelled.
    Cancelled,
    /// The peer closed.
    Closed,
    /// The peer reset.
    Reset,
    /// The scheduler rejected the unit.
    Scheduler(SchedulerError),
}
impl fmt::Display for DatagramError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLarge { maximum } => {
                write!(formatter, "datagram exceeds {maximum}-byte limit")
            }
            Self::WouldBlock => formatter.write_str("datagram queue is full"),
            Self::Deadline => formatter.write_str("datagram operation deadline elapsed"),
            Self::Cancelled => formatter.write_str("datagram operation cancelled"),
            Self::Closed => formatter.write_str("datagram peer is closed"),
            Self::Reset => formatter.write_str("datagram peer reset"),
            Self::Scheduler(error) => error.fmt(formatter),
        }
    }
}
impl std::error::Error for DatagramError {}
impl From<SchedulerError> for DatagramError {
    fn from(error: SchedulerError) -> Self {
        Self::Scheduler(error)
    }
}

/// Synthetic bounded endpoint address used only in datagram diagnostics.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SyntheticAddress(u32);
impl SyntheticAddress {
    /// Returns the stable synthetic numeric address.
    pub const fn get(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialOrd, Ord, PartialEq)]
struct DeliveryKey {
    deadline: ManualInstant,
    link: LinkId,
    direction: LinkDirection,
    order_sequence: u64,
    sequence: u64,
    duplicate_index: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CloseMode {
    Open,
    Graceful,
    Reset,
}

#[derive(Debug)]
struct StreamInbound {
    bytes: VecDeque<u8>,
    close: CloseMode,
    close_after_pending: bool,
}

#[derive(Debug)]
struct StreamState {
    inbound: Mutex<StreamInbound>,
    reserved: AtomicUsize,
    capacity: usize,
    data: Notify,
    space: Notify,
    outbound_closed: AtomicBool,
}

impl StreamState {
    fn new(capacity: usize) -> Self {
        Self {
            inbound: Mutex::new(StreamInbound {
                bytes: VecDeque::new(),
                close: CloseMode::Open,
                close_after_pending: false,
            }),
            reserved: AtomicUsize::new(0),
            capacity,
            data: Notify::new(),
            space: Notify::new(),
            outbound_closed: AtomicBool::new(false),
        }
    }

    fn reserve(&self, amount: usize) -> bool {
        let Ok(inbound) = self.inbound.lock() else {
            return false;
        };
        if inbound.close != CloseMode::Open {
            return false;
        }
        let reserved = self.reserved.load(Ordering::Acquire);
        if inbound
            .bytes
            .len()
            .saturating_add(reserved)
            .saturating_add(amount)
            > self.capacity
        {
            return false;
        }
        self.reserved.fetch_add(amount, Ordering::AcqRel);
        true
    }

    fn release_reservation(&self, amount: usize) {
        self.reserved.fetch_sub(
            amount.min(self.reserved.load(Ordering::Acquire)),
            Ordering::AcqRel,
        );
    }

    fn deliver(&self, payload: &[u8]) -> DeliveryResult {
        let Ok(mut inbound) = self.inbound.lock() else {
            return DeliveryResult::Closed;
        };
        if inbound.close == CloseMode::Reset || inbound.close == CloseMode::Graceful {
            return DeliveryResult::Closed;
        }
        if inbound.bytes.len().saturating_add(payload.len()) > self.capacity {
            return DeliveryResult::Blocked;
        }
        self.release_reservation(payload.len());
        inbound.bytes.extend(payload.iter().copied());
        drop(inbound);
        self.data.notify_waiters();
        DeliveryResult::Delivered
    }

    fn reset(&self) {
        if let Ok(mut inbound) = self.inbound.lock() {
            inbound.bytes.clear();
            inbound.close = CloseMode::Reset;
            inbound.close_after_pending = false;
        }
        self.reserved.store(0, Ordering::Release);
        self.data.notify_waiters();
        self.space.notify_waiters();
    }

    fn request_graceful(&self) {
        if let Ok(mut inbound) = self.inbound.lock() {
            if inbound.close == CloseMode::Open {
                inbound.close_after_pending = true;
            }
        }
        self.data.notify_waiters();
    }

    fn close_if_drained(&self) {
        if let Ok(mut inbound) = self.inbound.lock() {
            if inbound.close_after_pending && inbound.bytes.is_empty() {
                inbound.close = CloseMode::Graceful;
            }
        }
        self.data.notify_waiters();
    }

    fn try_read(&self, destination: &mut [u8]) -> Result<Option<usize>, StreamError> {
        let Ok(mut inbound) = self.inbound.lock() else {
            return Err(StreamError::Closed);
        };
        if inbound.close == CloseMode::Reset {
            return Err(StreamError::Reset);
        }
        if inbound.bytes.is_empty() {
            return match inbound.close {
                CloseMode::Graceful => Ok(Some(0)),
                _ => Ok(None),
            };
        }
        let amount = destination.len().min(inbound.bytes.len());
        for slot in destination.iter_mut().take(amount) {
            *slot = inbound.bytes.pop_front().expect("length checked");
        }
        let drained = inbound.bytes.is_empty() && inbound.close_after_pending;
        drop(inbound);
        self.space.notify_waiters();
        if drained {
            self.close_if_drained();
        }
        Ok(Some(amount))
    }
}

#[derive(Debug)]
struct DatagramInbound {
    packets: VecDeque<DatagramPacket>,
    bytes: usize,
    close: CloseMode,
}

#[derive(Debug)]
struct DatagramState {
    inbound: Mutex<DatagramInbound>,
    reserved_packets: AtomicUsize,
    reserved_bytes: AtomicUsize,
    packet_capacity: usize,
    byte_capacity: usize,
    data: Notify,
    space: Notify,
    outbound_closed: AtomicBool,
}

impl DatagramState {
    fn new(config: DatagramConfig) -> Self {
        Self {
            inbound: Mutex::new(DatagramInbound {
                packets: VecDeque::new(),
                bytes: 0,
                close: CloseMode::Open,
            }),
            reserved_packets: AtomicUsize::new(0),
            reserved_bytes: AtomicUsize::new(0),
            packet_capacity: config.receive_capacity,
            byte_capacity: config
                .max_datagram_size
                .saturating_mul(config.receive_capacity),
            data: Notify::new(),
            space: Notify::new(),
            outbound_closed: AtomicBool::new(false),
        }
    }

    fn reserve(&self, amount: usize) -> bool {
        let Ok(inbound) = self.inbound.lock() else {
            return false;
        };
        if inbound.close != CloseMode::Open {
            return false;
        }
        if inbound
            .packets
            .len()
            .saturating_add(self.reserved_packets.load(Ordering::Acquire))
            >= self.packet_capacity
            || inbound
                .bytes
                .saturating_add(self.reserved_bytes.load(Ordering::Acquire))
                .saturating_add(amount)
                > self.byte_capacity
        {
            return false;
        }
        self.reserved_packets.fetch_add(1, Ordering::AcqRel);
        self.reserved_bytes.fetch_add(amount, Ordering::AcqRel);
        true
    }

    fn release_reservation(&self, amount: usize) {
        self.reserved_packets.fetch_sub(
            1.min(self.reserved_packets.load(Ordering::Acquire)),
            Ordering::AcqRel,
        );
        self.reserved_bytes.fetch_sub(
            amount.min(self.reserved_bytes.load(Ordering::Acquire)),
            Ordering::AcqRel,
        );
    }

    fn deliver(&self, packet: DatagramPacket) -> DeliveryResult {
        let Ok(mut inbound) = self.inbound.lock() else {
            return DeliveryResult::Closed;
        };
        if inbound.close != CloseMode::Open {
            return DeliveryResult::Closed;
        }
        if inbound.packets.len() >= self.packet_capacity
            || inbound.bytes.saturating_add(packet.payload.len()) > self.byte_capacity
        {
            return DeliveryResult::Blocked;
        }
        self.release_reservation(packet.payload.len());
        inbound.bytes = inbound.bytes.saturating_add(packet.payload.len());
        inbound.packets.push_back(packet);
        drop(inbound);
        self.data.notify_waiters();
        DeliveryResult::Delivered
    }

    fn reset(&self) {
        if let Ok(mut inbound) = self.inbound.lock() {
            inbound.packets.clear();
            inbound.bytes = 0;
            inbound.close = CloseMode::Reset;
        }
        self.reserved_packets.store(0, Ordering::Release);
        self.reserved_bytes.store(0, Ordering::Release);
        self.data.notify_waiters();
        self.space.notify_waiters();
    }

    fn close(&self) {
        if let Ok(mut inbound) = self.inbound.lock() {
            inbound.close = CloseMode::Graceful;
        }
        self.data.notify_waiters();
        self.space.notify_waiters();
    }

    fn try_recv(&self) -> Result<Option<DatagramPacket>, DatagramError> {
        let Ok(mut inbound) = self.inbound.lock() else {
            return Err(DatagramError::Closed);
        };
        if inbound.close == CloseMode::Reset {
            return Err(DatagramError::Reset);
        }
        let Some(packet) = inbound.packets.pop_front() else {
            return match inbound.close {
                CloseMode::Graceful => Err(DatagramError::Closed),
                _ => Ok(None),
            };
        };
        inbound.bytes = inbound.bytes.saturating_sub(packet.payload.len());
        drop(inbound);
        self.space.notify_waiters();
        Ok(Some(packet))
    }
}

#[derive(Clone, Debug)]
enum Target {
    Stream(Arc<StreamState>),
    Datagram(Arc<DatagramState>),
}
impl Target {
    fn reserve(&self, amount: usize) -> bool {
        match self {
            Self::Stream(value) => value.reserve(amount),
            Self::Datagram(value) => value.reserve(amount),
        }
    }
    fn release(&self, amount: usize) {
        match self {
            Self::Stream(value) => value.release_reservation(amount),
            Self::Datagram(value) => value.release_reservation(amount),
        }
    }
    fn deliver(&self, payload: Vec<u8>, source: SyntheticAddress) -> DeliveryResult {
        match self {
            Self::Stream(value) => value.deliver(&payload),
            Self::Datagram(value) => value.deliver(DatagramPacket { source, payload }),
        }
    }
    fn reset(&self) {
        match self {
            Self::Stream(value) => value.reset(),
            Self::Datagram(value) => value.reset(),
        }
    }
    fn graceful(&self) {
        match self {
            Self::Stream(value) => value.request_graceful(),
            Self::Datagram(value) => value.close(),
        }
    }
    fn is_same(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Stream(left), Self::Stream(right)) => Arc::ptr_eq(left, right),
            (Self::Datagram(left), Self::Datagram(right)) => Arc::ptr_eq(left, right),
            _ => false,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DeliveryResult {
    Delivered,
    Blocked,
    Closed,
}

#[derive(Debug)]
struct Delivery {
    target: Target,
    source: SyntheticAddress,
    payload: Vec<u8>,
    lease: i2pr_core::ResourceBundle,
    applied: Vec<u16>,
    terminal: Option<FaultTerminal>,
}

#[derive(Debug)]
struct SchedulerState {
    pending: BTreeMap<DeliveryKey, Delivery>,
    next_sequence: BTreeMap<(LinkId, LinkDirection), u64>,
    targets: Vec<Target>,
    events: Vec<ReplayEvent>,
    closed: bool,
    buffered_bytes: usize,
}

struct ScheduleRequest<'a> {
    link: LinkId,
    direction: LinkDirection,
    kind: FaultUnitKind,
    source: SyntheticAddress,
    target: Target,
    faults: &'a FaultScript,
    payload: Vec<u8>,
}

#[derive(Debug)]
struct SchedulerInner {
    clock: ManualClock,
    config: SchedulerConfig,
    budget: ResourceBudget,
    state: Mutex<SchedulerState>,
}

/// A clonable deterministic scheduler. It has no scheduler task of its own.
#[derive(Clone, Debug)]
pub struct NetworkScheduler {
    inner: Arc<SchedulerInner>,
}

impl NetworkScheduler {
    /// Creates a scheduler with explicit limits and a fresh resource budget.
    pub fn new(clock: ManualClock, config: SchedulerConfig) -> Result<Self, SchedulerError> {
        let budget = ResourceBudget::new([
            ResourceLimit::new(
                ResourceClass::PendingTimers,
                config.max_pending_deliveries as u64,
            )?,
            ResourceLimit::new(
                ResourceClass::BufferedBytes,
                config.max_buffered_bytes as u64,
            )?,
            ResourceLimit::new(ResourceClass::SimulatedStreamLinks, MAX_LINKS as u64)?,
            ResourceLimit::new(ResourceClass::SimulatedDatagramLinks, MAX_LINKS as u64)?,
        ])?;
        Self::with_budget(clock, config, budget)
    }

    /// Creates a scheduler using an existing Plan 022 resource budget.
    pub fn with_budget(
        clock: ManualClock,
        config: SchedulerConfig,
        budget: ResourceBudget,
    ) -> Result<Self, SchedulerError> {
        Ok(Self {
            inner: Arc::new(SchedulerInner {
                clock,
                config,
                budget,
                state: Mutex::new(SchedulerState {
                    pending: BTreeMap::new(),
                    next_sequence: BTreeMap::new(),
                    targets: Vec::new(),
                    events: Vec::new(),
                    closed: false,
                    buffered_bytes: 0,
                }),
            }),
        })
    }

    /// Returns the manual clock shared by this scheduler.
    pub fn clock(&self) -> &ManualClock {
        &self.inner.clock
    }

    /// Returns the scheduler resource budget.
    pub fn budget(&self) -> &ResourceBudget {
        &self.inner.budget
    }

    /// Creates a bounded stream link pair.
    pub fn stream_link(
        &self,
        link: LinkId,
        config: StreamConfig,
        faults: FaultScript,
    ) -> Result<StreamLink, SchedulerError> {
        let lease = self.inner.budget.try_acquire(ResourceRequest::new(
            ResourceClass::SimulatedStreamLinks,
            1,
        )?)?;
        let left = Arc::new(StreamState::new(config.receive_capacity));
        let right = Arc::new(StreamState::new(config.receive_capacity));
        let owner = Arc::new(StreamOwner {
            scheduler: self.clone(),
            link,
            config,
            faults,
            left: Arc::clone(&left),
            right: Arc::clone(&right),
            left_address: SyntheticAddress(link.get().saturating_mul(2).saturating_sub(1)),
            right_address: SyntheticAddress(link.get().saturating_mul(2)),
            _lease: lease,
        });
        self.add_targets(Target::Stream(left), Target::Stream(right))?;
        Ok(StreamLink {
            left: StreamEndpoint {
                owner: Arc::clone(&owner),
                side: Side::Left,
            },
            right: StreamEndpoint {
                owner,
                side: Side::Right,
            },
        })
    }

    /// Creates a bounded datagram link pair.
    pub fn datagram_link(
        &self,
        link: LinkId,
        config: DatagramConfig,
        faults: FaultScript,
    ) -> Result<DatagramLink, SchedulerError> {
        let lease = self.inner.budget.try_acquire(ResourceRequest::new(
            ResourceClass::SimulatedDatagramLinks,
            1,
        )?)?;
        let left = Arc::new(DatagramState::new(config));
        let right = Arc::new(DatagramState::new(config));
        let owner = Arc::new(DatagramOwner {
            scheduler: self.clone(),
            link,
            config,
            faults,
            left: Arc::clone(&left),
            right: Arc::clone(&right),
            left_address: SyntheticAddress(link.get().saturating_mul(2).saturating_sub(1)),
            right_address: SyntheticAddress(link.get().saturating_mul(2)),
            _lease: lease,
        });
        self.add_targets(Target::Datagram(left), Target::Datagram(right))?;
        Ok(DatagramLink {
            left: DatagramEndpoint {
                owner: Arc::clone(&owner),
                side: Side::Left,
            },
            right: DatagramEndpoint {
                owner,
                side: Side::Right,
            },
        })
    }

    fn add_targets(&self, left: Target, right: Target) -> Result<(), SchedulerError> {
        let mut state = self
            .inner
            .state
            .lock()
            .map_err(|_| SchedulerError::Closed)?;
        if state.closed {
            return Err(SchedulerError::Closed);
        }
        if state.targets.len() >= MAX_LINKS.saturating_mul(2) {
            return Err(SchedulerError::PendingLimit);
        }
        state.targets.push(left);
        state.targets.push(right);
        Ok(())
    }

    fn schedule(&self, request: ScheduleRequest) -> Result<usize, SchedulerError> {
        let ScheduleRequest {
            link,
            direction,
            kind,
            source,
            target,
            faults,
            payload,
        } = request;
        let input_len = payload.len();
        if payload.is_empty() {
            return Ok(0);
        }
        let mut state = self
            .inner
            .state
            .lock()
            .map_err(|_| SchedulerError::Closed)?;
        if state.closed {
            return Err(SchedulerError::Closed);
        }
        let sequence = *state.next_sequence.entry((link, direction)).or_insert(0);
        *state
            .next_sequence
            .get_mut(&(link, direction))
            .expect("sequence entry") = sequence
            .checked_add(1)
            .ok_or(SchedulerError::PendingLimit)?;
        let plan = faults.apply(link, direction, kind, sequence, payload)?;
        if let Some(FaultTerminal::Reset) = plan.terminal {
            self.purge_target_locked(&mut state, &target);
            target.reset();
            state.events.push(ReplayEvent::terminal(
                link,
                direction,
                kind,
                sequence,
                &plan.applied,
                FaultTerminal::Reset,
            ));
            return Ok(input_len);
        }
        let units = plan
            .units
            .iter()
            .filter(|unit| !unit.payload.is_empty())
            .collect::<Vec<_>>();
        if state.pending.len().saturating_add(units.len())
            > self.inner.config.max_pending_deliveries
        {
            return Err(SchedulerError::PendingLimit);
        }
        let bytes = units.iter().map(|unit| unit.payload.len()).sum::<usize>();
        if state.buffered_bytes.saturating_add(bytes) > self.inner.config.max_buffered_bytes {
            return Err(SchedulerError::ByteLimit);
        }
        if bytes > 0 && !target.reserve(bytes) {
            return Err(SchedulerError::ReceiverBackpressure);
        }
        let mut deliveries = Vec::with_capacity(units.len());
        for (index, unit) in units.iter().enumerate() {
            let deadline = self
                .inner
                .clock
                .now()
                .as_nanos()
                .checked_add(
                    unit.delay
                        .as_nanos()
                        .try_into()
                        .map_err(|_| SchedulerError::Clock(ClockError::Overflow))?,
                )
                .ok_or(SchedulerError::Clock(ClockError::Overflow))?;
            let lease = match self.inner.budget.try_acquire_bundle([
                ResourceRequest::new(ResourceClass::PendingTimers, 1)?,
                ResourceRequest::new(ResourceClass::BufferedBytes, unit.payload.len() as u64)?,
            ]) {
                Ok(lease) => lease,
                Err(error) => {
                    target.release(bytes);
                    return Err(SchedulerError::Resource(error));
                }
            };
            let terminal = if index + 1 == units.len() {
                plan.terminal
            } else {
                None
            };
            deliveries.push((
                DeliveryKey {
                    deadline: ManualInstant::from_nanos(deadline),
                    link,
                    direction,
                    order_sequence: unit.order_sequence,
                    sequence,
                    duplicate_index: unit.duplicate_index,
                },
                Delivery {
                    target: target.clone(),
                    source,
                    payload: unit.payload.clone(),
                    lease,
                    applied: plan.applied.clone(),
                    terminal,
                },
            ));
        }
        for (key, delivery) in deliveries {
            state.buffered_bytes = state.buffered_bytes.saturating_add(delivery.payload.len());
            state.pending.insert(key, delivery);
        }
        if units.is_empty() {
            if matches!(plan.terminal, Some(FaultTerminal::Disconnect)) {
                target.graceful();
            }
            state.events.push(ReplayEvent::outcome(
                link,
                direction,
                kind,
                sequence,
                0,
                &plan.applied,
                ReplayOutcome::Dropped,
            ));
        }
        Ok(input_len)
    }

    fn purge_target_locked(&self, state: &mut SchedulerState, target: &Target) {
        let keys = state
            .pending
            .iter()
            .filter(|(_, delivery)| delivery.target.is_same(target))
            .map(|(key, _)| *key)
            .collect::<Vec<_>>();
        for key in keys {
            if let Some(delivery) = state.pending.remove(&key) {
                state.buffered_bytes = state.buffered_bytes.saturating_sub(delivery.payload.len());
                target.release(delivery.payload.len());
                drop(delivery.lease);
            }
        }
    }

    /// Advances time and delivers all units due at the new instant.
    pub fn advance(&self, duration: Duration) -> Result<AdvanceReport, SchedulerError> {
        self.inner.clock.advance(duration)?;
        self.deliver_due()
    }

    /// Advances exactly to the next scheduled deadline and delivers due work.
    pub fn advance_to_next_event(&self) -> Result<AdvanceReport, SchedulerError> {
        let Some(deadline) = self.next_deadline() else {
            return Ok(AdvanceReport::default());
        };
        let now = self.inner.clock.now();
        let delta = deadline.elapsed().saturating_sub(now.elapsed());
        self.advance(delta)
    }

    fn deliver_due(&self) -> Result<AdvanceReport, SchedulerError> {
        let mut report = AdvanceReport::default();
        loop {
            let (key, delivery) = {
                let mut state = self
                    .inner
                    .state
                    .lock()
                    .map_err(|_| SchedulerError::Closed)?;
                let Some((&key, _)) = state.pending.first_key_value() else {
                    break;
                };
                if key.deadline > self.inner.clock.now() {
                    break;
                }
                let delivery = state.pending.remove(&key).expect("pending key exists");
                state.buffered_bytes = state.buffered_bytes.saturating_sub(delivery.payload.len());
                (key, delivery)
            };
            let target = delivery.target.clone();
            match target.deliver(delivery.payload.clone(), delivery.source) {
                DeliveryResult::Delivered => {
                    report.delivered = report.delivered.saturating_add(1);
                    report.progressed = true;
                    if let Some(terminal) = delivery.terminal {
                        match terminal {
                            FaultTerminal::Disconnect => target.graceful(),
                            FaultTerminal::Reset => target.reset(),
                            FaultTerminal::Drop => {}
                        }
                    }
                    self.record_delivery(&key, &delivery, ReplayOutcome::Delivered);
                    drop(delivery.lease);
                }
                DeliveryResult::Closed => {
                    target.release(delivery.payload.len());
                    report.dropped = report.dropped.saturating_add(1);
                    report.progressed = true;
                    self.record_delivery(&key, &delivery, ReplayOutcome::Dropped);
                    drop(delivery.lease);
                }
                DeliveryResult::Blocked => {
                    let mut state = self
                        .inner
                        .state
                        .lock()
                        .map_err(|_| SchedulerError::Closed)?;
                    state.buffered_bytes =
                        state.buffered_bytes.saturating_add(delivery.payload.len());
                    state.pending.insert(key, delivery);
                    report.blocked = report.blocked.saturating_add(1);
                    break;
                }
            }
            self.maybe_close_drained(&target);
        }
        Ok(report)
    }

    fn record_delivery(&self, key: &DeliveryKey, delivery: &Delivery, outcome: ReplayOutcome) {
        if let Ok(mut state) = self.inner.state.lock() {
            state.events.push(ReplayEvent::outcome(
                key.link,
                key.direction,
                if matches!(delivery.target, Target::Stream(_)) {
                    FaultUnitKind::Stream
                } else {
                    FaultUnitKind::Datagram
                },
                key.sequence,
                key.duplicate_index,
                &delivery.applied,
                outcome,
            ));
        }
    }

    fn maybe_close_drained(&self, target: &Target) {
        let pending = self
            .inner
            .state
            .lock()
            .map(|state| {
                state
                    .pending
                    .values()
                    .any(|delivery| delivery.target.is_same(target))
            })
            .unwrap_or(false);
        if !pending {
            if let Target::Stream(stream) = target {
                stream.close_if_drained();
            }
        }
    }

    fn next_deadline(&self) -> Option<ManualInstant> {
        self.inner
            .state
            .lock()
            .ok()
            .and_then(|state| state.pending.first_key_value().map(|(key, _)| key.deadline))
    }
    /// Returns whether any delivery is pending.
    pub fn has_pending(&self) -> bool {
        self.inner
            .state
            .lock()
            .map(|state| !state.pending.is_empty())
            .unwrap_or(false)
    }
    /// Returns a privacy-safe resource and queue snapshot.
    pub fn snapshot(&self) -> SchedulerSnapshot {
        let (pending, bytes, closed) = self
            .inner
            .state
            .lock()
            .map(|state| (state.pending.len(), state.buffered_bytes, state.closed))
            .unwrap_or((0, 0, true));
        SchedulerSnapshot {
            pending_deliveries: pending,
            buffered_bytes: bytes,
            pending_timers: self.inner.clock.pending_timers(),
            stream_links: self
                .inner
                .budget
                .usage(ResourceClass::SimulatedStreamLinks)
                .map(|usage| usage.used)
                .unwrap_or(0),
            datagram_links: self
                .inner
                .budget
                .usage(ResourceClass::SimulatedDatagramLinks)
                .map(|usage| usage.used)
                .unwrap_or(0),
            closed,
            resource_usage: self.inner.budget.snapshot().unwrap_or_default(),
        }
    }

    /// Closes the scheduler, purges queued units, and wakes endpoint readers/writers.
    pub fn close(&self) {
        let targets = if let Ok(mut state) = self.inner.state.lock() {
            state.closed = true;
            let deliveries = std::mem::take(&mut state.pending)
                .into_values()
                .collect::<Vec<_>>();
            state.buffered_bytes = 0;
            for delivery in deliveries {
                delivery.target.release(delivery.payload.len());
                drop(delivery.lease);
            }
            state.targets.clone()
        } else {
            Vec::new()
        };
        for target in targets {
            target.reset();
        }
    }

    /// Produces a bounded replay record.
    pub fn replay(
        &self,
        seed: ReproducibilitySeed,
        scenario: &str,
        steps: usize,
    ) -> crate::ReplayRecord {
        let (events, final_time) = self
            .inner
            .state
            .lock()
            .map(|state| (state.events.clone(), self.inner.clock.now()))
            .unwrap_or((Vec::new(), self.inner.clock.now()));
        crate::ReplayRecord {
            seed,
            scenario: scenario.to_owned(),
            events,
            final_time,
            snapshot: self.snapshot(),
            steps,
        }
    }
}

/// Delivery counts from one manual scheduler pump.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AdvanceReport {
    /// Units delivered.
    pub delivered: usize,
    /// Units dropped.
    pub dropped: usize,
    /// Due work blocked by a receiver.
    pub blocked: usize,
    /// Whether this pump made progress.
    pub progressed: bool,
}

/// Safe scheduler state; it contains no payloads.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SchedulerSnapshot {
    /// Queued delivery units.
    pub pending_deliveries: usize,
    /// Queued bytes.
    pub buffered_bytes: usize,
    /// Registered manual sleepers.
    pub pending_timers: usize,
    /// Held stream-link leases.
    pub stream_links: u64,
    /// Held datagram-link leases.
    pub datagram_links: u64,
    /// Whether shutdown has closed this scheduler.
    pub closed: bool,
    /// Underlying bounded resource usage.
    pub resource_usage: Vec<ResourceUsage>,
}

/// Replay-safe event outcome.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReplayOutcome {
    Scheduled,
    Delivered,
    Dropped,
    Disconnect,
    Reset,
}

/// Replay-safe event metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReplayEvent {
    pub link: LinkId,
    pub direction: LinkDirection,
    pub kind: FaultUnitKind,
    pub sequence: u64,
    pub duplicate_index: u8,
    pub rules: Vec<u16>,
    pub outcome: ReplayOutcome,
}
impl ReplayEvent {
    fn outcome(
        link: LinkId,
        direction: LinkDirection,
        kind: FaultUnitKind,
        sequence: u64,
        duplicate_index: u8,
        rules: &[u16],
        outcome: ReplayOutcome,
    ) -> Self {
        Self {
            link,
            direction,
            kind,
            sequence,
            duplicate_index,
            rules: rules.to_vec(),
            outcome,
        }
    }
    fn terminal(
        link: LinkId,
        direction: LinkDirection,
        kind: FaultUnitKind,
        sequence: u64,
        rules: &[u16],
        terminal: FaultTerminal,
    ) -> Self {
        Self::outcome(
            link,
            direction,
            kind,
            sequence,
            0,
            rules,
            match terminal {
                FaultTerminal::Disconnect => ReplayOutcome::Disconnect,
                FaultTerminal::Reset => ReplayOutcome::Reset,
                FaultTerminal::Drop => ReplayOutcome::Dropped,
            },
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Side {
    Left,
    Right,
}
impl Side {
    fn direction(self) -> LinkDirection {
        match self {
            Self::Left => LinkDirection::AtoB,
            Self::Right => LinkDirection::BtoA,
        }
    }
}

#[derive(Debug)]
struct StreamOwner {
    scheduler: NetworkScheduler,
    link: LinkId,
    config: StreamConfig,
    faults: FaultScript,
    left: Arc<StreamState>,
    right: Arc<StreamState>,
    left_address: SyntheticAddress,
    right_address: SyntheticAddress,
    _lease: ResourceLease,
}

/// A bounded duplex stream link.
#[derive(Debug)]
pub struct StreamLink {
    left: StreamEndpoint,
    right: StreamEndpoint,
}
impl StreamLink {
    /// Returns the first endpoint.
    pub fn left(&self) -> StreamEndpoint {
        self.left.clone()
    }
    /// Returns the second endpoint.
    pub fn right(&self) -> StreamEndpoint {
        self.right.clone()
    }
}

/// One endpoint of a bounded stream.
#[derive(Clone, Debug)]
pub struct StreamEndpoint {
    owner: Arc<StreamOwner>,
    side: Side,
}
impl StreamEndpoint {
    fn local(&self) -> &Arc<StreamState> {
        match self.side {
            Side::Left => &self.owner.left,
            Side::Right => &self.owner.right,
        }
    }
    fn peer(&self) -> Target {
        match self.side {
            Side::Left => Target::Stream(Arc::clone(&self.owner.right)),
            Side::Right => Target::Stream(Arc::clone(&self.owner.left)),
        }
    }
    fn peer_space(&self) -> &Notify {
        match self.side {
            Side::Left => &self.owner.right.space,
            Side::Right => &self.owner.left.space,
        }
    }
    fn source(&self) -> SyntheticAddress {
        match self.side {
            Side::Left => self.owner.left_address,
            Side::Right => self.owner.right_address,
        }
    }
    /// Returns this endpoint's synthetic address.
    pub fn address(&self) -> SyntheticAddress {
        match self.side {
            Side::Left => self.owner.left_address,
            Side::Right => self.owner.right_address,
        }
    }
    /// Attempts one deterministic partial write.
    pub fn try_write(&self, bytes: &[u8]) -> Result<usize, StreamError> {
        if bytes.is_empty() {
            return Ok(0);
        }
        if self.local().outbound_closed.load(Ordering::Acquire) {
            return Err(StreamError::Closed);
        }
        let amount = bytes.len().min(self.owner.config.max_segment_bytes);
        self.owner
            .scheduler
            .schedule(ScheduleRequest {
                link: self.owner.link,
                direction: self.side.direction(),
                kind: FaultUnitKind::Stream,
                source: self.source(),
                target: self.peer(),
                faults: &self.owner.faults,
                payload: bytes[..amount].to_vec(),
            })
            .map(|_| amount)
            .map_err(StreamError::from)
    }
    /// Writes all bytes using bounded segments, a deadline, and cancellation.
    pub async fn write_until(
        &self,
        bytes: &[u8],
        deadline: Deadline,
        cancellation: &CancellationToken,
    ) -> Result<usize, StreamError> {
        let mut written = 0;
        while written < bytes.len() {
            let notified = self.peer_space().notified();
            match self.try_write(&bytes[written..]) {
                Ok(amount) => written += amount,
                Err(StreamError::Scheduler(
                    SchedulerError::ReceiverBackpressure
                    | SchedulerError::PendingLimit
                    | SchedulerError::ByteLimit,
                ))
                | Err(StreamError::WouldBlock) => {
                    let sleep = self.owner.scheduler.clock().sleep_until(deadline);
                    tokio::select! { _ = notified => {}, _ = cancellation.cancelled() => return Err(StreamError::Cancelled), result = sleep => { result.map_err(|error| StreamError::Scheduler(SchedulerError::Clock(error)))?; return Err(StreamError::Deadline); } }
                }
                Err(error) => return Err(error),
            }
        }
        Ok(written)
    }
    /// Attempts to read available bytes, returning `None` when it must wait.
    pub fn try_read(&self, destination: &mut [u8]) -> Result<Option<usize>, StreamError> {
        self.local().try_read(destination)
    }
    /// Reads bytes using a deadline and cancellation. Returns zero at graceful EOF.
    pub async fn read_until(
        &self,
        destination: &mut [u8],
        deadline: Deadline,
        cancellation: &CancellationToken,
    ) -> Result<usize, StreamError> {
        loop {
            let notified = self.local().data.notified();
            if let Some(result) = self.try_read(destination)? {
                return Ok(result);
            }
            let sleep = self.owner.scheduler.clock().sleep_until(deadline);
            tokio::select! { _ = notified => {}, _ = cancellation.cancelled() => return Err(StreamError::Cancelled), result = sleep => { result.map_err(|error| StreamError::Scheduler(SchedulerError::Clock(error)))?; return Err(StreamError::Deadline); } }
        }
    }
    /// Half-closes this endpoint's write direction after queued bytes drain.
    pub fn shutdown(&self) -> Result<(), StreamError> {
        if self.local().outbound_closed.swap(true, Ordering::AcqRel) {
            return Ok(());
        }
        let peer = self.peer();
        if let Target::Stream(state) = &peer {
            state.request_graceful();
        }
        Ok(())
    }
    /// Immediately resets the peer and discards queued bytes.
    pub fn reset(&self) {
        let peer = self.peer();
        self.owner.scheduler.close_target(peer);
    }
}

#[derive(Debug)]
struct DatagramOwner {
    scheduler: NetworkScheduler,
    link: LinkId,
    config: DatagramConfig,
    faults: FaultScript,
    left: Arc<DatagramState>,
    right: Arc<DatagramState>,
    left_address: SyntheticAddress,
    right_address: SyntheticAddress,
    _lease: ResourceLease,
}

/// A bounded duplex datagram link.
#[derive(Debug)]
pub struct DatagramLink {
    left: DatagramEndpoint,
    right: DatagramEndpoint,
}
impl DatagramLink {
    /// Returns the first endpoint.
    pub fn left(&self) -> DatagramEndpoint {
        self.left.clone()
    }
    /// Returns the second endpoint.
    pub fn right(&self) -> DatagramEndpoint {
        self.right.clone()
    }
}

/// A complete datagram and its synthetic source address.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DatagramPacket {
    /// Synthetic source address.
    pub source: SyntheticAddress,
    /// Complete payload.
    pub payload: Vec<u8>,
}

/// One endpoint of a bounded datagram link.
#[derive(Clone, Debug)]
pub struct DatagramEndpoint {
    owner: Arc<DatagramOwner>,
    side: Side,
}
impl DatagramEndpoint {
    fn local(&self) -> &Arc<DatagramState> {
        match self.side {
            Side::Left => &self.owner.left,
            Side::Right => &self.owner.right,
        }
    }
    fn peer(&self) -> Target {
        match self.side {
            Side::Left => Target::Datagram(Arc::clone(&self.owner.right)),
            Side::Right => Target::Datagram(Arc::clone(&self.owner.left)),
        }
    }
    fn peer_space(&self) -> &Notify {
        match self.side {
            Side::Left => &self.owner.right.space,
            Side::Right => &self.owner.left.space,
        }
    }
    fn source(&self) -> SyntheticAddress {
        match self.side {
            Side::Left => self.owner.left_address,
            Side::Right => self.owner.right_address,
        }
    }
    /// Returns this endpoint's synthetic address.
    pub fn address(&self) -> SyntheticAddress {
        self.source()
    }
    /// Attempts to queue one complete datagram.
    pub fn try_send(&self, payload: &[u8]) -> Result<usize, DatagramError> {
        if payload.len() > self.owner.config.max_datagram_size {
            return Err(DatagramError::TooLarge {
                maximum: self.owner.config.max_datagram_size,
            });
        }
        if self.local().outbound_closed.load(Ordering::Acquire) {
            return Err(DatagramError::Closed);
        }
        self.owner
            .scheduler
            .schedule(ScheduleRequest {
                link: self.owner.link,
                direction: self.side.direction(),
                kind: FaultUnitKind::Datagram,
                source: self.source(),
                target: self.peer(),
                faults: &self.owner.faults,
                payload: payload.to_vec(),
            })
            .map(|_| payload.len())
            .map_err(DatagramError::from)
    }
    /// Sends one datagram with a deadline and cancellation.
    pub async fn send_until(
        &self,
        payload: &[u8],
        deadline: Deadline,
        cancellation: &CancellationToken,
    ) -> Result<usize, DatagramError> {
        loop {
            let notified = self.peer_space().notified();
            match self.try_send(payload) {
                Ok(amount) => return Ok(amount),
                Err(DatagramError::Scheduler(
                    SchedulerError::ReceiverBackpressure
                    | SchedulerError::PendingLimit
                    | SchedulerError::ByteLimit,
                ))
                | Err(DatagramError::WouldBlock) => {
                    let sleep = self.owner.scheduler.clock().sleep_until(deadline);
                    tokio::select! { _ = notified => {}, _ = cancellation.cancelled() => return Err(DatagramError::Cancelled), result = sleep => { result.map_err(|error| DatagramError::Scheduler(SchedulerError::Clock(error)))?; return Err(DatagramError::Deadline); } }
                }
                Err(error) => return Err(error),
            }
        }
    }
    /// Attempts to receive one complete datagram.
    pub fn try_recv(&self) -> Result<Option<DatagramPacket>, DatagramError> {
        self.local().try_recv()
    }
    /// Receives one complete datagram with a deadline and cancellation.
    pub async fn recv_until(
        &self,
        deadline: Deadline,
        cancellation: &CancellationToken,
    ) -> Result<DatagramPacket, DatagramError> {
        loop {
            let notified = self.local().data.notified();
            if let Some(packet) = self.try_recv()? {
                return Ok(packet);
            }
            let sleep = self.owner.scheduler.clock().sleep_until(deadline);
            tokio::select! { _ = notified => {}, _ = cancellation.cancelled() => return Err(DatagramError::Cancelled), result = sleep => { result.map_err(|error| DatagramError::Scheduler(SchedulerError::Clock(error)))?; return Err(DatagramError::Deadline); } }
        }
    }
    /// Gracefully closes this endpoint's send direction.
    pub fn shutdown(&self) {
        self.local().outbound_closed.store(true, Ordering::Release);
        self.peer().graceful();
    }
    /// Resets the peer.
    pub fn reset(&self) {
        self.owner.scheduler.close_target(self.peer());
    }
}

impl NetworkScheduler {
    fn close_target(&self, target: Target) {
        if let Ok(mut state) = self.inner.state.lock() {
            self.purge_target_locked(&mut state, &target);
            target.reset();
        }
    }
}
