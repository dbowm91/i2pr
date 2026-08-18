//! Runtime-neutral tunnel identity, exploratory pool, build-record
//! layout, build-cryptography seam, reply-path provider, short-
//! tunnel-build state machine, and local tunnel data plane for
//! `i2pr`.
//!
//! This crate is the Milestone 5 implementation surface (Plans 107,
//! 108, 109, 111–115, and the Plan 116 local tunnel data plane +
//! final closure pass). It owns:
//!
//! - typed tunnel identity ([`identity`])
//! - bounded exploratory tunnel pool configuration ([`config`])
//! - the deterministic [`pool::ExploratoryPool`] with bounded
//!   replacement, expiry, and failure accounting
//! - the [`build::BuildRecordLayout`] surface over the existing
//!   `i2pr_proto::DeferredBuildRecords` codec and the canonical
//!   wire constants for short and variable builds
//! - the [`build_crypto::BuildCryptography`] seam together with the
//!   Plan 109 ECIES-X25519 Noise-N primitive that protects short
//!   tunnel-build request and reply records
//! - the typed short-build request/reply records ([`short`] and the
//!   [`short_record`] module) and the per-hop crypto contexts that
//!   drive the build state machine
//! - the [`short_state::ShortBuildStateMachine`] runtime-neutral
//!   build state machine and the
//!   [`short::ShortBuildStateMachine::take_established_material`]
//!   success-only one-shot material transfer (Plan 116 F1)
//! - the [`short_state::ShortBuildRegistrar`] that registers a
//!   fully validated build in the [`pool::ExploratoryPool`] only
//!   after every hop has accepted, with the canonical
//!   `admit_established_machine` and `admit_material` surfaces and
//!   the legacy `admit(&ShortBuildOutcome, …)` failing closed
//! - the deterministic [`responder::DeterministicResponder`]
//!   peer simulator that exercises the Noise-N crypto primitive
//!   end-to-end
//! - the [`provider::ExploratoryPoolReplyPathProvider`] that turns
//!   the pool into a [`i2pr_netdb::ReplyPath`] source the Plan 105
//!   lookup state machine can consume
//! - the Plan 115 canonical production I2NP bridge
//!   ([`bridge::ShortBuildI2npBridge`]) that wraps a
//!   `ShortBuildAction::Deliver` in a complete I2NP type-25 message
//!   without double-prefixing the STBM record count byte
//! - the Plan 116 local tunnel data plane:
//!   - AES-256 ECB/CBC/ECB layer transforms and the duplicate
//!     window in [`layer`]
//!   - canonical I2P Tunnel Message Specification checksum builder
//!     and parser, automatic complete-message fragmentation, and the
//!     `unfragmented_overhead` / `fragmented_first_overhead` /
//!     `FOLLOW_ON_OVERHEAD` constants in [`data`]
//!   - the bounded [`fragment::BoundedReassembler`] with
//!     `insert_with_delivery` carrying the first-fragment
//!     `DeliveryInstruction` through reassembly
//!   - the `EstablishedTunnel` / `EstablishedHop` /
//!     `EstablishedMaterial` secret-material ownership with the
//!     one-shot `into_extracted` transfer in [`established`]
//!   - the runtime-neutral outbound/inbound/local role composition
//!     with multi-cell `forward_cells` / `process_cells` seams in
//!     [`roles`]
//!
//! The crate deliberately remains runtime-neutral: it does not open
//! sockets, does not perform DNS, does not spawn tasks, and depends
//! only on `i2pr-proto`, `i2pr-crypto`, `i2pr-core`, and
//! `i2pr-netdb`.

#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![allow(clippy::module_name_repetitions)]

pub mod bridge;
pub mod build;
pub mod build_crypto;
pub mod config;
pub mod conformance_fixtures;
pub mod data;
pub mod established;
pub mod fixed_vectors;
pub mod fragment;
pub mod identity;
pub mod layer;
pub mod multirecord;
pub mod pool;
pub mod provider;
pub mod responder;
pub mod roles;
pub mod short;
pub mod short_record;
pub mod short_state;

pub use bridge::{BridgeError, BridgeHeader, BridgeRecord, ShortBuildI2npBridge};
pub use build::{
    BuildCryptographyUnavailable, BuildRecordLayout, BuildRecordLayoutError, BuildReplyKind,
    BuildRequestKind,
};
pub use build_crypto::{
    AEAD_KEY_LEN, AEAD_NONCE_LEN, BuildCryptography, BuildCryptographyError, EPHEMERAL_KEY_LEN,
    EciesX25519BuildCryptography, HASH_PREFIX_LEN, LayerKeys, NoBuildCryptography,
    NoiseRequestState, OpenedShortRequest, SealedShortRequest, TAG_LEN, ValidatedRecordSlot,
};
pub use config::{
    ExploratoryConfigError, ExploratoryPoolConfig, MAX_BUILD_CONCURRENCY, MAX_EXPLORATORY_INBOUND,
    MAX_EXPLORATORY_OUTBOUND, MAX_FAILURE_THRESHOLD, MAX_HOPS, MIN_HOPS,
};
pub use data::{
    DeliveryInstruction, FragmentDelivery, MAX_FRAGMENT_BODY_BYTES, MAX_FRAGMENT_COUNT,
    MAX_PLAINTEXT_DATA_BYTES, MAX_TUNNEL_MESSAGE_PAYLOAD_BYTES, TunnelMessageBuilder,
    TunnelMessageError, TunnelMessageParser, TunnelPayloadHeader,
};
pub use established::{
    EstablishedHop, EstablishedMaterial, EstablishedNextHop, EstablishedRole, EstablishedTunnel,
    EstablishedTunnelError, zero_id, zero_peer,
};
pub use fragment::{
    BoundedReassembler, MAX_REASSEMBLY_AGGREGATE_BYTES, MAX_REASSEMBLY_BYTES_PER_MESSAGE,
    MAX_REASSEMBLY_MESSAGES, TunnelFragment,
};
pub use identity::{
    MAX_TUNNEL_ID, TunnelDirection, TunnelId, TunnelIdError, TunnelLifetime, TunnelLifetimeError,
    TunnelPeer, TunnelRole, TunnelState,
};
pub use layer::{
    DuplicateToken, DuplicateWindow, DuplicateWindowError, TUNNEL_IV_LEN, TUNNEL_PAYLOAD_LEN,
    TunnelLayerTransform,
};
pub use multirecord::{
    CHACHA20_KEY_LEN, CreatorReplyPostprocessor, MAX_RECORD_COUNT, MIN_PRODUCTION_RECORD_COUNT,
    MessageHopProcessor, MultiHopFixture, MultiHopReferenceFixture, MultiRecordError,
    MultiRecordHopSpec, OriginatorFake, PreparedHopContext, PreparedShortBuildMessage,
    ProcessedHopResult, RECORD_BYTES, REPLY_PLAINTEXT_BYTES, REQUEST_PLAINTEXT_BYTES,
    RecordAssignment, RecordOwner, ShortBuildRecordSet, SlotIndex, assign_record_slots,
    build_minimum_record_count, build_originator_fake_record, build_padding_fake_record,
    chacha20_transform, chacha20_xor, decode_outbound_tunnel_build_reply,
    decode_short_tunnel_build_payload, encode_count_prefixed_short_payload,
    encode_outbound_tunnel_build_reply, prepare_short_build_message,
    validate_count_prefixed_short_payload, verify_originator_fake,
};
pub use pool::{
    ExploratoryPool, MAX_HOPS_PER_TUNNEL, PoolError, PoolFullError, RegisterError, RegisterOutcome,
    RegistrationError, TunnelEntry, TunnelRegistration, TunnelSlot,
};
pub use provider::ExploratoryPoolReplyPathProvider;
pub use responder::{DeterministicResponder, ResponderError};
pub use roles::{
    InboundGatewayRole, InboundParticipantRole, LocalInboundEndpointRole, OBGWRouterDelivery,
    OutboundEndpointRole, OutboundGatewayRole, OutboundParticipantRole, RouterDeliveryAction,
    RouterDeliveryKind, TunnelRoleError,
};
pub use short::{
    BuildAttemptId, BuildEvent, HopCryptoContext, HopIndex, HopSpec, PerHopReply, ShortBuildAction,
    ShortBuildConstructionError, ShortBuildOutcome, ShortBuildPath, ShortTunnelBuildMessage,
};
pub use short_record::{
    BuildOptions, BuildOptionsError, HopRole, LayerEncryptionType, REQUEST_EXPIRATION_SECONDS,
    ShortBuildError, ShortReplyRecord, ShortRequestRecord, ShortResponseCode,
};
pub use short_state::{HopResponse, ShortBuildRegistrar, ShortBuildState, ShortBuildStateMachine};
