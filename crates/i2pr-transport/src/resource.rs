//! Transport-specific use of the shared core resource governor.

use i2pr_core::{
    ResourceBudget, ResourceBundle, ResourceClass, ResourceError, ResourceLease, ResourceLimit,
    ResourceRequest, ResourceUsage,
};
use std::fmt;

/// Absolute ceiling for one configured transport resource class.
pub const MAX_TRANSPORT_RESOURCE_LIMIT: u64 = 1 << 30;
/// Absolute ceiling for one per-link queue item count.
pub const MAX_TRANSPORT_QUEUE_CAPACITY: u64 = 4_096;

/// Initial bounded infrastructure ceilings used by a transport manager.
///
/// These are ownership ceilings, not production protocol defaults. Later
/// runtime configuration plans may select deployment-specific values while
/// preserving the bounds checked here.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransportLimits {
    /// Maximum candidates holding a pending-handshake lease.
    pub max_pending_handshakes: u64,
    /// Maximum authenticated or draining links globally.
    pub max_active_links: u64,
    /// Maximum bytes retained in all transport queues.
    pub max_buffered_bytes: u64,
    /// Maximum queued delivery items globally.
    pub max_queued_messages: u64,
    /// Maximum link entries for one peer.
    pub max_links_per_peer: u64,
    /// Maximum queued delivery items for one link.
    pub max_messages_per_link: u64,
    /// Maximum queued bytes for one link.
    pub max_bytes_per_link: u64,
}

impl TransportLimits {
    /// Validates all transport ownership ceilings.
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        max_pending_handshakes: u64,
        max_active_links: u64,
        max_buffered_bytes: u64,
        max_queued_messages: u64,
        max_links_per_peer: u64,
        max_messages_per_link: u64,
        max_bytes_per_link: u64,
    ) -> Result<Self, TransportResourceLimitsError> {
        let values = [
            (ResourceClass::PendingHandshakes, max_pending_handshakes),
            (ResourceClass::ActiveLinks, max_active_links),
            (ResourceClass::BufferedBytes, max_buffered_bytes),
            (ResourceClass::CommandQueueItems, max_queued_messages),
            (ResourceClass::ActiveLinks, max_links_per_peer),
            (ResourceClass::CommandQueueItems, max_messages_per_link),
            (ResourceClass::BufferedBytes, max_bytes_per_link),
        ];
        let mut index = 0;
        while index < values.len() {
            let (class, value) = values[index];
            if value == 0 {
                return Err(TransportResourceLimitsError::Zero { class });
            }
            if value > MAX_TRANSPORT_RESOURCE_LIMIT
                || (index == 3 || index == 5) && value > MAX_TRANSPORT_QUEUE_CAPACITY
            {
                return Err(TransportResourceLimitsError::TooLarge {
                    class,
                    maximum: if index == 3 || index == 5 {
                        MAX_TRANSPORT_QUEUE_CAPACITY
                    } else {
                        MAX_TRANSPORT_RESOURCE_LIMIT
                    },
                });
            }
            index += 1;
        }
        if max_links_per_peer > max_active_links {
            return Err(TransportResourceLimitsError::ScopedExceedsGlobal {
                scoped: ResourceClass::ActiveLinks,
                global: ResourceClass::ActiveLinks,
            });
        }
        if max_messages_per_link > max_queued_messages {
            return Err(TransportResourceLimitsError::ScopedExceedsGlobal {
                scoped: ResourceClass::CommandQueueItems,
                global: ResourceClass::CommandQueueItems,
            });
        }
        if max_bytes_per_link > max_buffered_bytes {
            return Err(TransportResourceLimitsError::ScopedExceedsGlobal {
                scoped: ResourceClass::BufferedBytes,
                global: ResourceClass::BufferedBytes,
            });
        }
        Ok(Self {
            max_pending_handshakes,
            max_active_links,
            max_buffered_bytes,
            max_queued_messages,
            max_links_per_peer,
            max_messages_per_link,
            max_bytes_per_link,
        })
    }

    /// A small deterministic ceiling set useful for synthetic contract tests.
    pub const fn for_test() -> Self {
        Self {
            max_pending_handshakes: 2,
            max_active_links: 4,
            max_buffered_bytes: 16 * 1024,
            max_queued_messages: 4,
            max_links_per_peer: 2,
            max_messages_per_link: 2,
            max_bytes_per_link: 8 * 1024,
        }
    }
}

/// Validation failures for transport ownership ceilings.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransportResourceLimitsError {
    /// A zero ceiling would make the corresponding path unusable.
    Zero {
        /// Resource class with the invalid zero ceiling.
        class: ResourceClass,
    },
    /// A ceiling exceeds the infrastructure bound.
    TooLarge {
        /// Resource class with the oversized ceiling.
        class: ResourceClass,
        /// Infrastructure maximum.
        maximum: u64,
    },
    /// A scoped ceiling exceeds the corresponding global ceiling.
    ScopedExceedsGlobal {
        /// The scoped resource whose ceiling is invalid.
        scoped: ResourceClass,
        /// The global resource that bounds the scoped resource.
        global: ResourceClass,
    },
}

impl fmt::Display for TransportResourceLimitsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Zero { class } => write!(formatter, "zero transport limit for {class:?}"),
            Self::TooLarge { class, maximum } => {
                write!(formatter, "transport limit for {class:?} exceeds {maximum}")
            }
            Self::ScopedExceedsGlobal { scoped, global } => {
                write!(
                    formatter,
                    "transport limit for {scoped:?} exceeds global {global:?}"
                )
            }
        }
    }
}

impl std::error::Error for TransportResourceLimitsError {}

/// A non-cloneable transport wrapper around one exact shared resource lease.
#[derive(Debug)]
pub struct TransportLease {
    inner: ResourceLease,
    class: ResourceClass,
    amount: u64,
}

impl TransportLease {
    /// Returns the shared resource class held by this owner.
    pub const fn class(&self) -> ResourceClass {
        self.class
    }

    /// Returns the exact number of units held by this owner.
    pub const fn amount(&self) -> u64 {
        self.amount
    }

    /// Releases the exact grant by consuming this owner.
    pub fn release(self) {
        let Self { inner, .. } = self;
        inner.release();
    }
}

/// Shared bounded resources used by transport manager state.
#[derive(Clone, Debug)]
pub struct TransportResources {
    budget: ResourceBudget,
}

impl TransportResources {
    /// Creates the shared budget from transport-specific ceilings.
    pub fn new(limits: TransportLimits) -> Result<Self, ResourceError> {
        let budget = ResourceBudget::new([
            ResourceLimit::new(
                ResourceClass::PendingHandshakes,
                limits.max_pending_handshakes,
            )?,
            ResourceLimit::new(ResourceClass::ActiveLinks, limits.max_active_links)?,
            ResourceLimit::new(ResourceClass::BufferedBytes, limits.max_buffered_bytes)?,
            ResourceLimit::new(ResourceClass::CommandQueueItems, limits.max_queued_messages)?,
        ])?;
        Ok(Self { budget })
    }

    /// Admits one exact class grant, retaining the shared lease semantics.
    pub fn admit(
        &self,
        class: ResourceClass,
        amount: u64,
    ) -> Result<TransportLease, ResourceError> {
        let request = ResourceRequest::new(class, amount)?;
        let lease = self.budget.try_acquire(request)?;
        Ok(TransportLease {
            inner: lease,
            class,
            amount,
        })
    }

    /// Atomically admits one queue item and its exact encoded-byte charge.
    pub fn admit_queue(&self, message_bytes: u64) -> Result<TransportQueueLease, ResourceError> {
        let item = ResourceRequest::new(ResourceClass::CommandQueueItems, 1)?;
        let bytes = ResourceRequest::new(ResourceClass::BufferedBytes, message_bytes)?;
        let bundle = self.budget.try_acquire_bundle([item, bytes])?;
        Ok(TransportQueueLease {
            inner: bundle,
            message_bytes,
        })
    }

    /// Returns current usage for one configured transport class.
    pub fn usage(&self, class: ResourceClass) -> Result<ResourceUsage, ResourceError> {
        self.budget.usage(class)
    }

    /// Returns deterministic usage for all transport-configured classes.
    pub fn snapshot(&self) -> Result<Vec<ResourceUsage>, ResourceError> {
        self.budget.snapshot()
    }
}

/// A non-cloneable atomic queue reservation for one item and its bytes.
#[derive(Debug)]
pub struct TransportQueueLease {
    inner: ResourceBundle,
    message_bytes: u64,
}

impl TransportQueueLease {
    /// Returns the exact encoded-byte charge held by this reservation.
    pub const fn message_bytes(&self) -> u64 {
        self.message_bytes
    }

    /// Releases the atomic queue reservation by consuming it.
    pub fn release(self) {
        let Self { inner, .. } = self;
        inner.release();
    }
}
