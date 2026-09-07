//! Runtime-neutral I2CP wire, profile, connection, session, and option
//! surface (Plans 164 and 165).
//!
//! This module owns:
//!
//! - the strict bounded I2CP framing and structural message codecs
//!   ([`frame`], [`message`]);
//! - the connection state machine and version handshake
//!   ([`connection`]);
//! - canonical SessionConfig verification with injected clock
//!   ([`verify`]);
//! - the option disposition table and bounded projection into
//!   `i2pr_client::DestinationConfig` ([`config`]);
//! - the bounded runtime-neutral session registry with reserve/commit/
//!   rollback ([`session`]);
//! - the typed action vocabulary the state machine emits ([`actions`]).
//!
//! It owns **no** sockets, timers, channels, or task runtimes. The
//! Plan 167 daemon projects these typed actions into `i2pr-client`
//! destination runtime operations.
//!
//! Normative authority is the official I2CP specification at
//! `i2p/i2p.website @ 26467e4b275e3a58280b9d4e6d4745d58bb8c499`
//! (`content/en/docs/specs/i2cp.md`, accurate for API 0.9.67) with
//! the I2CP overview page as the payload-format authority. Java I2P
//! 2.13.0 (`i2p/i2p.i2p @ 9134f808337b401e8e53c73734c81fab04280c9d`)
//! and go-i2cp (`go-i2p/go-i2cp @ b529ee1c10a6011558b4d69fc9436a4afc489eac`)
//! are inspected references only; no implementation source is copied.
//!
//! # M9 compatibility profile
//!
//! i2pr does not claim blanket API 0.9.67 compliance. The honest M9
//! profile targets the modern Standard LeaseSet2 + Ed25519/X25519
//! path used by current clients:
//!
//! ```text
//! implemented-m9
//!   GetDate / SetDate
//!   CreateSession / ReconfigureSession / DestroySession
//!   SessionStatus
//!   RequestVariableLeaseSet / CreateLeaseSet2 (Standard LeaseSet2 only)
//!   SendMessage / SendMessageExpires
//!   MessagePayload / MessageStatus
//!   GetBandwidthLimits / BandwidthLimits
//!   DestLookup / DestReply
//!   HostLookup / HostReply (structural; client probing deferred to Plan 170)
//!   Disconnect
//!
//! planned-later
//!   multi-session subsession semantics (codecs carry SessionId;
//!     the Plan 165 state machine decides acceptance)
//!
//! explicitly-unsupported
//!   BlindingInfo (blinded destinations are not in the M9 profile)
//!   EncryptedLeaseSet / MetaLeaseSet publication via CreateLeaseSet2
//!   PQ encryption types 5-7 in LeaseSets
//!   legacy ElGamal/DSA destination modes beyond structural parsing
//!   offline-signed SessionConfig / LeaseSet2 sections
//!
//! spec-defined-ignore
//!   Proposal 171 outbound-tunnel-switching flag (draft; no switching
//!     is implemented — Plan 165 documents the disposition)
//!   SendMessageExpires reliability-override bits 10-9 (unimplemented
//!     per specification; accepted without effect)
//!
//! legacy-deprecated
//!   CreateLeaseSet (type 4)
//!   ReceiveMessageBegin / ReceiveMessageEnd (types 6/7)
//!   RequestLeaseSet (type 21)
//!   ReportAbuse (type 29)
//!   abandoned preliminary CreateLeaseSet2 (type 40, treated as unknown)
//! ```
//!
//! Deprecated, unsupported, and unknown message types are rejected at
//! the classification layer without parsing their bodies, so framing
//! never desynchronizes.

pub mod actions;
pub mod config;
pub mod connection;
pub mod error;
pub mod frame;
pub mod ids;
pub mod mapping;
pub mod message;
pub mod payload;
pub mod session;
pub mod verify;

pub use actions::{DestinationLookupKey, I2cpAction};
pub use config::{
    M9_FAST_RECEIVE_DEFAULT, M9_LEASE_SET_ENC_TYPE, M9_LEASE_SET_TYPE,
    M9_MESSAGE_RELIABILITY_BEST_EFFORT, MAX_SESSION_CONFIG_KEY_BYTES, MAX_SESSION_CONFIG_OPTIONS,
    MAX_SESSION_CONFIG_VALUE_BYTES, OptionDisposition, OptionNote, ProjectedPolicy,
    ReconfigurationClass, SessionConfigLimits, classify_reconfigure_diff, default_registry_config,
    default_tunnel_lifetime, project_options, reconfiguration_class, validate_mapping_shape,
    validate_reconfigure_classifications,
};
pub use connection::{
    ConnectionState, ConnectionStateMachine, M9_ADVERTISED_VERSION, MAX_VERSION_STRING_BYTES,
};
pub use error::I2cpError;
pub use frame::{
    FRAME_HEADER_LEN, FrameDecoder, FrameHeader, MAX_I2CP_BODY_BYTES, PROTOCOL_BYTE, RawFrame,
    check_protocol_byte, decode_frame, decode_header, encode_frame,
};
pub use ids::{ClientNonce, HostRequestId, MessageId, SessionId};
pub use mapping::{
    MAX_I2CP_MAPPING_BODY_BYTES, MAX_I2CP_MAPPING_TEXT_BYTES, encode_mapping,
    split_mapping_lenient, split_mapping_strict,
};
pub use message::{
    BANDWIDTH_LIMITS_BODY_LEN, BandwidthLimits, CreateLeaseSet2, CreateSession, DestLookup,
    DestReply, DestReplyBody, DestroySession, Direction, Disconnect, GetBandwidthLimits, GetDate,
    HostLookup, HostLookupKey, HostReply, HostReplyResult, LEASE_SET_TYPE_CLASSIC,
    LEASE_SET_TYPE_ENCRYPTED, LEASE_SET_TYPE_META, LEASE_SET_TYPE_STANDARD_V2,
    MAX_I2CP_DESTINATION_BYTES, MAX_I2CP_PRIVATE_KEY_BYTES, MAX_I2CP_PRIVATE_KEYS,
    MAX_I2CP_STRING_BYTES, MAX_MESSAGE_STATUS_CODE, MAX_SESSION_STATUS_CODE, MAX_VARIABLE_LEASES,
    Message, MessageDisposition, MessagePayload, MessageStatus, MessageStatusCode, MessageType,
    ReconfigureSession, RequestVariableLeaseSet, RequestedLease, SEND_FLAGS_NO_BUNDLE,
    SEND_FLAGS_RELIABILITY_MASK, SEND_FLAGS_RESERVED_MASK, SEND_FLAGS_TAG_THRESHOLD_MASK,
    SEND_FLAGS_TAGS_TO_SEND_MASK, SESSION_CONFIG_MAX_SKEW_MS, SendFlags, SendMessage,
    SendMessageExpires, SessionConfig, SessionDecryptionKey, SessionStatus, SessionStatusCode,
    SetDate, decode_typed,
};
pub use payload::{
    GZIP_HEADER_LEN, GZIP_MAGIC, GZIP_METHOD_DEFLATE, GZIP_TRAILER_LEN, GZIP_XFLAGS_JAVA,
    MAX_I2CP_DECOMPRESSED_BYTES, MAX_I2CP_PAYLOAD_BYTES, PROTOCOL_DATAGRAM, PROTOCOL_DATAGRAM_RAW,
    PROTOCOL_EXPERIMENTAL_FIRST, PROTOCOL_EXPERIMENTAL_LAST, PROTOCOL_RESERVED, PROTOCOL_STREAMING,
    Payload, PayloadGzipHeader, split_payload,
};
pub use session::{
    MAX_REGISTRY_SESSION_ID, SessionEntry, SessionRegistry, SessionRegistryLimits,
    SessionReservation,
};
pub use verify::{
    Clock, FixedClock, SystemClock, VERIFICATION_SKEW_MS, VerifiedSessionConfig,
    verify_session_config,
};
