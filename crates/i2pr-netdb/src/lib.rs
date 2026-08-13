//! Runtime-neutral RouterInfo validation, local NetDB foundation, and
//! local signed RouterInfo construction for `i2pr`.
//!
//! This crate is part of the Plan 103 child of Plan 102. It owns the
//! first stateful router-information subsystem: cryptographic and
//! temporal RouterInfo validation, RouterHash derivation/binding, a
//! bounded in-memory store with deterministic
//! replacement/conflict/expiry, floodfill-capability extraction as data
//! rather than trust, and construction of the local signed RouterInfo
//! without advertising unqualified transports.
//!
//! The crate deliberately remains runtime-neutral: it does not open
//! sockets, does not perform DNS, downloads nothing, persists nothing,
//! and depends only on `i2pr-proto` and `i2pr-crypto`. It does not
//! own Tokio, file-system effects, or a transport implementation.

#![forbid(unsafe_code)]

mod base64;
mod local;
mod reseed;
mod router_info;
mod routing;
mod store;

pub use base64::{I2pBase64Error, MAX_DECODED_LEN, encode_filename_prefix};
pub use local::{LocalRouterInfo, LocalRouterInfoBuilder, LocalRouterInfoError};
pub use reseed::TrustedSigner;
pub use reseed::{
    ReseedEntryReport, ReseedEntryState, ReseedLimits, ReseedSignatureType, ReseedSignerId,
    ReseedSignerTrustSet, ReseedTrustError, ReseedVerifiedBundle, ReseedVerifyOutcome,
    ReseedVerifyReport, parse_su3, verify_su3, verify_su3_archive, verify_su3_with_signers,
};
pub use router_info::RouterInfoValidationPolicy as ValidationPolicy;
pub use router_info::{
    RouterHash, RouterInfoValidationError, RouterInfoValidationPolicy, ValidatedRouterInfo,
    ValidationContext, router_hash,
};
pub use routing::{NearestSelection, xor_distance};
pub use store::{InsertOutcome, RouterInfoStore, RouterInfoStoreConfig, RouterInfoStoreStats};
