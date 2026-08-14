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
mod databaselookup;
mod local;
mod lookup_action;
mod lookup_engine;
mod lookup_id;
mod lookup_policy;
mod publication;
mod reseed;
mod router_info;
mod routing;
mod store;
mod store_message;

pub use base64::{I2pBase64Error, MAX_DECODED_LEN, encode_filename_prefix};
pub use databaselookup::{DatabaseLookupBuildError, build_databaselookup};
pub use local::{LocalRouterInfo, LocalRouterInfoBuilder, LocalRouterInfoError};
pub use lookup_action::{
    DecompressionError, LOOKUP_EXCLUDED_PEER_BUDGET, LookupAction, LookupFinalState, LookupOutcome,
    MAX_COMPRESSED_ROUTER_INFO_BYTES, MAX_DECOMPRESSED_ROUTER_INFO_BYTES, ReplyPathSink,
    decompress_router_info,
};
pub use lookup_engine::{
    CoalescedRouterInfoLookup, DeliveryOutcome, LookupDiagnostics, LookupEngineError, LookupResult,
    ResponseOutcome, RouterInfoLookup, StartOutcome, handle_database_store,
    handle_databasestore_message, handle_delivery_outcome, handle_search_reply,
    handle_searchreply_message,
};
pub use lookup_id::{
    CoalescedTargets, LookupId, LookupKind, MAX_COALESCED_LOOKUPS, MAX_WAITERS_PER_LOOKUP,
    ReplyPath, ReplyPathError, WaiterSet, router_hash_from_proto_hash,
};
pub use lookup_policy::{
    DEFAULT_MAX_CANDIDATES_CONSIDERED, DEFAULT_MAX_PEERS_PER_LOOKUP, DEFAULT_MAX_SUGGESTED_HASHES,
    DEFAULT_PER_ATTEMPT_DEADLINE_MS, DEFAULT_SUGGESTED_HASH_LIMIT, DEFAULT_TOTAL_DEADLINE_MS,
    FloodfillSelection, LookupPolicy, LookupPolicyError, MAX_SUGGESTED_HASH_LIMIT,
    select_floodfill_candidates,
};
pub use publication::{
    MAX_PUBLICATION_ATTEMPTS, PublicationAttempt, PublicationAttemptRecord,
    PublicationAttemptState, PublicationCoordinator, PublicationCorrelation, PublicationError,
    PublicationSnapshot,
};
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
pub use routing::{
    NearestSelection, RoutingKeyError, daily_routing_key, format_daily_key, xor_distance,
};
pub use store::{InsertOutcome, RouterInfoStore, RouterInfoStoreConfig, RouterInfoStoreStats};
pub use store_message::{
    UnsolicitedStoreError, UnsolicitedStoreOutcome, UnsolicitedStorePolicy,
    handle_unsolicited_databasestore,
};
