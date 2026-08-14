//! Runtime-neutral tunnel identity, exploratory pool, build-record
//! layout, build-cryptography seam, reply-path provider, and
//! short-tunnel-build state machine for `i2pr`.
//!
//! This crate is the Milestone 5 implementation surface (Plans 107
//! and 108). It owns:
//!
//! - typed tunnel identity ([`identity`])
//! - bounded exploratory tunnel pool configuration ([`config`])
//! - the deterministic [`pool::ExploratoryPool`] with bounded
//!   replacement, expiry, and failure accounting
//! - the [`build::BuildRecordLayout`] surface over the existing
//!   `i2pr_proto::DeferredBuildRecords` codec and the canonical
//!   wire constants for short and variable builds
//! - the [`build_crypto::BuildCryptography`] seam together with the
//!   Plan 108 ECIES-X25519 primitive that protects short
//!   tunnel-build request/reply records
//! - the typed short-build request/reply records ([`short`] and the
//!   [`short_record`] module) and the per-hop crypto contexts that
//!   drive the build state machine
//! - the [`short_state::ShortBuildStateMachine`] runtime-neutral
//!   build state machine
//! - the [`short_state::ShortBuildRegistrar`] that registers a
//!   fully validated build in the [`pool::ExploratoryPool`] only
//!   after every hop has accepted
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
#![allow(clippy::module_name_repetitions)]

pub mod build;
pub mod build_crypto;
pub mod config;
pub mod identity;
pub mod pool;
pub mod provider;
pub mod responder;
pub mod short;
pub mod short_record;
pub mod short_state;

pub use build::{
    BuildCryptographyUnavailable, BuildRecordLayout, BuildRecordLayoutError, BuildReplyKind,
    BuildRequestKind,
};
pub use build_crypto::{
    BuildCryptography, BuildCryptographyError, EciesX25519BuildCryptography, LayerKeys,
    NoBuildCryptography, SHORT_REPLY_KEY_LEN, SHORT_REQUEST_KEY_LEN,
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
pub use short::{
    BuildAttemptId, BuildEvent, HopCryptoContext, ShortBuildAction, ShortBuildConstructionError,
    ShortBuildOutcome, ShortBuildPath, ShortTunnelBuildMessage,
};
pub use short_record::{
    BuildOptions, BuildOptionsError, HopRole, LayerEncryptionType, ShortBuildError,
    ShortReplyRecord, ShortRequestRecord, ShortResponseCode,
};
pub use short_state::{HopResponse, ShortBuildRegistrar, ShortBuildState, ShortBuildStateMachine};
