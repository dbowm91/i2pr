//! Plan 292 server inbound peer allow/deny policy (runtime-neutral).
//!
//! `access_list` and `white_list` unite into the allow set;
//! `black_list` is the deny set. Entries are canonical Base32
//! destination hashes (52 characters, with or without the
//! `.b32.i2p` suffix): the control boundary cannot resolve `.i2p`
//! names or full destinations synchronously, so anything else is
//! rejected with a static reason instead of being resolved late or
//! guessed. Matching is exact on the 32-byte peer hash observed at
//! inbound accept: denied hashes always lose, an empty allow set
//! admits everyone not denied, and a non-empty allow set admits
//! only its members.

#![forbid(unsafe_code)]

use crate::destination::B32_SUFFIX;
use crate::errors::ServiceTunnelError;
use std::collections::BTreeMap;

/// Maximum entries in one allow/deny list.
pub const MAX_ACCESS_LIST_ENTRIES: usize = 64;
/// Maximum distinct peers retained by the bounded inbound-rate owner.
pub const MAX_RATE_LIMIT_PEERS: usize = 4096;
const RATE_WINDOWS_MS: [u64; 3] = [60_000, 3_600_000, 86_400_000];
const MAX_PROPOSAL_RATE: u32 = 100_000;

/// Proposal server connection-rate limits. Zero disables a limit.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ServerConnectionRateLimits {
    /// Maximum accepted connections per peer per minute.
    pub client_per_minute: u32,
    /// Maximum accepted connections per peer per hour.
    pub client_per_hour: u32,
    /// Maximum accepted connections per peer per day.
    pub client_per_day: u32,
    /// Maximum accepted connections across all peers per minute.
    pub total_per_minute: u32,
    /// Maximum accepted connections across all peers per hour.
    pub total_per_hour: u32,
    /// Maximum accepted connections across all peers per day.
    pub total_per_day: u32,
}

impl ServerConnectionRateLimits {
    /// Whether any peer or aggregate rate is configured.
    pub fn enabled(self) -> bool {
        self.client_per_minute != 0
            || self.client_per_hour != 0
            || self.client_per_day != 0
            || self.total_per_minute != 0
            || self.total_per_hour != 0
            || self.total_per_day != 0
    }

    /// Rejects direct specs outside the Proposal's bounded integer range.
    pub fn validate(self) -> Result<Self, ServiceTunnelError> {
        if self
            .as_array()
            .iter()
            .any(|value| *value > MAX_PROPOSAL_RATE)
        {
            return Err(ServiceTunnelError::ExceedsCeiling {
                field: "server_connection_rate",
                reason: "server connection rate exceeds 100000",
            });
        }
        Ok(self)
    }

    fn as_array(self) -> [u32; 6] {
        [
            self.client_per_minute,
            self.client_per_hour,
            self.client_per_day,
            self.total_per_minute,
            self.total_per_hour,
            self.total_per_day,
        ]
    }

    fn client_limits(self) -> [u32; 3] {
        [
            self.client_per_minute,
            self.client_per_hour,
            self.client_per_day,
        ]
    }

    fn total_limits(self) -> [u32; 3] {
        [
            self.total_per_minute,
            self.total_per_hour,
            self.total_per_day,
        ]
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct WindowCounter {
    epoch: u64,
    count: u32,
}

/// Bounded fixed-window accounting for authenticated inbound server peers.
/// The caller supplies process-monotonic milliseconds and serializes calls.
#[derive(Debug)]
pub struct ServerConnectionRateLimiter {
    limits: ServerConnectionRateLimits,
    total: [WindowCounter; 3],
    peers: BTreeMap<[u8; 32], [WindowCounter; 3]>,
}

impl ServerConnectionRateLimiter {
    /// Creates a fresh limiter for one service generation.
    pub fn new(limits: ServerConnectionRateLimits) -> Self {
        Self {
            limits,
            total: [WindowCounter::default(); 3],
            peers: BTreeMap::new(),
        }
    }

    /// Reserves one admission if every configured peer and total window allows it.
    /// At peer-table capacity, expired records are reclaimed; an unseen peer is
    /// rejected if the bounded table is still full.
    pub fn admit(&mut self, peer: [u8; 32], now_ms: u64) -> bool {
        if !self.limits.enabled() {
            return true;
        }
        let epochs = RATE_WINDOWS_MS.map(|window| now_ms / window);
        let client_limits = self.limits.client_limits();
        let total_limits = self.limits.total_limits();
        if !within_limits(&self.total, &epochs, &total_limits) {
            return false;
        }

        if client_limits.iter().any(|limit| *limit != 0) && !self.peers.contains_key(&peer) {
            if self.peers.len() >= MAX_RATE_LIMIT_PEERS {
                self.peers.retain(|_, counters| {
                    counters.iter().enumerate().any(|(index, counter)| {
                        client_limits[index] != 0 && counter.epoch == epochs[index]
                    })
                });
            }
            if self.peers.len() >= MAX_RATE_LIMIT_PEERS {
                return false;
            }
        }
        let peer_counters = self.peers.get(&peer).copied().unwrap_or_default();
        if !within_limits(&peer_counters, &epochs, &client_limits) {
            return false;
        }

        increment_windows(&mut self.total, &epochs, &total_limits);
        if client_limits.iter().any(|limit| *limit != 0) {
            let counters = self.peers.entry(peer).or_default();
            increment_windows(counters, &epochs, &client_limits);
        }
        true
    }

    /// Number of currently retained peer records, exposed for boundedness tests.
    pub fn tracked_peers(&self) -> usize {
        self.peers.len()
    }
}

fn within_limits(counters: &[WindowCounter; 3], epochs: &[u64; 3], limits: &[u32; 3]) -> bool {
    (0..3).all(|index| {
        limits[index] == 0
            || counters[index].epoch != epochs[index]
            || counters[index].count < limits[index]
    })
}

fn increment_windows(counters: &mut [WindowCounter; 3], epochs: &[u64; 3], limits: &[u32; 3]) {
    for index in 0..3 {
        if limits[index] == 0 {
            continue;
        }
        if counters[index].epoch != epochs[index] {
            counters[index] = WindowCounter {
                epoch: epochs[index],
                count: 0,
            };
        }
        counters[index].count = counters[index].count.saturating_add(1);
    }
}

/// Inbound peer destination-hash policy for server tunnels.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ServerAccessPolicy {
    /// Allowed peer hashes (`access_list` union `white_list`).
    pub allow: Vec<[u8; 32]>,
    /// Denied peer hashes (`black_list`).
    pub deny: Vec<[u8; 32]>,
    /// Authenticated peer and aggregate connection-rate limits.
    pub connection_rates: ServerConnectionRateLimits,
}

impl ServerAccessPolicy {
    /// Whether both lists are empty (admits everyone).
    pub fn is_empty(&self) -> bool {
        self.allow.is_empty() && self.deny.is_empty()
    }

    /// Decides one inbound peer: denied hashes always lose; an
    /// empty allow set admits everyone else; otherwise only members
    /// are admitted.
    pub fn allows(&self, peer: &[u8; 32]) -> bool {
        if self.deny.contains(peer) {
            return false;
        }
        if self.allow.is_empty() {
            return true;
        }
        self.allow.contains(peer)
    }

    /// Parses allow entries (`access_list` plus `white_list` values)
    /// and deny entries (`black_list` values) into a policy. Empty
    /// items are skipped; every other item must be a canonical
    /// Base32 destination hash.
    pub fn parse(allow_values: &[&str], deny_values: &[&str]) -> Result<Self, ServiceTunnelError> {
        let mut allow = Vec::new();
        for value in allow_values {
            parse_entries(value, &mut allow)?;
        }
        let mut deny = Vec::new();
        for value in deny_values {
            parse_entries(value, &mut deny)?;
        }
        Ok(Self {
            allow,
            deny,
            connection_rates: ServerConnectionRateLimits::default(),
        })
    }
}

/// Splits one option value on commas and whitespace and parses every
/// non-empty item as a destination hash.
fn parse_entries(value: &str, out: &mut Vec<[u8; 32]>) -> Result<(), ServiceTunnelError> {
    for item in value.split([',', ' ', '\t', '\r', '\n']) {
        let item = item.trim();
        if item.is_empty() {
            continue;
        }
        if out.len() >= MAX_ACCESS_LIST_ENTRIES {
            return Err(ServiceTunnelError::ExceedsCeiling {
                field: "access_list",
                reason: "access lists exceed the per-tunnel entry ceiling",
            });
        }
        out.push(parse_hash_entry(item)?);
    }
    Ok(())
}

/// Parses one entry as a canonical Base32 destination hash, with or
/// without the `.b32.i2p` suffix. Never echoes the entry: values
/// stay out of errors, logs, and control output.
fn parse_hash_entry(item: &str) -> Result<[u8; 32], ServiceTunnelError> {
    let label = item.strip_suffix(B32_SUFFIX).unwrap_or(item);
    crate::destination::decode_base32_label(label).ok_or(ServiceTunnelError::InvalidTarget {
        value: truncated_entry(),
        reason: "access entries must be canonical base32 destination hashes",
    })
}

/// Fixed placeholder so entry values never reach errors.
fn truncated_entry() -> String {
    String::from("<redacted-entry>")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Canonical 52-char label with a distinct payload char
    /// (the final char stays `a` so the trailing padding bits are
    /// zero, as canonical 32-byte encodings require).
    fn b32(byte: u8) -> String {
        format!(
            "{}{}a.b32.i2p",
            "a".repeat(50),
            (b'b' + (byte % 20)) as char
        )
    }

    #[test]
    fn allow_deny_semantics() {
        let policy = ServerAccessPolicy::parse(&[&b32(0)], &[&b32(1)]).expect("parse");
        let allowed = crate::destination::decode_base32_label(
            b32(0).strip_suffix(B32_SUFFIX).expect("suffix"),
        )
        .expect("hash");
        let denied = crate::destination::decode_base32_label(
            b32(1).strip_suffix(B32_SUFFIX).expect("suffix"),
        )
        .expect("hash");
        assert!(policy.allows(&allowed));
        assert!(!policy.allows(&denied));
        assert!(!policy.allows(&[9_u8; 32]));
        let open = ServerAccessPolicy::parse(&[], &[]).expect("parse");
        assert!(open.is_empty());
        assert!(open.allows(&[9_u8; 32]));
        // Deny wins over allow for the same hash.
        let conflict = ServerAccessPolicy::parse(&[&b32(0)], &[&b32(0)]).expect("parse");
        assert!(!conflict.allows(&allowed));
    }

    #[test]
    fn separators_and_suffix_forms() {
        let suffixed = b32(2);
        let bare = suffixed
            .strip_suffix(B32_SUFFIX)
            .expect("suffix")
            .to_owned();
        let policy = ServerAccessPolicy::parse(&[&format!("{suffixed}, {bare}\n {bare}")], &[])
            .expect("parse");
        assert_eq!(policy.allow.len(), 3);
    }

    #[test]
    fn non_hash_entries_rejected_without_echo() {
        let bad_label = "a".repeat(52).replace('a', "8");
        for bad in [
            "example.i2p",
            "127.0.0.1",
            "AAAA",
            bad_label.as_str(),
            "not a hash at all, with spaces and commas,,",
        ] {
            let error = ServerAccessPolicy::parse(&[bad], &[]).expect_err("must fail");
            let text = format!("{error}");
            assert!(!text.contains("example.i2p"), "value leaked: {text}");
            assert!(!text.contains("127.0.0.1"), "value leaked: {text}");
        }
    }

    #[test]
    fn entry_ceiling_enforced() {
        let many = (0..=MAX_ACCESS_LIST_ENTRIES)
            .map(|_| b32(3))
            .collect::<Vec<_>>()
            .join(",");
        assert!(ServerAccessPolicy::parse(&[&many], &[]).is_err());
    }

    #[test]
    fn connection_rate_limiter_enforces_peer_and_aggregate_windows() {
        let mut per_peer = ServerConnectionRateLimiter::new(ServerConnectionRateLimits {
            client_per_minute: 1,
            ..ServerConnectionRateLimits::default()
        });
        assert!(per_peer.admit([1; 32], 10));
        assert!(!per_peer.admit([1; 32], 11));
        assert!(per_peer.admit([2; 32], 11));
        assert!(per_peer.admit([1; 32], 60_000));

        let mut aggregate = ServerConnectionRateLimiter::new(ServerConnectionRateLimits {
            total_per_hour: 1,
            ..ServerConnectionRateLimits::default()
        });
        assert!(aggregate.admit([1; 32], 0));
        assert!(!aggregate.admit([2; 32], 1));
        assert!(aggregate.admit([2; 32], 3_600_000));
        assert_eq!(aggregate.tracked_peers(), 0);
    }

    #[test]
    fn connection_rate_limiter_is_bounded_and_fails_closed() {
        let mut limiter = ServerConnectionRateLimiter::new(ServerConnectionRateLimits {
            client_per_day: 1,
            ..ServerConnectionRateLimits::default()
        });
        for index in 0..MAX_RATE_LIMIT_PEERS {
            let mut peer = [0_u8; 32];
            peer[..4].copy_from_slice(&(index as u32).to_be_bytes());
            assert!(limiter.admit(peer, 1));
        }
        assert_eq!(limiter.tracked_peers(), MAX_RATE_LIMIT_PEERS);
        assert!(!limiter.admit([0xff; 32], 2));
        // A new day reclaims old records before adding the next peer.
        assert!(limiter.admit([0xff; 32], 86_400_000));
        assert_eq!(limiter.tracked_peers(), 1);
    }
}
