//! Bounded local destination payload contracts.
//!
//! Plan 120 §10 defines only the local message contracts Plan 122 will consume.
//! There is deliberately no streaming protocol, no Garlic wrapping, and no
//! plaintext tunnel-delivery shortcut: an accepted outbound payload is retained
//! in a bounded queue and reported as not routable until the Plan 121 ECIES
//! Garlic session layer and the Plan 122 destination routing land.

use core::fmt;
use std::collections::VecDeque;

/// Hard ceiling on a single destination payload body.
pub const MAX_DESTINATION_PAYLOAD_BYTES: usize = 32 * 1024;

/// A bounded application payload handed to or received from a destination.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DestinationPayload {
    protocol: u8,
    bytes: Vec<u8>,
}

impl DestinationPayload {
    /// Constructs a bounded payload.
    pub fn new(protocol: u8, bytes: Vec<u8>) -> Result<Self, PayloadError> {
        if bytes.is_empty() {
            return Err(PayloadError::EmptyBody);
        }
        if bytes.len() > MAX_DESTINATION_PAYLOAD_BYTES {
            return Err(PayloadError::BodyTooLarge {
                actual: bytes.len(),
                maximum: MAX_DESTINATION_PAYLOAD_BYTES,
            });
        }
        Ok(Self { protocol, bytes })
    }

    /// I2P datagram/streaming protocol discriminator.
    pub const fn protocol(&self) -> u8 {
        self.protocol
    }

    /// Borrows the payload body.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Payload body length in bytes.
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    /// Whether the payload body is empty. Always `false` for a constructed
    /// payload; retained for API symmetry with `len`.
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
}

/// Why a queued outbound payload cannot yet be routed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RoutingUnavailable {
    /// The ECIES Garlic session layer (Plan 121) is not implemented, so the
    /// payload cannot be sealed for a remote destination.
    AwaitingGarlicSessionLayer,
    /// Remote LeaseSet2 resolution and destination routing (Plan 122) are not
    /// implemented.
    AwaitingDestinationRouting,
}

impl fmt::Display for RoutingUnavailable {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AwaitingGarlicSessionLayer => {
                formatter.write_str("routing unavailable: awaiting Plan 121 Garlic session layer")
            }
            Self::AwaitingDestinationRouting => {
                formatter.write_str("routing unavailable: awaiting Plan 122 destination routing")
            }
        }
    }
}

/// Disposition of an accepted outbound payload.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QueuedOutbound {
    /// Queue depth after the payload was accepted.
    pub queued_messages: usize,
    /// Aggregate queued bytes after the payload was accepted.
    pub queued_bytes: usize,
    /// Why the payload is not routable yet.
    pub routing: RoutingUnavailable,
}

/// Bounded FIFO payload queue with explicit item and byte ceilings.
#[derive(Debug)]
pub struct BoundedPayloadQueue {
    max_messages: usize,
    max_bytes: usize,
    queued_bytes: usize,
    entries: VecDeque<DestinationPayload>,
}

impl BoundedPayloadQueue {
    /// Constructs a bounded queue.
    pub fn new(max_messages: u16, max_bytes: usize) -> Self {
        Self {
            max_messages: usize::from(max_messages),
            max_bytes,
            queued_bytes: 0,
            entries: VecDeque::new(),
        }
    }

    /// Pushes a payload, rejecting it when either ceiling would be exceeded.
    pub fn push(&mut self, payload: DestinationPayload) -> Result<(), PayloadError> {
        if self.entries.len() >= self.max_messages {
            return Err(PayloadError::QueueFull {
                queued: self.entries.len(),
                maximum: self.max_messages,
            });
        }
        let projected = self.queued_bytes.saturating_add(payload.len());
        if projected > self.max_bytes {
            return Err(PayloadError::QueueBytesExceeded {
                projected,
                maximum: self.max_bytes,
            });
        }
        self.queued_bytes = projected;
        self.entries.push_back(payload);
        Ok(())
    }

    /// Pops the oldest payload, releasing its byte accounting.
    pub fn pop(&mut self) -> Option<DestinationPayload> {
        let payload = self.entries.pop_front()?;
        self.queued_bytes = self.queued_bytes.saturating_sub(payload.len());
        Some(payload)
    }

    /// Number of queued payloads.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the queue is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Aggregate queued bytes.
    pub const fn queued_bytes(&self) -> usize {
        self.queued_bytes
    }

    /// Configured maximum payload count.
    pub const fn max_messages(&self) -> usize {
        self.max_messages
    }

    /// Configured maximum aggregate bytes.
    pub const fn max_bytes(&self) -> usize {
        self.max_bytes
    }

    /// Releases every queued payload, returning the number dropped.
    pub fn release_all(&mut self) -> usize {
        let released = self.entries.len();
        self.entries.clear();
        self.queued_bytes = 0;
        released
    }
}

/// Typed payload and queue failures.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum PayloadError {
    /// The payload body was empty.
    #[error("destination payload body was empty")]
    EmptyBody,
    /// The payload body exceeded the hard ceiling.
    #[error("destination payload body {actual} exceeds maximum {maximum}")]
    BodyTooLarge {
        /// Supplied length.
        actual: usize,
        /// Accepted ceiling.
        maximum: usize,
    },
    /// The queue already holds the maximum number of payloads.
    #[error("destination payload queue full: {queued} of {maximum}")]
    QueueFull {
        /// Current depth.
        queued: usize,
        /// Accepted ceiling.
        maximum: usize,
    },
    /// The queue's aggregate byte ceiling would be exceeded.
    #[error("destination payload queue bytes {projected} exceeds maximum {maximum}")]
    QueueBytesExceeded {
        /// Projected aggregate bytes.
        projected: usize,
        /// Accepted ceiling.
        maximum: usize,
    },
    /// The destination is stopping and no longer accepts payloads.
    #[error("destination is stopping and no longer accepts payloads")]
    Stopping,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn payload(len: usize) -> DestinationPayload {
        DestinationPayload::new(6, vec![0x41; len]).expect("payload")
    }

    #[test]
    fn payload_bodies_are_bounded() {
        assert_eq!(
            DestinationPayload::new(6, Vec::new()),
            Err(PayloadError::EmptyBody)
        );
        assert_eq!(
            DestinationPayload::new(6, vec![0; MAX_DESTINATION_PAYLOAD_BYTES + 1]),
            Err(PayloadError::BodyTooLarge {
                actual: MAX_DESTINATION_PAYLOAD_BYTES + 1,
                maximum: MAX_DESTINATION_PAYLOAD_BYTES,
            })
        );
        let accepted = payload(16);
        assert_eq!(accepted.protocol(), 6);
        assert_eq!(accepted.len(), 16);
        assert!(!accepted.is_empty());
    }

    #[test]
    fn queue_enforces_message_and_byte_ceilings() {
        let mut queue = BoundedPayloadQueue::new(2, 64);
        queue.push(payload(16)).expect("first");
        queue.push(payload(16)).expect("second");
        assert_eq!(
            queue.push(payload(16)),
            Err(PayloadError::QueueFull {
                queued: 2,
                maximum: 2
            })
        );
        assert_eq!(queue.queued_bytes(), 32);

        let mut byte_bound = BoundedPayloadQueue::new(8, 24);
        byte_bound.push(payload(16)).expect("first");
        assert_eq!(
            byte_bound.push(payload(16)),
            Err(PayloadError::QueueBytesExceeded {
                projected: 32,
                maximum: 24
            })
        );
    }

    #[test]
    fn pop_and_release_return_byte_accounting() {
        let mut queue = BoundedPayloadQueue::new(4, 128);
        queue.push(payload(10)).expect("first");
        queue.push(payload(20)).expect("second");
        assert_eq!(queue.queued_bytes(), 30);
        assert_eq!(queue.pop().expect("pop").len(), 10);
        assert_eq!(queue.queued_bytes(), 20);
        assert_eq!(queue.release_all(), 1);
        assert_eq!(queue.queued_bytes(), 0);
        assert!(queue.is_empty());
        assert!(queue.pop().is_none());
        assert_eq!(queue.max_messages(), 4);
        assert_eq!(queue.max_bytes(), 128);
    }

    #[test]
    fn routing_unavailable_renders_the_owning_plan() {
        assert!(
            RoutingUnavailable::AwaitingGarlicSessionLayer
                .to_string()
                .contains("Plan 121")
        );
        assert!(
            RoutingUnavailable::AwaitingDestinationRouting
                .to_string()
                .contains("Plan 122")
        );
    }
}
