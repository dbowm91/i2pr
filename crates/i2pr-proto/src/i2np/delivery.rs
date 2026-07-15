//! Delivery-status body structure.

use super::*;

/// DeliveryStatus message body.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeliveryStatusMessage {
    /// ID of the delivered message.
    pub message_id: u32,
    /// Creation or arrival time; freshness is deferred.
    pub timestamp: Date,
}

impl DeliveryStatusMessage {
    /// Creates a delivery-status body.
    pub const fn new(message_id: u32, timestamp: Date) -> Self {
        Self {
            message_id,
            timestamp,
        }
    }
}
