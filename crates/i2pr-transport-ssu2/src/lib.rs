//! Runtime-neutral SSU2 v2 protocol implementation.
//!
//! Plan 155 established strict, vector-backed address/header/block
//! primitives. Plan 156 adds the complete establishment protocol:
//! the Noise XK transcript (`crypto`), the TokenRequest/Retry/
//! SessionRequest/SessionCreated/SessionConfirmed codecs with cheap
//! prevalidation and RouterInfo binding (`handshake`), the bounded
//! one-use token lifecycle (`token`), and the consuming
//! initiator/responder state machines (`state_machine`).
//!
//! This crate owns protocol values and sequencing only: no Tokio, no
//! sockets, no filesystem I/O, no async functions, no timers, and no
//! task ownership. The data phase lives in `session` (Plan 157); the
//! supervised UDP adapter in `i2pr-runtime` belongs to Plan 158.
//!
//! Normative traceability: `specs/protocols/09-ssu2.md` and the
//! Milestone 8 source refresh in `specs/SOURCES.md` (I2P website
//! commit `88596022920bdf99f27db27688faf4f204792fcd`; SSU2 page
//! `Completed`, accurate for 0.9.69). Classical SSU2 v2 is the
//! target; PQ-hybrid v3/v4 is deferred compatibility-watch debt and
//! SSU1 remains unsupported. No socket/runtime or router-to-router
//! interoperability is claimed by this crate.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod address;
pub mod block;
pub mod constants;
pub mod crypto;
pub mod handshake;
pub mod header;
pub mod packet;
pub mod session;
pub mod state_machine;
pub mod token;

pub use address::{
    ConfiguredListenAddress, ResolvedDialTarget, Ssu2AddressClass, Ssu2AddressError,
    Ssu2AddressMaterial, Ssu2Capabilities, Ssu2Endpoint, Ssu2Introducer, Ssu2RouterAddress,
    Ssu2TransportStyle,
};
pub use block::{
    AckBlock, AddressBlock, Block, BlockError, CongestionBlock, DecodedBlock, FirstFragmentBlock,
    FirstPacketNumberBlock, FollowOnFragmentBlock, I2npMessageBlock, NewTokenBlock, OptionsBlock,
    PaddingBlock, ParsedBlocks, PathChallengeBlock, PathResponseBlock, PeerTestBlock,
    RelayIntroBlock, RelayRequestBlock, RelayResponseBlock, RelayResponseCode, RelayTagBlock,
    RouterInfoBlock, TerminationBlock, TerminationReason, TimestampBlock,
};
pub use crypto::{
    DataCipher, DataDirectionKeys, IntroKey, Role, Ssu2CryptoError, Ssu2PublicKey, Ssu2SplitKeys,
    Ssu2Transcript, TranscriptHash, derive_data_keys, open_token_payload, protocol_initial_hash,
    session_confirmed_header_key, session_created_header_key,
};
pub use handshake::{
    AuthenticatedPeer, ClockSkewPolicy, ConfirmedReassembly, HandshakeError, HandshakeReplayCache,
    ReplayDecision, ReplayToken, RetryMessage, RouterInfoFreshness, SessionCreatedParts,
    SessionRequestParts, TokenRequest, build_confirmed_payload, build_retry,
    build_session_confirmed, build_session_created, build_session_request, build_token_request,
    parse_retry, parse_session_created, parse_session_request, parse_token_request,
    prevalidate_long_datagram, require_first_router_info, require_timestamp, split_confirmed_jumbo,
    validate_router_info,
};
pub use header::{
    DataHeader, HeaderError, HeaderForm, LongHeader, MessageType, SessionConfirmedHeader,
};
pub use packet::{DatagramLengthClass, PacketError};
pub use session::{
    DropReason, ReceiveOutcome, SessionAction, SessionConfig, SessionCounters, SessionError,
    SessionEvent, Ssu2Session,
};
pub use state_machine::{
    AuthenticatedSsu2Session, ConfirmedParams, DatagramBytes, DeadlineKind, DropCategory,
    HandshakeAction, Initiator, InitiatorConfig, InitiatorSecrets, Responder, ResponderConfig,
    ResponderParams, RetryAnswer, StateMachineError, TerminateReason,
};
pub use token::{Ssu2Token, TokenError, TokenStore, retry_response_budget};
