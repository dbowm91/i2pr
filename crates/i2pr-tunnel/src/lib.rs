//! Runtime-neutral tunnel identity, exploratory pool, build-record
//! layout, build-cryptography seam, and reply-path provider for
//! `i2pr`.
//!
//! This crate is the first Milestone 5 implementation surface (Plan
//! 107). It owns:
//!
//! - typed tunnel identity ([`identity`])
//! - bounded exploratory tunnel pool configuration ([`config`])
//! - the deterministic [`pool::ExploratoryPool`] with bounded
//!   replacement, expiry, and failure accounting
//! - the [`build::BuildRecordLayout`] surface over the existing
//!   `i2pr_proto::DeferredBuildRecords` codec
//! - the [`build_crypto::BuildCryptography`] seam that future plans
//!   plug into with a live ECIES-X25519 primitive
//! - the [`provider::ExploratoryPoolReplyPathProvider`] that turns
//!   the pool into a [`i2pr_netdb::ReplyPath`] source the Plan 105
//!   lookup state machine can consume
//!
//! The crate deliberately remains runtime-neutral: it does not open
//! sockets, does not perform DNS, does not spawn tasks, and depends
//! only on `i2pr-proto`, `i2pr-crypto`, `i2pr-core`, and
//! `i2pr-netdb`.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod build;
pub mod build_crypto;
pub mod config;
pub mod identity;
pub mod pool;
pub mod provider;

pub use build::{
    BuildCryptographyUnavailable, BuildRecordLayout, BuildRecordLayoutError, BuildRequestKind,
};
pub use build_crypto::{
    BuildCryptography, BuildCryptographyError, LAYER_KEY_LEN, LayerKeys, NoBuildCryptography,
};
pub use config::{
    ExploratoryConfigError, ExploratoryPoolConfig, MAX_BUILD_CONCURRENCY, MAX_EXPLORATORY_INBOUND,
    MAX_EXPLORATORY_OUTBOUND, MAX_FAILURE_THRESHOLD, MAX_HOPS, MIN_HOPS,
};
pub use identity::{
    MAX_TUNNEL_ID, TunnelDirection, TunnelId, TunnelIdError, TunnelLifetime, TunnelLifetimeError,
    TunnelPeer, TunnelRole, TunnelState,
};
pub use pool::{
    ExploratoryPool, MAX_HOPS_PER_TUNNEL, PoolError, PoolFullError, RegisterError, RegisterOutcome,
    RegistrationError, TunnelRegistration, TunnelSlot,
};
pub use provider::ExploratoryPoolReplyPathProvider;
