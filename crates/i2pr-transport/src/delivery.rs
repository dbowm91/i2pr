//! Owned delivery requests and typed outcomes.

use std::fmt;

use crate::manager::QueueAccounting;
use crate::resource::TransportQueueLease;
use crate::{Deadline, DeliveryId, EncodedI2npMessage, LinkId, PeerId, TerminationCategory};

/// An outbound request handed from a caller-owned response path to a link.
///
/// The runtime owns the response mapping represented by [`DeliveryId`]. This
/// contract deliberately exposes no channel, future, socket, or async trait.
pub struct DeliveryRequest {
    id: DeliveryId,
    target: PeerId,
    message: EncodedI2npMessage,
    deadline: Deadline,
    cancellation: Option<i2pr_core::CancellationToken>,
}

impl DeliveryRequest {
    /// Creates a request with a process-local one-shot response identifier.
    pub fn new(
        target: PeerId,
        message: EncodedI2npMessage,
        deadline: Deadline,
    ) -> Result<Self, crate::LinkIdError> {
        Ok(Self {
            id: DeliveryId::generate()?,
            target,
            message,
            deadline,
            cancellation: None,
        })
    }

    /// Creates a request with a caller-supplied response identifier.
    pub const fn with_id(
        id: DeliveryId,
        target: PeerId,
        message: EncodedI2npMessage,
        deadline: Deadline,
    ) -> Self {
        Self {
            id,
            target,
            message,
            deadline,
            cancellation: None,
        }
    }

    /// Attaches a caller-owned cancellation token to this request.
    pub fn with_cancellation(mut self, cancellation: i2pr_core::CancellationToken) -> Self {
        self.cancellation = Some(cancellation);
        self
    }

    /// Returns whether the caller has cancelled this request.
    pub fn is_cancelled(&self) -> bool {
        self.cancellation
            .as_ref()
            .is_some_and(i2pr_core::CancellationToken::is_cancelled)
    }

    /// Returns the runtime-owned response identifier.
    pub const fn id(&self) -> DeliveryId {
        self.id
    }

    /// Returns the target peer reference for state lookup.
    pub const fn target(&self) -> PeerId {
        self.target
    }

    /// Returns the caller's absolute monotonic deadline.
    pub const fn deadline(&self) -> Deadline {
        self.deadline
    }

    /// Returns the encoded message length without borrowing its bytes.
    pub fn message_len(&self) -> usize {
        self.message.len()
    }

    /// Borrows the encoded bytes for a runtime-owned write operation.
    pub fn message_bytes(&self) -> &[u8] {
        self.message.as_bytes()
    }

    /// Hands the encoded message owner to the runtime.
    pub fn into_message(self) -> EncodedI2npMessage {
        self.message
    }
}

impl fmt::Debug for DeliveryRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DeliveryRequest")
            .field("id", &self.id)
            .field("message_len", &self.message.len())
            .field("deadline", &self.deadline)
            .field("cancelled", &self.is_cancelled())
            .finish()
    }
}

/// A request retained by a link queue with its exact resource grants.
pub struct QueuedDelivery {
    request: DeliveryRequest,
    queue_lease: TransportQueueLease,
    _accounting: QueueAccounting,
}

impl QueuedDelivery {
    pub(crate) fn new(
        request: DeliveryRequest,
        queue_lease: TransportQueueLease,
        accounting: QueueAccounting,
    ) -> Self {
        Self {
            request,
            queue_lease,
            _accounting: accounting,
        }
    }

    /// Returns the request identifier without exposing peer or payload bytes.
    pub const fn id(&self) -> DeliveryId {
        self.request.id()
    }

    /// Returns the queued message length.
    pub fn message_len(&self) -> usize {
        self.request.message_len()
    }

    /// Returns the target peer reference for queue selection.
    pub const fn target(&self) -> PeerId {
        self.request.target()
    }

    /// Returns the queued request deadline.
    pub const fn deadline(&self) -> Deadline {
        self.request.deadline()
    }

    /// Borrows the encoded bytes while this queue owner remains alive.
    pub fn message_bytes(&self) -> &[u8] {
        self.request.message_bytes()
    }

    /// Releases queue accounting and hands the request to the write owner.
    pub fn into_request(self) -> DeliveryRequest {
        let Self {
            request,
            queue_lease,
            _accounting: _,
        } = self;
        queue_lease.release();
        request
    }
}

impl fmt::Debug for QueuedDelivery {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("QueuedDelivery")
            .field("id", &self.id())
            .field("message_len", &self.message_len())
            .field("deadline", &self.deadline())
            .finish()
    }
}

/// Typed result of attempting to hand a request to an authenticated link.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeliveryOutcome {
    /// The request now belongs to the bounded link queue.
    Accepted {
        /// The authenticated local link that accepted the request.
        link_id: LinkId,
    },
    /// No authenticated link currently exists for the target peer.
    NoActiveLink,
    /// The selected link queue has no item capacity.
    QueueFull,
    /// A pending link replacement or closure made the selected link unusable.
    LinkClosedBeforeWrite,
    /// The selected link was retired by duplicate replacement.
    LinkReplaced,
    /// A queue or byte resource could not be admitted.
    ResourceDenied,
    /// The caller's deadline had elapsed before admission.
    DeadlineElapsed,
    /// The caller cancelled before admission.
    Cancelled,
    /// A fixed protocol termination category ended the attempted delivery.
    ProtocolTerminated {
        /// The fixed protocol category that ended the attempt.
        category: TerminationCategory,
    },
    /// The request target did not match the selected link identity.
    PeerIdentityMismatch,
    /// A higher layer newly scheduled a dial attempt.
    DialScheduled,
    /// A higher layer already has a dial attempt pending.
    DialAlreadyPending,
}
