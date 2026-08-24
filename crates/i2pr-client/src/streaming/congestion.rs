#![allow(dead_code)]

//! Congestion and window control.
//!
//! The congestion policy controls how many packets may be in flight
//! simultaneously. The initial implementation uses a simple
//! additive-increase / multiplicative-decrease (AIMD) scheme that is
//! stable under ordinary loss and never sends unbounded
//! retransmissions.

use crate::streaming::config::StreamingConfig;

/// Initial congestion window in packets.
pub const INITIAL_CONGESTION_WINDOW: u32 = 16;

/// Maximum congestion window in packets.
pub const MAX_CONGESTION_WINDOW: u32 = 256;

/// Minimum congestion window in packets.
pub const MIN_CONGESTION_WINDOW: u32 = 1;

/// Configuration for the congestion policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CongestionConfig {
    /// Initial congestion window.
    pub initial_window: u32,
    /// Maximum congestion window.
    pub max_window: u32,
    /// Minimum congestion window.
    pub min_window: u32,
}

impl CongestionConfig {
    /// Builds a congestion config from the streaming config.
    pub fn from_config(config: &StreamingConfig) -> Self {
        let initial = INITIAL_CONGESTION_WINDOW.min(config.max_send_window_packets as u32);
        let max = (config.max_send_window_packets as u32).min(MAX_CONGESTION_WINDOW);
        let max = max.max(initial).max(MIN_CONGESTION_WINDOW);
        Self {
            initial_window: initial,
            max_window: max,
            min_window: MIN_CONGESTION_WINDOW,
        }
    }
}

/// Decision returned by the congestion policy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CongestionDecision {
    /// The window allows sending the given number of additional
    /// packets.
    Allow {
        /// Number of additional packets allowed.
        additional_packets: u32,
    },
    /// The window is exhausted; no new packets may be sent.
    Full,
}

/// Manages the congestion window. Tracks acknowledged packets,
/// increases the window on progress, and reduces it on loss.
#[derive(Debug)]
pub struct CongestionPolicy {
    config: CongestionConfig,
    /// Current congestion window in packets.
    current_window: u32,
    /// Number of packets acknowledged since last window increase.
    acked_since_increase: u32,
    /// Number of packets in flight.
    in_flight: u32,
}

impl CongestionPolicy {
    /// Creates a new congestion policy.
    pub fn new(config: CongestionConfig) -> Self {
        Self {
            current_window: config.initial_window,
            config,
            acked_since_increase: 0,
            in_flight: 0,
        }
    }

    /// Creates a new congestion policy from the streaming config.
    pub fn from_streaming_config(config: &StreamingConfig) -> Self {
        Self::new(CongestionConfig::from_config(config))
    }

    /// Evaluates whether additional packets may be sent.
    pub fn evaluate(&self) -> CongestionDecision {
        let available = self.current_window.saturating_sub(self.in_flight);
        if available > 0 {
            CongestionDecision::Allow {
                additional_packets: available,
            }
        } else {
            CongestionDecision::Full
        }
    }

    /// Records that a packet was sent (increases in-flight count).
    pub fn record_sent(&mut self) {
        self.in_flight = self.in_flight.saturating_add(1);
    }

    /// Records that a packet was acknowledged (decreases in-flight
    /// count and potentially increases the window).
    pub fn record_acked(&mut self) {
        self.in_flight = self.in_flight.saturating_sub(1);
        self.acked_since_increase = self.acked_since_increase.saturating_add(1);
        // Additive increase: grow window by 1 after every full window
        // of acks.
        if self.acked_since_increase >= self.current_window {
            self.current_window =
                (self.current_window.saturating_add(1)).min(self.config.max_window);
            self.acked_since_increase = 0;
        }
    }

    /// Records a loss event (multiplicative decrease).
    pub fn record_loss(&mut self) {
        let halved = self.current_window.max(self.config.min_window) / 2;
        self.current_window = halved.max(self.config.min_window);
        self.acked_since_increase = 0;
    }

    /// Returns the current congestion window.
    pub fn current_window(&self) -> u32 {
        self.current_window
    }

    /// Returns the number of packets currently in flight.
    pub fn in_flight(&self) -> u32 {
        self.in_flight
    }
}
