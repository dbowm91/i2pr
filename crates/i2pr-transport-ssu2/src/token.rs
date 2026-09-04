//! Bounded one-use address-validation token lifecycle (runtime-neutral).
//!
//! Tokens are 8-byte values bound to the exact source socket address
//! (IP and port; IPv4 tokens never validate on IPv6 and vice versa via
//! the address comparison). Token randomness and wall-clock time are
//! supplied by the caller: production OS randomness/time fulfillment
//! belongs to the runtime (Plan 158); tests inject deterministic bytes
//! and timestamps.
//!
//! Normative traceability: SSU2 specification sections Token Request,
//! Retry, New Token block, and the token-lifecycle rules (one-use,
//! short Retry-token expiry, sender-specified NewToken expiry, source
//! binding, last-token-valid storage guidance, and migration
//! invalidation).

use std::{
    fmt,
    net::{IpAddr, SocketAddr},
    vec::Vec,
};

use thiserror::Error;

use crate::constants;
use crate::handshake::HandshakeError;

/// Typed failures from token issuance and validation.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum TokenError {
    /// The token value was zero (reserved for rejection semantics).
    #[error("SSU2 token value is zero")]
    ZeroToken,
    /// No stored token matched the presented value.
    #[error("SSU2 token is unknown")]
    UnknownToken,
    /// The stored token expired before presentation.
    #[error("SSU2 token expired")]
    ExpiredToken,
    /// The stored token was already consumed (one-use semantics).
    #[error("SSU2 token was already consumed")]
    ReusedToken,
    /// The presenting source differs from the bound source.
    #[error("SSU2 token source mismatch")]
    WrongSource,
    /// The bounded table cannot retain another token.
    #[error("SSU2 token table is full")]
    TableFull,
}

impl From<TokenError> for HandshakeError {
    fn from(value: TokenError) -> Self {
        match value {
            TokenError::ZeroToken
            | TokenError::UnknownToken
            | TokenError::ExpiredToken
            | TokenError::ReusedToken
            | TokenError::WrongSource => Self::TokenRejected,
            TokenError::TableFull => Self::LocalPolicyDenied,
        }
    }
}

/// A nonzero 8-byte address-validation token value.
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct Ssu2Token(u64);

impl Ssu2Token {
    /// Wraps raw token bytes, rejecting the zero value.
    pub fn new(value: u64) -> Result<Self, TokenError> {
        if value == 0 {
            return Err(TokenError::ZeroToken);
        }
        Ok(Self(value))
    }

    /// Wraps big-endian wire bytes, rejecting the zero value.
    pub fn from_be_bytes(bytes: [u8; 8]) -> Result<Self, TokenError> {
        Self::new(u64::from_be_bytes(bytes))
    }

    /// Returns the big-endian wire encoding.
    pub const fn to_be_bytes(self) -> [u8; 8] {
        self.0.to_be_bytes()
    }

    /// Returns the raw token value for header placement.
    pub const fn value(self) -> u64 {
        self.0
    }
}

impl fmt::Debug for Ssu2Token {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Ssu2Token(<redacted>)")
    }
}

struct TokenEntry {
    token: u64,
    source: SocketAddr,
    issued_at: u64,
}

/// A bounded one-use token table driven by caller-supplied time and
/// randomness. All bounds are exact: issuance evicts the oldest entry
/// deterministically when a quota is full, and consumption removes the
/// entry so reuse fails closed.
pub struct TokenStore {
    entries: Vec<TokenEntry>,
    max_global: usize,
    max_per_source: usize,
    lifetime_seconds: u64,
    epoch: u64,
}

impl TokenStore {
    /// Creates an empty table with explicit quotas. Quotas above the
    /// protocol ceilings are rejected so tests pin exact capacities.
    pub fn new(
        max_global: usize,
        max_per_source: usize,
        lifetime_seconds: u64,
    ) -> Result<Self, TokenError> {
        if max_global == 0
            || max_per_source == 0
            || lifetime_seconds == 0
            || max_global > constants::MAX_TOKENS_GLOBAL
            || max_per_source > constants::MAX_TOKENS_PER_SOURCE
        {
            return Err(TokenError::TableFull);
        }
        Ok(Self {
            entries: Vec::with_capacity(max_global),
            max_global,
            max_per_source,
            lifetime_seconds,
            epoch: 0,
        })
    }

    /// Creates the default establishment table.
    pub fn establishment() -> Self {
        Self {
            entries: Vec::with_capacity(constants::MAX_TOKENS_GLOBAL),
            max_global: constants::MAX_TOKENS_GLOBAL,
            max_per_source: constants::MAX_TOKENS_PER_SOURCE,
            lifetime_seconds: constants::TOKEN_LIFETIME_SECONDS,
            epoch: 0,
        }
    }

    /// Returns the current key-epoch (bumped by rotation/restart).
    pub const fn epoch(&self) -> u64 {
        self.epoch
    }

    /// Returns the number of retained tokens.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns whether no tokens are retained.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Removes expired entries deterministically and releases accounting.
    pub fn expire(&mut self, now: u64) {
        let lifetime = self.lifetime_seconds;
        self.entries
            .retain(|entry| now.saturating_sub(entry.issued_at) <= lifetime);
    }

    /// Rotates the table epoch (key/generator restart): all previously
    /// issued tokens are invalidated and accounting is released.
    pub fn rotate(&mut self) {
        self.entries.clear();
        self.epoch = self.epoch.saturating_add(1);
    }

    /// Issues a token bound to `source` from caller-supplied bytes.
    /// Expired entries are released first; a full per-source or global
    /// quota deterministically evicts the oldest entry in scope.
    pub fn issue(
        &mut self,
        source: SocketAddr,
        now: u64,
        token_bytes: [u8; 8],
    ) -> Result<Ssu2Token, TokenError> {
        let token = Ssu2Token::new(u64::from_be_bytes(token_bytes))?;
        self.expire(now);
        let per_source = self
            .entries
            .iter()
            .filter(|entry| entry.source == source)
            .count();
        if per_source >= self.max_per_source {
            let oldest = self
                .entries
                .iter()
                .enumerate()
                .filter(|(_, entry)| entry.source == source)
                .min_by_key(|(_, entry)| entry.issued_at)
                .map(|(index, _)| index)
                .ok_or(TokenError::TableFull)?;
            self.entries.remove(oldest);
        }
        if self.entries.len() >= self.max_global {
            let oldest = self
                .entries
                .iter()
                .enumerate()
                .min_by_key(|(_, entry)| entry.issued_at)
                .map(|(index, _)| index)
                .ok_or(TokenError::TableFull)?;
            self.entries.remove(oldest);
        }
        self.entries.push(TokenEntry {
            token: token.value(),
            source,
            issued_at: now,
        });
        Ok(token)
    }

    /// Validates and consumes a presented token: exact value, exact
    /// source (IP and port; family is part of the address comparison),
    /// and unexpired. Consumption removes the entry; any second use
    /// fails closed as unknown.
    pub fn consume(&mut self, token: u64, source: SocketAddr, now: u64) -> Result<(), TokenError> {
        if token == 0 {
            return Err(TokenError::ZeroToken);
        }
        let position = self.entries.iter().position(|entry| entry.token == token);
        let Some(index) = position else {
            return Err(TokenError::UnknownToken);
        };
        let entry = &self.entries[index];
        if now.saturating_sub(entry.issued_at) > self.lifetime_seconds {
            self.entries.remove(index);
            return Err(TokenError::ExpiredToken);
        }
        if entry.source != source {
            return Err(TokenError::WrongSource);
        }
        self.entries.remove(index);
        Ok(())
    }

    /// Returns whether the exact source address family is IPv6.
    /// (Family separation falls out of the exact address comparison;
    /// this helper documents the v4/v6 non-transfer rule for callers.)
    pub const fn source_is_ipv6(source: SocketAddr) -> bool {
        matches!(source.ip(), IpAddr::V6(_))
    }
}

/// Returns the Retry amplification budget for a request length: at most
/// three times the request datagram length.
pub const fn retry_response_budget(request_length: usize) -> usize {
    request_length
        .saturating_mul(constants::RETRY_AMPLIFICATION_NUMERATOR)
        .saturating_div(constants::RETRY_AMPLIFICATION_DENOMINATOR)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v4(last: u8, port: u16) -> SocketAddr {
        SocketAddr::new(format!("192.0.2.{last}").parse().expect("test ip"), port)
    }

    fn v6(last: u16, port: u16) -> SocketAddr {
        SocketAddr::new(format!("2001:db8::{last}").parse().expect("test ip"), port)
    }

    #[test]
    fn issue_consume_round_trip_is_one_use() {
        let mut store = TokenStore::establishment();
        let token = store
            .issue(v4(1, 1000), 500, 0x0102_0304_0506_0708_u64.to_be_bytes())
            .expect("issue");
        assert_eq!(token.value(), 0x0102_0304_0506_0708);
        assert_eq!(store.len(), 1);
        store
            .consume(token.value(), v4(1, 1000), 505)
            .expect("consume");
        assert!(store.is_empty());
        assert_eq!(
            store.consume(token.value(), v4(1, 1000), 506),
            Err(TokenError::UnknownToken)
        );
    }

    #[test]
    fn zero_token_is_rejected() {
        let mut store = TokenStore::establishment();
        assert_eq!(
            store.issue(v4(1, 1000), 500, [0_u8; 8]),
            Err(TokenError::ZeroToken)
        );
        assert_eq!(
            store.consume(0, v4(1, 1000), 500),
            Err(TokenError::ZeroToken)
        );
    }

    #[test]
    fn expired_tokens_fail_and_release() {
        let mut store = TokenStore::establishment();
        let token = store.issue(v4(1, 1000), 500, [7_u8; 8]).expect("issue");
        assert_eq!(
            store.consume(
                token.value(),
                v4(1, 1000),
                500 + constants::TOKEN_LIFETIME_SECONDS + 1
            ),
            Err(TokenError::ExpiredToken)
        );
        assert!(store.is_empty());
    }

    #[test]
    fn wrong_source_and_wrong_port_fail_closed() {
        let mut store = TokenStore::establishment();
        let token = store.issue(v4(1, 1000), 500, [7_u8; 8]).expect("issue");
        assert_eq!(
            store.consume(token.value(), v4(2, 1000), 505),
            Err(TokenError::WrongSource)
        );
        assert_eq!(
            store.consume(token.value(), v4(1, 1001), 505),
            Err(TokenError::WrongSource)
        );
        assert_eq!(
            store.consume(token.value(), v6(1, 1000), 505),
            Err(TokenError::WrongSource)
        );
        store
            .consume(token.value(), v4(1, 1000), 505)
            .expect("consume");
    }

    #[test]
    fn per_source_quota_evicts_oldest_deterministically() {
        let mut store = TokenStore::new(256, 2, 3600).expect("store");
        let source = v4(9, 9000);
        let first = store.issue(source, 100, [1_u8; 8]).expect("first");
        let _second = store.issue(source, 101, [2_u8; 8]).expect("second");
        let _third = store.issue(source, 102, [3_u8; 8]).expect("third");
        assert_eq!(store.len(), 2);
        assert_eq!(
            store.consume(first.value(), source, 103),
            Err(TokenError::UnknownToken)
        );
    }

    #[test]
    fn global_capacity_evicts_oldest_deterministically() {
        let mut store = TokenStore::new(2, 4, 3600).expect("store");
        let first = store.issue(v4(1, 1), 100, [1_u8; 8]).expect("first");
        let _second = store.issue(v4(2, 2), 101, [2_u8; 8]).expect("second");
        let _third = store.issue(v4(3, 3), 102, [3_u8; 8]).expect("third");
        assert_eq!(store.len(), 2);
        assert_eq!(
            store.consume(first.value(), v4(1, 1), 103),
            Err(TokenError::UnknownToken)
        );
    }

    #[test]
    fn rotation_invalidates_everything() {
        let mut store = TokenStore::establishment();
        let token = store.issue(v4(1, 1000), 500, [7_u8; 8]).expect("issue");
        let epoch = store.epoch();
        store.rotate();
        assert_eq!(store.epoch(), epoch + 1);
        assert!(store.is_empty());
        assert_eq!(
            store.consume(token.value(), v4(1, 1000), 505),
            Err(TokenError::UnknownToken)
        );
    }

    #[test]
    fn retry_budget_is_three_times_request() {
        assert_eq!(retry_response_budget(80), 240);
        assert_eq!(retry_response_budget(0), 0);
    }
}
