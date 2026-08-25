#![allow(dead_code)]

//! Streaming configuration with hard bounds.
//!
//! Plan 123 defines the mandatory resource limits for the minimal
//! streaming core. Every bound is enforced at configuration time so
//! the runtime never needs to check ceilings after construction.

/// Maximum application payload bytes inside one streaming packet.
pub const MAX_PACKET_PAYLOAD_BYTES: usize = i2pr_proto::streaming::MAX_STREAMING_PAYLOAD_BYTES;

/// Minimum negotiated streaming payload bytes per packet. A peer
/// negotiation may not reduce the per-packet payload below this floor.
pub const MIN_STREAMING_PAYLOAD_BYTES_PER_PACKET: usize = 128;

/// Maximum negotiated streaming payload bytes per packet. The
/// initial SYN advertises the local ceiling; the established
/// connection uses `min(local, remote)`.
pub const MAX_STREAMING_PAYLOAD_BYTES_PER_PACKET: usize = MAX_PACKET_PAYLOAD_BYTES;

/// Hard ceiling on the number of inbound pending streams per local
/// destination.
pub const MAX_INBOUND_PENDING_STREAMS: usize = 64;

/// Hard ceiling on the number of outbound pending streams per local
/// destination.
pub const MAX_OUTBOUND_PENDING_STREAMS: usize = 64;

/// Hard ceiling on the number of active streams per local destination.
pub const MAX_STREAMS_PER_DESTINATION_LIMIT: usize = 256;

/// Hard ceiling on the listener accept backlog per local port.
pub const MAX_LISTENER_BACKLOG: usize = 64;

/// Hard ceiling on the send window in packets.
pub const MAX_SEND_WINDOW_PACKETS: usize = 128;

/// Hard ceiling on the receive reorder window in packets.
pub const MAX_RECV_WINDOW_PACKETS: usize = 128;

/// Hard ceiling on the number of unacked packets.
pub const MAX_UNACKED_PACKETS: usize = 256;

/// Hard ceiling on the maximum retransmission count per packet.
pub const MAX_RETRANSMIT_COUNT: usize = 16;

/// Hard ceiling on the number of pre-SYN unknown-stream reorder
/// packets the manager retains.
pub const MAX_PRE_SYN_BUFFER: usize = 8;

/// Default connection setup timeout in milliseconds.
pub const DEFAULT_SETUP_TIMEOUT_MS: u64 = 30_000;

/// Default idle timeout in milliseconds.
pub const DEFAULT_IDLE_TIMEOUT_MS: u64 = 300_000;

/// Default close timeout in milliseconds.
pub const DEFAULT_CLOSE_TIMEOUT_MS: u64 = 10_000;

/// Default delayed-ACK deadline in milliseconds (Plan 130 §7 D3).
/// The current I2P reference default is 750 ms
/// (`i2p.streaming.initialAckDelay`); a receiver coalesces the
/// acknowledgements of packets arriving inside one deadline into a
/// single standalone ACK.
pub const DEFAULT_DELAYED_ACK_MS: u64 = 750;

/// Hard ceiling on the delayed-ACK deadline. The Streaming
/// specification bounds advisory delay values at 60000 ms.
pub const MAX_DELAYED_ACK_MS: u64 = 60_000;

/// Configuration for the streaming layer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StreamingConfig {
    /// Maximum streams per local destination.
    pub max_streams_per_destination: u16,
    /// Maximum inbound pending streams per local port.
    pub max_inbound_pending: u16,
    /// Maximum outbound pending streams.
    pub max_outbound_pending: u16,
    /// Maximum listener backlog per port.
    pub max_listener_backlog: u16,
    /// Maximum send window in packets.
    pub max_send_window_packets: u16,
    /// Maximum receive reorder window in packets.
    pub max_recv_window_packets: u16,
    /// Maximum unacked packets.
    pub max_unacked_packets: u16,
    /// Maximum retransmission attempts per packet.
    pub max_retransmit_count: u8,
    /// Connection setup timeout in milliseconds.
    pub setup_timeout_ms: u64,
    /// Idle timeout in milliseconds.
    pub idle_timeout_ms: u64,
    /// Close timeout in milliseconds.
    pub close_timeout_ms: u64,
    /// Delayed-ACK deadline in milliseconds (Plan 130 §7 D3).
    pub delayed_ack_ms: u64,
    /// Maximum pre-SYN reorder buffer size.
    pub max_pre_syn_buffer: u16,
}

impl StreamingConfig {
    /// Constructs a configuration after applying every ceiling.
    #[allow(clippy::too_many_arguments)]
    pub const fn try_new(
        max_streams_per_destination: u16,
        max_inbound_pending: u16,
        max_outbound_pending: u16,
        max_listener_backlog: u16,
        max_send_window_packets: u16,
        max_recv_window_packets: u16,
        max_unacked_packets: u16,
        max_retransmit_count: u8,
        setup_timeout_ms: u64,
        idle_timeout_ms: u64,
        close_timeout_ms: u64,
        delayed_ack_ms: u64,
        max_pre_syn_buffer: u16,
    ) -> Result<Self, StreamingConfigError> {
        if max_streams_per_destination == 0 {
            return Err(StreamingConfigError::ZeroMaxStreamsPerDestination);
        }
        if (max_streams_per_destination as usize) > MAX_STREAMS_PER_DESTINATION_LIMIT {
            return Err(StreamingConfigError::MaxStreamsExceedsLimit {
                actual: max_streams_per_destination,
                maximum: MAX_STREAMS_PER_DESTINATION_LIMIT as u16,
            });
        }
        if max_inbound_pending == 0 {
            return Err(StreamingConfigError::ZeroMaxInboundPending);
        }
        if (max_inbound_pending as usize) > MAX_INBOUND_PENDING_STREAMS {
            return Err(StreamingConfigError::InboundPendingExceedsLimit {
                actual: max_inbound_pending,
                maximum: MAX_INBOUND_PENDING_STREAMS as u16,
            });
        }
        if max_outbound_pending == 0 {
            return Err(StreamingConfigError::ZeroMaxOutboundPending);
        }
        if (max_outbound_pending as usize) > MAX_OUTBOUND_PENDING_STREAMS {
            return Err(StreamingConfigError::OutboundPendingExceedsLimit {
                actual: max_outbound_pending,
                maximum: MAX_OUTBOUND_PENDING_STREAMS as u16,
            });
        }
        if (max_listener_backlog as usize) > MAX_LISTENER_BACKLOG {
            return Err(StreamingConfigError::ListenerBacklogExceedsLimit {
                actual: max_listener_backlog,
                maximum: MAX_LISTENER_BACKLOG as u16,
            });
        }
        if max_send_window_packets == 0 {
            return Err(StreamingConfigError::ZeroSendWindow);
        }
        if (max_send_window_packets as usize) > MAX_SEND_WINDOW_PACKETS {
            return Err(StreamingConfigError::SendWindowExceedsLimit {
                actual: max_send_window_packets,
                maximum: MAX_SEND_WINDOW_PACKETS as u16,
            });
        }
        if max_recv_window_packets == 0 {
            return Err(StreamingConfigError::ZeroRecvWindow);
        }
        if (max_recv_window_packets as usize) > MAX_RECV_WINDOW_PACKETS {
            return Err(StreamingConfigError::RecvWindowExceedsLimit {
                actual: max_recv_window_packets,
                maximum: MAX_RECV_WINDOW_PACKETS as u16,
            });
        }
        if max_unacked_packets == 0 {
            return Err(StreamingConfigError::ZeroMaxUnacked);
        }
        if (max_unacked_packets as usize) > MAX_UNACKED_PACKETS {
            return Err(StreamingConfigError::UnackedExceedsLimit {
                actual: max_unacked_packets,
                maximum: MAX_UNACKED_PACKETS as u16,
            });
        }
        if max_retransmit_count == 0 {
            return Err(StreamingConfigError::ZeroMaxRetransmit);
        }
        if (max_retransmit_count as usize) > MAX_RETRANSMIT_COUNT {
            return Err(StreamingConfigError::RetransmitExceedsLimit {
                actual: max_retransmit_count,
                maximum: MAX_RETRANSMIT_COUNT as u8,
            });
        }
        if setup_timeout_ms == 0 {
            return Err(StreamingConfigError::ZeroSetupTimeout);
        }
        if idle_timeout_ms == 0 {
            return Err(StreamingConfigError::ZeroIdleTimeout);
        }
        if close_timeout_ms == 0 {
            return Err(StreamingConfigError::ZeroCloseTimeout);
        }
        if delayed_ack_ms == 0 {
            return Err(StreamingConfigError::ZeroDelayedAck);
        }
        if delayed_ack_ms > MAX_DELAYED_ACK_MS {
            return Err(StreamingConfigError::DelayedAckExceedsLimit {
                actual: delayed_ack_ms,
                maximum: MAX_DELAYED_ACK_MS,
            });
        }
        if (max_pre_syn_buffer as usize) > MAX_PRE_SYN_BUFFER {
            return Err(StreamingConfigError::PreSynBufferExceedsLimit {
                actual: max_pre_syn_buffer,
                maximum: MAX_PRE_SYN_BUFFER as u16,
            });
        }
        Ok(Self {
            max_streams_per_destination,
            max_inbound_pending,
            max_outbound_pending,
            max_listener_backlog,
            max_send_window_packets,
            max_recv_window_packets,
            max_unacked_packets,
            max_retransmit_count,
            setup_timeout_ms,
            idle_timeout_ms,
            close_timeout_ms,
            delayed_ack_ms,
            max_pre_syn_buffer,
        })
    }

    /// Returns a balanced experimental default.
    pub fn balanced() -> Self {
        Self::try_new(
            64,
            32,
            32,
            16,
            64,
            64,
            128,
            8,
            DEFAULT_SETUP_TIMEOUT_MS,
            DEFAULT_IDLE_TIMEOUT_MS,
            DEFAULT_CLOSE_TIMEOUT_MS,
            DEFAULT_DELAYED_ACK_MS,
            4,
        )
        .expect("balanced streaming config is within every ceiling")
    }
}

impl Default for StreamingConfig {
    fn default() -> Self {
        Self::balanced()
    }
}

/// Typed configuration validation failures.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum StreamingConfigError {
    #[error("streaming max streams per destination must be nonzero")]
    ZeroMaxStreamsPerDestination,
    #[error("streaming max streams per destination {actual} exceeds limit {maximum}")]
    MaxStreamsExceedsLimit { actual: u16, maximum: u16 },
    #[error("streaming max inbound pending must be nonzero")]
    ZeroMaxInboundPending,
    #[error("streaming max inbound pending {actual} exceeds limit {maximum}")]
    InboundPendingExceedsLimit { actual: u16, maximum: u16 },
    #[error("streaming max outbound pending must be nonzero")]
    ZeroMaxOutboundPending,
    #[error("streaming max outbound pending {actual} exceeds limit {maximum}")]
    OutboundPendingExceedsLimit { actual: u16, maximum: u16 },
    #[error("streaming listener backlog {actual} exceeds limit {maximum}")]
    ListenerBacklogExceedsLimit { actual: u16, maximum: u16 },
    #[error("streaming send window must be nonzero")]
    ZeroSendWindow,
    #[error("streaming send window {actual} exceeds limit {maximum}")]
    SendWindowExceedsLimit { actual: u16, maximum: u16 },
    #[error("streaming recv window must be nonzero")]
    ZeroRecvWindow,
    #[error("streaming recv window {actual} exceeds limit {maximum}")]
    RecvWindowExceedsLimit { actual: u16, maximum: u16 },
    #[error("streaming max unacked must be nonzero")]
    ZeroMaxUnacked,
    #[error("streaming max unacked {actual} exceeds limit {maximum}")]
    UnackedExceedsLimit { actual: u16, maximum: u16 },
    #[error("streaming max retransmit must be nonzero")]
    ZeroMaxRetransmit,
    #[error("streaming max retransmit {actual} exceeds limit {maximum}")]
    RetransmitExceedsLimit { actual: u8, maximum: u8 },
    #[error("streaming setup timeout must be nonzero")]
    ZeroSetupTimeout,
    #[error("streaming idle timeout must be nonzero")]
    ZeroIdleTimeout,
    #[error("streaming close timeout must be nonzero")]
    ZeroCloseTimeout,
    #[error("streaming delayed ack must be nonzero")]
    ZeroDelayedAck,
    #[error("streaming delayed ack {actual} ms exceeds limit {maximum} ms")]
    DelayedAckExceedsLimit { actual: u64, maximum: u64 },
    #[error("streaming pre-syn buffer {actual} exceeds limit {maximum}")]
    PreSynBufferExceedsLimit { actual: u16, maximum: u16 },
}
