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
mod els2;
mod els2_auth;
mod els2_transcript;
pub use els2_auth::{
    AuthBlock, AuthClientEntry, AuthClientMaterial, AuthClientPublicKey, AuthCookie, ClientName,
    ELS2_AUTH_CLIENT_ID_LENGTH, ELS2_AUTH_COOKIE_LENGTH, ELS2_AUTH_OKM_LENGTH,
    ELS2_DH_AUTH_HKDF_INFO, ELS2_DH_AUTH_KEY_TYPE_CODE, ELS2_PSK_AUTH_HKDF_INFO, Els2AuthError,
    Els2AuthScheme, Els2AuthSecretRole, Els2AuthorizationServerConfig, Els2ClientAuth,
    Els2ClientAuthSecret, MAX_ELS2_CLIENT_NAME_LENGTH, PskClientKey, build_dh_block,
    build_psk_block, dh_client_material, draw_generation_secrets, psk_client_material,
    recover_auth_cookie,
};
mod floodfill_role;
mod floodfill_service;
mod lease_set;
mod lease_set2;
mod local;
mod lookup_action;
mod lookup_engine;
mod lookup_id;
mod lookup_policy;
mod provenance;
mod publication;
mod replication;
mod reseed;
mod resource;
mod router_info;
mod routing;
mod server_store;
mod store;
mod store_message;

pub use base64::{I2pBase64Error, MAX_DECODED_LEN, decode, encode, encode_filename_prefix};
pub use databaselookup::{DatabaseLookupBuildError, build_databaselookup};
pub use els2::{
    BlindedStorageKey, BlindingIdentity, BlindingSchedule, BlindingScheduleConfig, DailyBlinding,
    DecryptedEls2, ELS2_AUTH_CLIENT_LENGTH, ELS2_CREDENTIAL_PERSONALIZATION,
    ELS2_LAYER_KEY_MATERIAL_LENGTH, ELS2_LAYER1_FLAG_PER_CLIENT, ELS2_LAYER1_HKDF_INFO,
    ELS2_LAYER1_RESERVED_MASK, ELS2_LAYER1_SCHEME_DH, ELS2_LAYER1_SCHEME_MASK,
    ELS2_LAYER1_SCHEME_PSK, ELS2_LAYER1_SCHEME_SHIFT, ELS2_LAYER2_HKDF_INFO, ELS2_SALT_LENGTH,
    ELS2_SUBCREDENTIAL_PERSONALIZATION, Els2Credentials, Els2Error, Els2InsertOutcome, Els2Store,
    Els2StoreConfig, Els2StoreStats, Els2ValidationContext, Els2ValidationError,
    Els2ValidationPolicy, Layer1Authorization, LookupSecret, MAX_ELS2_AUTH_CLIENTS,
    MAX_ELS2_INNER_LEASE_SET_LENGTH, MAX_ELS2_OUTER_CIPHERTEXT_LENGTH, MAX_ELS2_RECORD_LENGTH,
    OwnerBlinding, ValidatedEncryptedLeaseSet2, day_bound_expiry_offset,
    decrypt_no_auth_outer_ciphertext, decrypt_outer_ciphertext, derive_els2_credentials,
    encrypt_no_auth_outer_ciphertext, encrypt_outer_ciphertext, next_utc_day_boundary_seconds,
    unblinded_scalar_from_ed25519_seed, utc_blinding_day,
};
pub use els2_transcript::{
    Els2RecordSignatureProfile, Els2Type11Profile, classify_type11, sign_type11_deployed,
    verify_type11,
};
pub use floodfill_role::{
    FloodfillAdvertisementPermit, FloodfillEligibilitySnapshot, FloodfillRoleController,
    FloodfillRoleEffect, FloodfillRoleState, LoopbackReachabilityProof, is_qualified_ssu2_address,
};
pub use floodfill_service::{
    FloodfillAck, FloodfillIngress, FloodfillLookupEffect, FloodfillReplyIntent, FloodfillRole,
    FloodfillStoreEffect, FloodfillStorePolicy, FloodfillStoreService, FloodfillStoreStats,
    FloodfillTime, LookupFailure, ReplicationCandidate, ReplyProtection, SERVABLE_LEASE_LOOKUP,
    SERVABLE_NORMAL_LOOKUP, SERVABLE_RECORD_TYPES,
};
pub use lease_set::{
    LeaseSetInsertOutcome, LeaseSetStore, LeaseSetStoreConfig, LeaseSetValidationContext,
    LeaseSetValidationError, MetaLeaseSetInsertOutcome, MetaLeaseSetStore, MetaLeaseSetStoreConfig,
    ValidatedLeaseSet, ValidatedMetaLeaseSet,
};
pub use lease_set2::{
    DestinationHash, LeaseSet2InsertOutcome, LeaseSet2Store, LeaseSet2StoreConfig,
    LeaseSet2StoreStats, LeaseSet2ValidationContext, LeaseSet2ValidationError,
    LeaseSet2ValidationPolicy, LeaseSetDisclosureBlock, ValidatedLeaseSet2,
};
pub use local::{
    BandwidthClass, BandwidthClassError, CONTROLLED_NET_ID, CONTROLLED_ROUTER_VERSION,
    LocalRouterInfo, LocalRouterInfoBuilder, LocalRouterInfoError, controlled_router_options,
};
pub use lookup_action::{
    DecompressionError, LOOKUP_EXCLUDED_PEER_BUDGET, LookupAction, LookupFinalState, LookupOutcome,
    MAX_COMPRESSED_ROUTER_INFO_BYTES, MAX_DECOMPRESSED_ROUTER_INFO_BYTES, ReplyPathSink,
    decompress_router_info,
};
pub use lookup_engine::{
    CoalescedRouterInfoLookup, DeliveryOutcome, LookupDiagnostics, LookupEngineError, LookupResult,
    ResponseOutcome, RouterInfoLookup, StartOutcome, handle_database_store,
    handle_database_store_lease_set2, handle_databasestore_message, handle_delivery_outcome,
    handle_search_reply, handle_searchreply_message,
};

pub use lookup_id::{
    CoalescedTargets, LookupId, LookupKind, MAX_COALESCED_LOOKUPS, MAX_WAITERS_PER_LOOKUP,
    ReplyPath, ReplyPathError, ReplyPathProvider, WaiterSet, router_hash_from_destination,
    router_hash_from_proto_hash,
};
pub use lookup_policy::{
    DEFAULT_MAX_CANDIDATES_CONSIDERED, DEFAULT_MAX_PEERS_PER_LOOKUP, DEFAULT_MAX_SUGGESTED_HASHES,
    DEFAULT_PER_ATTEMPT_DEADLINE_MS, DEFAULT_SUGGESTED_HASH_LIMIT, DEFAULT_TOTAL_DEADLINE_MS,
    FloodfillSelection, LookupPolicy, LookupPolicyError, MAX_SUGGESTED_HASH_LIMIT,
    select_floodfill_candidates,
};
pub use provenance::{
    ClientNamespaceId, Eligibility as ProvenanceEligibility, InboundProvenance, NetDbNamespace,
    ProvenanceIndex, ProvenanceLimits, RecordId, RecordProvenance, StorePurpose,
};
pub use publication::{
    MAX_PUBLICATION_ATTEMPTS, PublicationAttempt, PublicationAttemptRecord,
    PublicationAttemptState, PublicationCoordinator, PublicationCorrelation, PublicationError,
    PublicationSnapshot,
};
pub use replication::{
    DirectFloodAction, FloodfillPeerView, ReplicationError, ReplicationPlan, ReplicationPlanner,
    ReplicationPolicy, ReplicationStats, RoutingKeyClass, nearest_peers,
};
pub use reseed::TrustedSigner;
pub use reseed::{
    ReseedEntryReport, ReseedEntryState, ReseedLimits, ReseedSignatureType, ReseedSignerId,
    ReseedSignerTrustSet, ReseedTrustError, ReseedVerifiedBundle, ReseedVerifyOutcome,
    ReseedVerifyReport, parse_su3, trust_signer_from_certificate, verify_su3, verify_su3_archive,
    verify_su3_with_signers,
};
pub use resource::{
    FloodfillResourceBudget, FloodfillResourcePolicy, FloodfillResourceSnapshot, ResourceKind,
    ResourceLease,
};
pub use router_info::RouterInfoValidationPolicy as ValidationPolicy;
pub use router_info::{
    RouterHash, RouterInfoValidationError, RouterInfoValidationPolicy, ValidatedRouterInfo,
    ValidationContext, router_hash,
};
pub use routing::{
    NearestSelection, RoutingKeyError, daily_routing_key, format_daily_key, xor_distance,
};
pub use server_store::{
    MaintenanceBatch, ServerInsertOutcome, ServerNetDb, ServerNetDbConfig, ValidatedNetDbRecord,
    daily_rollover_due,
};
pub use store::{InsertOutcome, RouterInfoStore, RouterInfoStoreConfig, RouterInfoStoreStats};
pub use store_message::{
    UnsolicitedStoreError, UnsolicitedStoreOutcome, UnsolicitedStorePolicy,
    handle_unsolicited_databasestore,
};
