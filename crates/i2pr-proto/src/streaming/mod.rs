//! I2P client payload framing and the Streaming protocol wire codec.
//!
//! Plan 128 owns the normative Streaming wire form. The codec is kept
//! wire-only; state-machine and connection policy live in the
//! `i2pr-client` streaming layer. The [`crate::streaming::payload`]
//! module owns the protocol-6 client payload envelope that wraps every
//! Streaming packet carried inside an I2NP `Data` body.
//!
//! # Wire layers
//!
//! ```text
//! Streaming packet                 (this crate, packet.rs)
//!   -> protocol-6 client payload   (this crate, payload.rs)
//!     -> I2NP Data body            (i2pr_proto::i2np)
//!       -> ECIES Garlic Clove      (i2pr_client::session)
//!         -> outbound destination tunnel + remote LeaseSet2
//! ```

#![forbid(unsafe_code)]

pub mod packet;
pub mod payload;

pub use packet::{
    CLOSE_FLAGS, DEFAULT_ADVERTISED_MAX_PAYLOAD, FLAG_CLOSE, FLAG_DELAY_REQUESTED, FLAG_ECHO,
    FLAG_FROM_INCLUDED, FLAG_MAX_PACKET_SIZE_INCLUDED, FLAG_NO_ACK, FLAG_OFFLINE_SIGNATURE,
    FLAG_PROFILE_INTERACTIVE, FLAG_RESERVED_MASK, FLAG_RESET, FLAG_SIGNATURE_INCLUDED,
    FLAG_SIGNATURE_REQUESTED, FLAG_SYNCHRONIZE, INITIAL_SYN_FLAGS, MAX_STREAMING_NACK_COUNT,
    MAX_STREAMING_OPTION_BYTES, MAX_STREAMING_PACKET_BYTES, MAX_STREAMING_PAYLOAD_BYTES,
    MIN_STREAMING_HEADER_BYTES, RESET_FLAGS, SYN_REPLAY_NACK_COUNT, SYN_RESPONSE_FLAGS,
    SignatureLocation, StreamingFlags, StreamingHeaderPeek, StreamingOptionDecodeContext,
    StreamingOptions, StreamingPacket, StreamingPacketBuilder, StreamingPacketError,
    StreamingReceiveLimit, StreamingSendLimit, build_signature_preimage, decode_streaming_packet,
    encode_streaming_packet, encode_syn_replay_binding, install_packet_signature,
    peek_streaming_header, validate_initial_syn, validate_signature_policy, validate_syn_response,
    verify_syn_replay_binding,
};
pub use payload::{
    ClientPayload, ClientPayloadDecodeError, ClientPayloadEncodeError,
    MAX_APPLICATION_PAYLOAD_BYTES, MAX_CLIENT_PAYLOAD_BYTES, STREAMING_PROTOCOL_NUMBER,
    decode_client_payload, encode_client_payload,
};
