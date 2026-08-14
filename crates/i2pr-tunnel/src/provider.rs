//! Reply-path provider adapter.
//!
//! Plan 107 §3.6 owns the bridge between the [`ExploratoryPool`] and
//! the [`i2pr_netdb::ReplyPath`] token the Plan 105 lookup state
//! machine consumes.
//!
//! The adapter is intentionally narrow: it asks the pool for the
//! oldest valid inbound tunnel and translates the result into a
//! `ReplyPath`. Time is supplied by the caller to keep the surface
//! deterministic.

#![forbid(unsafe_code)]

use i2pr_netdb::{ReplyPath, ReplyPathProvider};

use crate::pool::ExploratoryPool;

/// Adapter that exposes an [`ExploratoryPool`] as a reply-path
/// source. The adapter is `Clone` (cheap: it borrows the pool) and
/// is `Send + Sync` because the pool only exposes shared references.
#[derive(Clone, Debug)]
pub struct ExploratoryPoolReplyPathProvider<'a> {
    pool: &'a ExploratoryPool,
    now_seconds: u64,
}

impl<'a> ExploratoryPoolReplyPathProvider<'a> {
    /// Constructs a provider that borrows the supplied pool.
    pub const fn new(pool: &'a ExploratoryPool, now_seconds: u64) -> Self {
        Self { pool, now_seconds }
    }

    /// Updates the provider's view of the current time.
    pub fn set_now(&mut self, now_seconds: u64) {
        self.now_seconds = now_seconds;
    }

    /// Returns one [`ReplyPath`] if the pool currently contains at
    /// least one valid inbound tunnel. Returns `None` when no
    /// inbound tunnel is in the established-and-unexpired state.
    pub fn provide_reply_path(&self) -> Option<ReplyPath> {
        self.pool
            .select_inbound_reply_path(self.now_seconds)
            .and_then(|inner| inner.ok())
    }
}

impl<'a> ReplyPathProvider for ExploratoryPoolReplyPathProvider<'a> {
    fn has_inbound_tunnel(&self) -> bool {
        self.provide_reply_path().is_some()
    }

    fn provide_reply_path(&self) -> Option<ReplyPath> {
        ExploratoryPoolReplyPathProvider::provide_reply_path(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ExploratoryPoolConfig;
    use crate::identity::{TunnelId, TunnelPeer};
    use i2pr_proto::Hash;

    fn peer(value: u8) -> TunnelPeer {
        TunnelPeer::from_hash(Hash::from_bytes([value; 32]))
    }

    #[test]
    fn empty_pool_returns_none() {
        let pool = ExploratoryPool::new(ExploratoryPoolConfig::balanced());
        let provider = ExploratoryPoolReplyPathProvider::new(&pool, 0);
        assert!(provider.provide_reply_path().is_none());
        assert!(!provider.has_inbound_tunnel());
    }

    #[test]
    fn registered_inbound_tunnel_returns_path() {
        let mut pool = ExploratoryPool::new(ExploratoryPoolConfig::balanced());
        let id = TunnelId::new(0x4000).expect("nonzero");
        pool.register_inbound(id, vec![peer(7)], 0).expect("insert");
        let provider = ExploratoryPoolReplyPathProvider::new(&pool, 0);
        let path = provider.provide_reply_path().expect("path");
        assert_eq!(path.tunnel_id(), 0x4000);
        assert!(provider.has_inbound_tunnel());
    }

    #[test]
    fn expired_pool_returns_none() {
        let mut pool = ExploratoryPool::new(ExploratoryPoolConfig::balanced());
        let id = TunnelId::new(0x4000).expect("nonzero");
        pool.register_inbound(id, vec![peer(7)], 0).expect("insert");
        pool.advance_time(ExploratoryPoolConfig::balanced().lifetime().seconds() as u64);
        let provider = ExploratoryPoolReplyPathProvider::new(&pool, 0);
        assert!(provider.provide_reply_path().is_none());
        assert!(!provider.has_inbound_tunnel());
    }

    #[test]
    fn provider_supports_trait_dispatch() {
        let mut pool = ExploratoryPool::new(ExploratoryPoolConfig::balanced());
        let id = TunnelId::new(0x4000).expect("nonzero");
        pool.register_inbound(id, vec![peer(7)], 0).expect("insert");
        let provider = ExploratoryPoolReplyPathProvider::new(&pool, 0);
        let trait_object: &dyn ReplyPathProvider = &provider;
        assert!(trait_object.has_inbound_tunnel());
        let path = trait_object.provide_reply_path().expect("path");
        assert_eq!(path.tunnel_id(), 0x4000);
    }
}
