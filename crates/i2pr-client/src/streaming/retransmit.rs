#![allow(dead_code)]

//! Retransmission timer and RTT estimation.
//!
//! The retransmit policy tracks per-packet send timestamps, estimates
//! RTT from newly acknowledged non-retransmitted packets, and
//! computes retransmission timeouts with exponential backoff.

use crate::streaming::config::StreamingConfig;

/// Maximum retransmission attempts per packet.
pub const MAX_RETRANSMIT_ATTEMPTS: u32 = 16;

/// Minimum retransmission timeout in milliseconds.
pub const MIN_RTO_MILLIS: u64 = 100;

/// Maximum retransmission timeout in milliseconds.
pub const MAX_RTO_MILLIS: u64 = 60_000;

/// A sampled RTT measurement from a newly acknowledged packet.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RttSample {
    /// Measured RTT in milliseconds.
    pub rtt_ms: u64,
    /// Sequence number that was acknowledged.
    pub sequence: u32,
    /// Whether this was a retransmitted packet (retransmit samples
    /// are not used for RTT estimation).
    pub was_retransmit: bool,
}

/// Configuration for the retransmit policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetransmitConfig {
    /// Initial retransmission timeout in milliseconds.
    pub initial_rto_ms: u64,
    /// Minimum retransmission timeout in milliseconds.
    pub min_rto_ms: u64,
    /// Maximum retransmission timeout in milliseconds.
    pub max_rto_ms: u64,
    /// Maximum retransmission attempts.
    pub max_attempts: u32,
}

impl RetransmitConfig {
    /// Builds a retransmit config from the streaming config.
    pub fn from_config(config: &StreamingConfig) -> Self {
        Self {
            initial_rto_ms: 5_000,
            min_rto_ms: MIN_RTO_MILLIS,
            max_rto_ms: MAX_RTO_MILLIS.min(config.close_timeout_ms / 2),
            max_attempts: config.max_retransmit_count as u32,
        }
    }
}

/// Decision returned by the retransmit policy for a given packet.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RetransmitDecision {
    /// The packet should be retransmitted now.
    Retransmit {
        /// Sequence number to retransmit.
        sequence: u32,
    },
    /// The packet's retransmission budget is exhausted and it should
    /// be dropped.
    Drop {
        /// Sequence number whose budget was exhausted.
        sequence: u32,
    },
    /// No action needed; the packet is still within its RTO window.
    Wait,
}

/// Manages per-packet retransmission timers using a simple
/// exponential backoff with RTT sampling.
#[derive(Debug)]
pub struct RetransmitPolicy {
    config: RetransmitConfig,
    /// Estimated smoothed RTT in milliseconds.
    smoothed_rtt_ms: u64,
    /// Estimated RTT variance in milliseconds.
    rtt_variance_ms: u64,
    /// Current RTO in milliseconds.
    current_rto_ms: u64,
}

impl RetransmitPolicy {
    /// Creates a new retransmit policy.
    pub fn new(config: RetransmitConfig) -> Self {
        Self {
            current_rto_ms: config.initial_rto_ms,
            config,
            smoothed_rtt_ms: config.initial_rto_ms,
            rtt_variance_ms: config.initial_rto_ms / 2,
        }
    }

    /// Creates a new retransmit policy from the streaming config.
    pub fn from_streaming_config(config: &StreamingConfig) -> Self {
        Self::new(RetransmitConfig::from_config(config))
    }

    /// Evaluates whether a packet should be retransmitted based on
    /// the current time and the packet's send timestamp.
    pub fn evaluate(
        &self,
        sent_at_ms: u64,
        retransmit_count: u32,
        now_ms: u64,
    ) -> RetransmitDecision {
        if retransmit_count >= self.config.max_attempts {
            return RetransmitDecision::Wait;
        }
        let elapsed = now_ms.saturating_sub(sent_at_ms);
        if elapsed >= self.current_rto_ms {
            RetransmitDecision::Retransmit { sequence: 0 }
        } else {
            RetransmitDecision::Wait
        }
    }

    /// Updates the RTT estimate with a new sample.
    pub fn record_rtt_sample(&mut self, sample: &RttSample) {
        if sample.was_retransmit {
            return;
        }
        let rtt = sample.rtt_ms;
        // Exponential moving average: SRTT = (1 - alpha) * SRTT + alpha * R
        // alpha = 1/8
        let alpha_numerator = 1u64;
        let alpha_denominator = 8u64;
        self.smoothed_rtt_ms = (self.smoothed_rtt_ms * (alpha_denominator - alpha_numerator)
            + rtt * alpha_numerator)
            / alpha_denominator;
        // RTTVAR = (1 - beta) * RTTVAR + beta * |SRTT - R|
        // beta = 1/4
        let diff = rtt.abs_diff(self.smoothed_rtt_ms);
        let beta_numerator = 1u64;
        let beta_denominator = 4u64;
        self.rtt_variance_ms = (self.rtt_variance_ms * (beta_denominator - beta_numerator)
            + diff * beta_numerator)
            / beta_denominator;
        // RTO = SRTT + max(G, 4 * RTTVAR)
        let g = 0; // clock granularity
        let backoff = (4 * self.rtt_variance_ms).max(g);
        self.current_rto_ms = (self.smoothed_rtt_ms.saturating_add(backoff))
            .max(self.config.min_rto_ms)
            .min(self.config.max_rto_ms);
    }

    /// Computes the current RTO with exponential backoff for a
    /// retransmitted packet.
    pub fn rto_for_retransmit(&self, retransmit_count: u32) -> u64 {
        let mut rto = self.current_rto_ms;
        for _ in 0..retransmit_count {
            rto = rto.saturating_mul(2).min(self.config.max_rto_ms);
        }
        rto.max(self.config.min_rto_ms)
    }

    /// Returns the current estimated RTO.
    pub fn current_rto_ms(&self) -> u64 {
        self.current_rto_ms
    }

    /// Returns the current smoothed RTT.
    pub fn smoothed_rtt_ms(&self) -> u64 {
        self.smoothed_rtt_ms
    }
}
