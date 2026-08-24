//! Local I2P destination lifecycle, destination-specific tunnel pools, and
//! LeaseSet2 generation.
//!
//! Plan 120 owns the first real `i2pr-client` destination runtime. The crate
//! composes [`i2pr_core`] service/lifecycle patterns with [`i2pr_crypto`]
//! signing/encryption primitives, [`i2pr_proto`] common structures, the
//! [`i2pr_netdb`] Plan 119 LeaseSet2 validation path, and the shared bounded
//! tunnel pool from [`i2pr_tunnel`].
//!
//! # Layering
//!
//! ```text
//! i2pr-client
//!   -> i2pr-core
//!   -> i2pr-crypto
//!   -> i2pr-netdb
//!   -> i2pr-proto
//!   -> i2pr-tunnel
//! ```
//!
//! The destination runtime owns its secret keys; neither the `i2pr-proto`
//! nor the `i2pr-netdb` crate ever sees a destination private key. The
//! canonical destination policy is defined in [`config`]; the identity
//! owner in [`identity`]; the destination-specific tunnel pool in
//! [`pool`]; the local LeaseSet2 generation, signing, and lifecycle in
//! [`leaseset`]; the bounded payload contracts in [`message`]; and the
//! registry/runtime/handle in [`registry`].

#![forbid(unsafe_code)]

pub mod config;
pub mod dispatch;
pub mod identity;
pub mod lease_selection;
pub mod leaseset;
pub mod message;
pub mod pool;
pub mod registry;
pub mod routing;
pub mod session;
pub mod streaming;
pub mod testing;

pub use config::{
    DEFAULT_LEASE_PUBLICATION_MARGIN_SECONDS, DEFAULT_LEASE_ROTATION_MARGIN_SECONDS,
    DestinationConfig, DestinationConfigError, MAX_AGGREGATE_COMMAND_QUEUE_DEPTH,
    MAX_DESTINATION_BUILD_CONCURRENCY, MAX_DESTINATION_FAILURE_THRESHOLD, MAX_DESTINATION_INBOUND,
    MAX_DESTINATION_OUTBOUND, MAX_LEASE_PUBLICATION_MARGIN_SECONDS,
    MAX_LEASE_ROTATION_MARGIN_SECONDS, MAX_LOCAL_DESTINATIONS, MAX_PENDING_DESTINATION_BYTES,
    MAX_PENDING_DESTINATION_MESSAGES, RegistryConfig,
};
pub use dispatch::{
    DestinationDispatcher, InboundDispatchError, InboundDispatchOutcome, MAX_INBOUND_DESTINATIONS,
    MAX_INBOUND_PAYLOAD_BYTES_PER_DESTINATION, MAX_INBOUND_PENDING_MESSAGES,
};
pub use identity::{DestinationId, DestinationIdentity, DestinationIdentityError};
pub use lease_selection::{
    LeaseSelectionError, LeaseSelectionPolicy, LeaseSelector, MAX_LEASE_SAFETY_MARGIN_SECONDS,
    SelectedLease,
};
pub use leaseset::{
    LEASE_SET2_SIGNATURE_DOMAIN, LeaseSetDecision, LeaseSetError, LeaseSetLifecycle,
    LeaseSetRotationCause, LeaseSetSummary, LocalLeaseSet, build_signed_lease_set2, encoded_hash,
};
pub use message::{
    BoundedPayloadQueue, DestinationPayload, MAX_DESTINATION_PAYLOAD_BYTES, PayloadError,
    QueuedOutbound, RoutingUnavailable,
};
pub use pool::{
    BuildFailureDisposition, DestinationPoolError, DestinationTunnelPool, InboundLeaseSource,
    outcome_slot,
};
pub use registry::{
    DestinationCommand, DestinationEvent, DestinationHandle, DestinationProgress,
    DestinationRegistry, DestinationRuntime, DestinationRuntimeError, DestinationShutdown,
    DestinationState, RegistryError,
};
pub use routing::{
    DestinationOutboundRole, DestinationRouting, DestinationRoutingConfig, DestinationRoutingError,
    EncryptedOutbound, LookupIngestError, LookupIngestOutcome, MAX_CONCURRENT_REMOTE_LOOKUPS,
    MAX_PENDING_OUTBOUND_PER_REMOTE, OutboundDeliveryPlan, OutboundRequest, SendError,
    compose_outbound_delivery,
};
pub use session::{
    DEFAULT_SESSION_IDLE_SECONDS, EciesAdvanceReport, EciesOutboundMessage, EciesPayloadError,
    EciesSessionConfig, EciesSessionConfigError, EciesSessionError, EciesSessionManager,
    MAX_REPLAY_CACHE_ENTRIES, MAX_SESSION_IDLE_SECONDS, MAX_TAG_LOOK_AHEAD, PendingHandshakeRecord,
    decode_decrypted_payload, encode_garlic_clove_payload, encode_new_session_payload, local_clove,
};
