//! Lookup identity, exploratory reply-path tokens, and duplicate
//! coalescing primitives.
//!
//! Plan 105 §3 owns the typed lookup identity, the bounded waiter
//! list, and the bounded coalescing state. Plan 105 §4 owns the
//! outbound `DatabaseLookup` reply-path requirement — the explicit
//! handoff between the pure state machine and the future Milestone 5
//! exploratory-tunnel owner.

use i2pr_proto::Hash;

use crate::router_info::RouterHash;

/// Lookup kinds supported by this crate. Only `RouterInfo` is wired
/// end-to-end in Plan 105; future kinds will be added without breaking
/// the existing variant set.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum LookupKind {
    /// Standard `DatabaseLookup` for a `RouterInfo` record.
    RouterInfo,
}

impl LookupKind {
    /// Returns the I2P `DatabaseLookup` lookup-type bits for this kind.
    pub const fn wire_code(self) -> u8 {
        match self {
            Self::RouterInfo => 2,
        }
    }
}

/// Bounded lookup identity. The `RequestId` is caller-supplied; the
/// state machine never aliases it. The combination
/// `(RequestId, LookupKind, RouterHash)` is the unique key for one
/// active lookup.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LookupId {
    request_id: u64,
    kind: LookupKind,
    target: RouterHash,
}

impl LookupId {
    /// Constructs a lookup identity.
    pub const fn new(request_id: u64, kind: LookupKind, target: RouterHash) -> Self {
        Self {
            request_id,
            kind,
            target,
        }
    }

    /// Returns the caller-supplied request identifier.
    pub const fn request_id(&self) -> u64 {
        self.request_id
    }

    /// Returns the lookup kind.
    pub const fn kind(&self) -> LookupKind {
        self.kind
    }

    /// Returns the target RouterHash.
    pub const fn target(&self) -> RouterHash {
        self.target
    }
}

/// Maximum number of local waiters that may share one active lookup.
/// Plan 105 §3.4 requires a bounded waiter count; this constant is the
/// enforced ceiling.
pub const MAX_WAITERS_PER_LOOKUP: usize = 32;

/// Maximum number of `LookupId`s that may be coalesced into one
/// outbound attempt. The ceiling protects the `DatabaseLookup`
/// `excluded_peer` list from overflowing the protocol-defined limit.
pub const MAX_COALESCED_LOOKUPS: usize = 8;

/// Exploratory-tunnel reply path supplied by the future Milestone 5
/// owner. The token is intentionally narrow — only the fields the
/// `DatabaseLookup` payload actually requires.
///
/// The lookup state machine refuses to emit a standards-conformant
/// `DatabaseLookup` action until the call site supplies a token
/// struct. A direct peer link alone is **not** equivalent to a
/// complete reply path.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ReplyPath {
    /// RouterHash of the inbound exploratory tunnel gateway.
    pub gateway: RouterHash,
    /// Inbound exploratory tunnel identifier.
    pub tunnel_id: u32,
}

impl ReplyPath {
    /// Constructs a reply-path token after validating the tunnel
    /// identifier is non-zero.
    pub const fn new(gateway: RouterHash, tunnel_id: u32) -> Result<Self, ReplyPathError> {
        if tunnel_id == 0 {
            return Err(ReplyPathError::ZeroTunnelId);
        }
        Ok(Self { gateway, tunnel_id })
    }

    /// Returns the inbound gateway RouterHash.
    pub const fn gateway(&self) -> RouterHash {
        self.gateway
    }

    /// Returns the inbound tunnel identifier.
    pub const fn tunnel_id(&self) -> u32 {
        self.tunnel_id
    }
}

/// Validation failures for [`ReplyPath`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReplyPathError {
    /// The tunnel identifier was zero.
    ZeroTunnelId,
}

impl core::fmt::Display for ReplyPathError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::ZeroTunnelId => formatter.write_str("reply path tunnel id must be nonzero"),
        }
    }
}

impl std::error::Error for ReplyPathError {}

/// Bounded waiter list for a single lookup.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WaiterSet {
    waiters: Vec<u64>,
}

impl WaiterSet {
    /// Creates an empty waiter set.
    pub const fn new() -> Self {
        Self {
            waiters: Vec::new(),
        }
    }

    /// Adds a waiter request identifier. Returns `false` when the
    /// waiter is already present or the set is at capacity.
    pub fn add(&mut self, request_id: u64) -> bool {
        if self.waiters.contains(&request_id) {
            return false;
        }
        if self.waiters.len() >= MAX_WAITERS_PER_LOOKUP {
            return false;
        }
        self.waiters.push(request_id);
        true
    }

    /// Removes a waiter request identifier.
    pub fn remove(&mut self, request_id: u64) -> bool {
        if let Some(position) = self.waiters.iter().position(|value| *value == request_id) {
            self.waiters.remove(position);
            return true;
        }
        false
    }

    /// Returns the number of waiters.
    pub fn len(&self) -> usize {
        self.waiters.len()
    }

    /// Returns whether the set is empty.
    pub fn is_empty(&self) -> bool {
        self.waiters.is_empty()
    }

    /// Returns whether the set has reached its capacity.
    pub fn is_full(&self) -> bool {
        self.waiters.len() >= MAX_WAITERS_PER_LOOKUP
    }

    /// Returns the waiter identifiers in insertion order.
    pub fn request_ids(&self) -> &[u64] {
        &self.waiters
    }
}

impl Default for WaiterSet {
    fn default() -> Self {
        Self::new()
    }
}

/// Bounded coalescing state for one active lookup. The set tracks
/// the distinct requested `target` RouterHashes that have been
/// merged into the single outbound `DatabaseLookup` attempt.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CoalescedTargets {
    targets: Vec<RouterHash>,
}

impl CoalescedTargets {
    /// Adds a target RouterHash. Returns `false` when the target is
    /// already present or the set is at capacity.
    pub fn add(&mut self, target: RouterHash) -> bool {
        if self.targets.contains(&target) {
            return false;
        }
        if self.targets.len() >= MAX_COALESCED_LOOKUPS {
            return false;
        }
        self.targets.push(target);
        true
    }

    /// Returns the RouterHashes attached to the lookup.
    pub fn hashes(&self) -> &[RouterHash] {
        &self.targets
    }

    /// Returns the number of coalesced targets.
    pub fn len(&self) -> usize {
        self.targets.len()
    }

    /// Returns whether the set is empty.
    pub fn is_empty(&self) -> bool {
        self.targets.is_empty()
    }

    /// Returns whether the set has reached its capacity.
    pub fn is_full(&self) -> bool {
        self.targets.len() >= MAX_COALESCED_LOOKUPS
    }
}

/// Compatibility helper for callers that already hold a protocol
/// `Hash` value (such as a Bootstrap Manager output) and want to
/// convert it into a `RouterHash`.
pub fn router_hash_from_proto_hash(hash: Hash) -> RouterHash {
    RouterHash::from_hash(hash)
}

/// Reply-path provider implemented by the future Milestone 5
/// exploratory-tunnel owner. The seam consults the provider before
/// emitting any [`crate::LookupAction::SendDatabaselookup`] action.
///
/// The trait is intentionally narrow: the provider must report
/// whether at least one inbound tunnel is available and, on demand,
/// return one valid `ReplyPath`. A `None` result tells the seam to
/// stop the lookup with [`crate::LookupFinalState::PathUnavailable`].
///
/// Plan 107 wires `i2pr_tunnel::ExploratoryPoolReplyPathProvider` to
/// this trait. The seam will only consult a provider that is
/// actually injected; without an injection, the seam continues to
/// report the Plan 106 blocked status.
pub trait ReplyPathProvider {
    /// Returns `true` when at least one inbound exploratory tunnel is
    /// currently registered and unexpired.
    fn has_inbound_tunnel(&self) -> bool;
    /// Returns one reply path the seam can attach to the next
    /// outbound `DatabaseLookup`. Returns `None` when no valid
    /// inbound tunnel is registered.
    fn provide_reply_path(&self) -> Option<ReplyPath>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_kind_router_info_wire_code_is_two() {
        assert_eq!(LookupKind::RouterInfo.wire_code(), 2);
    }

    #[test]
    fn reply_path_rejects_zero_tunnel_id() {
        let gateway = RouterHash::from_bytes([0x01u8; 32]);
        let error = ReplyPath::new(gateway, 0).unwrap_err();
        assert_eq!(error, ReplyPathError::ZeroTunnelId);
    }

    #[test]
    fn reply_path_accepts_nonzero_tunnel_id() {
        let gateway = RouterHash::from_bytes([0x01u8; 32]);
        let path = ReplyPath::new(gateway, 7).expect("path");
        assert_eq!(path.gateway(), gateway);
        assert_eq!(path.tunnel_id(), 7);
    }

    #[test]
    fn waiter_set_enforces_capacity_and_uniqueness() {
        let mut set = WaiterSet::new();
        assert!(set.add(1));
        assert!(set.add(2));
        assert!(!set.add(1)); // duplicate rejected
        assert_eq!(set.len(), 2);
        // Push enough distinct entries to reach the cap.
        for value in 3..=MAX_WAITERS_PER_LOOKUP as u64 {
            assert!(set.add(value));
        }
        assert!(set.is_full());
        assert!(!set.add(MAX_WAITERS_PER_LOOKUP as u64 + 1));
    }

    #[test]
    fn waiter_set_remove_returns_presence() {
        let mut set = WaiterSet::new();
        set.add(42);
        assert!(set.remove(42));
        assert!(!set.remove(42));
        assert!(set.is_empty());
    }

    #[test]
    fn coalesced_targets_enforce_capacity_and_uniqueness() {
        let mut targets = CoalescedTargets::default();
        let a = RouterHash::from_bytes([0x01u8; 32]);
        let b = RouterHash::from_bytes([0x02u8; 32]);
        assert!(targets.add(a));
        assert!(targets.add(b));
        assert!(!targets.add(a));
        for index in 3..=MAX_COALESCED_LOOKUPS as u8 + 1 {
            let key = RouterHash::from_bytes([index; 32]);
            let added = targets.add(key);
            assert_eq!(added, (index as usize) <= MAX_COALESCED_LOOKUPS);
        }
        assert!(targets.is_full());
    }

    #[test]
    fn lookup_id_round_trips_its_fields() {
        let target = RouterHash::from_bytes([0x99u8; 32]);
        let id = LookupId::new(0x1234, LookupKind::RouterInfo, target);
        assert_eq!(id.request_id(), 0x1234);
        assert_eq!(id.kind(), LookupKind::RouterInfo);
        assert_eq!(id.target(), target);
    }
}
