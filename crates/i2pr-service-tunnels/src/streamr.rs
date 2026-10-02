//! Plan 291 runtime-neutral Streamr profile options.
//!
//! Java I2PTunnel Streamr (`net.i2p.i2ptunnel.streamr`) composes a
//! repliable-datagram subscriber control plane (one-byte subscribe
//! `0x00` / unsubscribe `0x01` over Datagram1, protocol 17) with an
//! unauthenticated raw-datagram media plane (protocol 18) under a
//! bounded subscriber table. This module owns the validated option
//! surface only: refresh cadence, subscription expiry, subscriber
//! ceiling, application payload ceiling, and the loopback UDP
//! endpoints. No sockets, no timers, no Tokio; the daemon owns all
//! UDP I/O. See `specs/protocols/12-repliable-datagrams-streamr.md`
//! for the frozen behavior reference.

#![forbid(unsafe_code)]

use std::net::SocketAddr;

use crate::ServiceTunnelError;

/// Default subscribe refresh: first five at 2 s, then steady
/// 10 s (Java `Pinger` cadence).
pub const DEFAULT_SUBSCRIBE_INTERVAL_MS: u64 = 10_000;
/// Fast-start subscribe interval for the first five subscribes.
pub const SUBSCRIBE_FAST_START_INTERVAL_MS: u64 = 2_000;
/// Number of fast-start subscribes before the steady cadence.
pub const SUBSCRIBE_FAST_START_COUNT: usize = 5;
/// Default subscription expiry from the last refresh (Java
/// `Subscriber::EXPIRATION`).
pub const DEFAULT_SUBSCRIPTION_EXPIRY_MS: u64 = 60_000;
/// Default subscriber ceiling (Java `MAX_SUBSCRIPTIONS`).
pub const DEFAULT_MAX_SUBSCRIBERS: usize = 10;
/// Default application payload ceiling in bytes (fork donor
/// value, inside the datagram reliability guidance).
pub const DEFAULT_PAYLOAD_LIMIT_BYTES: usize = 1200;
/// Hard application payload ceiling: one UDP read is at most one
/// I2P datagram, never fragmented.
pub const MAX_PAYLOAD_LIMIT_BYTES: usize = 1200;
/// Hard subscriber ceiling: the table is a bounded map, never
/// peer-grown.
pub const MAX_SUBSCRIBER_CEILING: usize = 64;
/// Slowest accepted subscribe refresh (must stay usable against
/// the shortest accepted expiry).
pub const MAX_SUBSCRIBE_INTERVAL_MS: u64 = 30_000;
/// Shortest accepted subscription expiry (must exceed any fast
/// refresh burst).
pub const MIN_SUBSCRIPTION_EXPIRY_MS: u64 = 10_000;
/// Longest accepted subscription expiry.
pub const MAX_SUBSCRIPTION_EXPIRY_MS: u64 = 300_000;
/// I2P destination port used on subscribe/media packets when the
/// service does not name one (Java `toPort = 0`).
pub const DEFAULT_STREAMR_I2P_PORT: u16 = 0;

/// Validated Streamr profile options shared by the subscriber
/// (client) and publisher (server) halves. Per-kind required
/// fields are enforced by `ServiceTunnelSpec::validate`, not here.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StreamrOptions {
    /// Loopback UDP endpoint: the media source the router reads
    /// (server half) or the media target the router writes
    /// (client half). Direction follows the kind; the name is
    /// router-centric and matches the control inventory mask.
    pub local_udp: Option<SocketAddr>,
    /// I2P destination port carried on subscribe/media packets.
    pub target_i2p_port: u16,
    /// Steady subscribe refresh interval in milliseconds (client
    /// half; fast-start is fixed by the freeze).
    pub subscribe_interval_ms: u64,
    /// Subscription expiry in milliseconds from the last refresh
    /// (server half).
    pub subscription_expiry_ms: u64,
    /// Subscriber table ceiling (server half).
    pub max_subscribers: usize,
    /// Application payload ceiling in bytes (both halves).
    pub payload_limit_bytes: usize,
}

impl Default for StreamrOptions {
    fn default() -> Self {
        Self {
            local_udp: None,
            target_i2p_port: DEFAULT_STREAMR_I2P_PORT,
            subscribe_interval_ms: DEFAULT_SUBSCRIBE_INTERVAL_MS,
            subscription_expiry_ms: DEFAULT_SUBSCRIPTION_EXPIRY_MS,
            max_subscribers: DEFAULT_MAX_SUBSCRIBERS,
            payload_limit_bytes: DEFAULT_PAYLOAD_LIMIT_BYTES,
        }
    }
}

impl StreamrOptions {
    /// Validates ranges and loopback shape. Cross-half presence
    /// (which endpoint must be set) is enforced per kind by the
    /// spec validator.
    pub fn validate(&self) -> Result<(), ServiceTunnelError> {
        if !(1_000..=MAX_SUBSCRIBE_INTERVAL_MS).contains(&self.subscribe_interval_ms) {
            return Err(ServiceTunnelError::ExceedsCeiling {
                field: "subscribe_interval_ms",
                reason: "must be within 1000..=30000",
            });
        }
        if !(MIN_SUBSCRIPTION_EXPIRY_MS..=MAX_SUBSCRIPTION_EXPIRY_MS)
            .contains(&self.subscription_expiry_ms)
        {
            return Err(ServiceTunnelError::ExceedsCeiling {
                field: "subscription_expiry_ms",
                reason: "must be within 10000..=300000",
            });
        }
        if self.max_subscribers == 0 || self.max_subscribers > MAX_SUBSCRIBER_CEILING {
            return Err(ServiceTunnelError::ExceedsCeiling {
                field: "max_subscribers",
                reason: "must be within 1..=64",
            });
        }
        if self.payload_limit_bytes == 0 || self.payload_limit_bytes > MAX_PAYLOAD_LIMIT_BYTES {
            return Err(ServiceTunnelError::ExceedsCeiling {
                field: "payload_limit_bytes",
                reason: "must be within 1..=1200",
            });
        }
        for endpoint in [self.local_udp] {
            if let Some(addr) = endpoint
                && !addr.ip().is_loopback()
            {
                return Err(ServiceTunnelError::InvalidTarget {
                    value: addr.to_string(),
                    reason: "streamr UDP endpoints must be loopback",
                });
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_freeze() {
        let options = StreamrOptions::default();
        assert_eq!(options.subscribe_interval_ms, 10_000);
        assert_eq!(options.subscription_expiry_ms, 60_000);
        assert_eq!(options.max_subscribers, 10);
        assert_eq!(options.payload_limit_bytes, 1200);
        options.validate().expect("defaults validate");
    }

    #[test]
    fn non_loopback_udp_is_rejected() {
        let options = StreamrOptions {
            local_udp: Some("192.0.2.1:5000".parse().expect("addr")),
            ..StreamrOptions::default()
        };
        assert!(options.validate().is_err());
    }

    #[test]
    fn ceilings_are_enforced() {
        let options = StreamrOptions {
            payload_limit_bytes: 1201,
            ..StreamrOptions::default()
        };
        assert!(options.validate().is_err());
        let options = StreamrOptions {
            max_subscribers: 65,
            ..StreamrOptions::default()
        };
        assert!(options.validate().is_err());
        let options = StreamrOptions {
            subscribe_interval_ms: 999,
            ..StreamrOptions::default()
        };
        assert!(options.validate().is_err());
    }
}
