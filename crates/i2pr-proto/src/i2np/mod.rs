//! Bounded structural codecs for the initial I2NP message surface.
//!
//! This module owns wire representation only. It does not apply expiration,
//! duplicate, routing, queue, transport-authentication, NetDB, tunnel, or
//! garlic policy. Bodies that require later cryptographic or state-machine
//! work are retained in explicitly named deferred values after their framing
//! has been validated.

use std::fmt;

use crate::codec::{CodecError, DecodeCursor, EncodeBuffer, decode_exact, encode_to_vec};
use crate::{Date, Hash, LeaseSet, LeaseSet2};
use zeroize::Zeroizing;

/// The largest I2NP payload accepted by this codec.
///
/// The official I2NP documentation describes a nominal 64 KiB payload, but
/// tunnel fragmentation constrains a message to approximately 61.2 KiB.
pub const MAX_I2NP_PAYLOAD_SIZE: usize = 62_708;
/// The standard I2NP header size in bytes.
pub const STANDARD_HEADER_SIZE: usize = 16;
/// The obsolete SSU short header size in bytes.
pub const SHORT_SSU_HEADER_SIZE: usize = 5;
/// The NTCP2/SSU2 short header size in bytes.
pub const SHORT_TRANSPORT_HEADER_SIZE: usize = 9;
/// The maximum number of excluded peers in a DatabaseLookup.
pub const MAX_DATABASE_LOOKUP_EXCLUDED_PEERS: usize = 512;
/// The bounded number of peers retained in a DatabaseSearchReply.
pub const MAX_DATABASE_SEARCH_REPLY_PEERS: usize = 16;
/// The maximum number of records in a tunnel-build message.
pub const MAX_BUILD_RECORDS: usize = 8;
/// The legacy and variable tunnel-build record size.
pub const VARIABLE_BUILD_RECORD_SIZE: usize = 528;
/// The current short tunnel-build encrypted record size.
///
/// Each short tunnel-build record is a 32-byte ephemeral X25519
/// public key followed by a single ChaCha20-Poly1305 sealed
/// plaintext request record and its authentication tag.
pub const SHORT_BUILD_RECORD_SIZE: usize = 218;
/// The plaintext size of a short tunnel-build request record.
///
/// Each hop receives this plaintext after the ECIES-X25519
/// decryption. The buffer is exactly sized; builders reject any
/// payload that is not 154 bytes.
pub const SHORT_REQUEST_PLAINTEXT_SIZE: usize = 154;
/// The plaintext size of a short tunnel-build reply record.
///
/// Each hop returns this plaintext inside the ECIES-X25519 reply
/// authentication envelope. The buffer is exactly sized.
pub const SHORT_REPLY_PLAINTEXT_SIZE: usize = 202;
/// X25519 ephemeral public key prefix length in a short build
/// record. Used to pre-compute the offset of the AEAD envelope.
pub const SHORT_BUILD_EPHEMERAL_KEY_LEN: usize = 32;
/// ChaCha20-Poly1305 nonce length (in bytes) for the short
/// tunnel-build request record. The nonce is stored inside the
/// record body.
pub const SHORT_BUILD_NONCE_LEN: usize = 16;
/// ChaCha20-Poly1305 authentication tag length (in bytes) for the
/// short tunnel-build record.
pub const SHORT_BUILD_TAG_LEN: usize = 16;
/// The fixed tunnel-data payload size.
pub const TUNNEL_DATA_PAYLOAD_SIZE: usize = 1024;

mod deferred;
mod delivery;
mod header;
mod message;
mod netdb;
mod tunnel;

pub use deferred::*;
pub use delivery::*;
pub use header::*;
pub use message::*;
pub use netdb::*;
pub use tunnel::*;
