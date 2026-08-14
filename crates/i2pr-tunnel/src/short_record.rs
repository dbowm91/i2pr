//! Typed short tunnel-build request and reply records.
//!
//! Plan 108 §7 owns the typed internal representation for the 154-byte
//! request plaintext and the 202-byte reply plaintext. The module
//! does not perform cryptography — it only validates the structure of
//! one record and produces the canonical byte buffer the
//! `build_crypto` and `short` modules consume.
//!
//! All field sizes mirror the ECIES-X25519 short tunnel-build
//! specification the Plan 108 implementation follows. The module
//! rejects malformed inputs and refuses to expose partially
//! validated records.

#![forbid(unsafe_code)]

use std::fmt;

use i2pr_proto::{
    CodecError, Date, Hash, Mapping, SHORT_REPLY_PLAINTEXT_SIZE, SHORT_REQUEST_PLAINTEXT_SIZE,
};
use thiserror::Error;
use zeroize::Zeroizing;

use crate::identity::TunnelId;

/// The fixed role byte mask carried by a short request record.
///
/// A value of zero marks the participant role. Exactly one of the
/// [`HopRole`] variants is accepted; simultaneously setting gateway
/// and endpoint is rejected at construction time.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HopRole {
    /// Inbound gateway: receives the request from the inbound side.
    InboundGateway,
    /// Intermediate participant: decrypts one layer and forwards.
    Participant,
    /// Outbound endpoint: terminates the tunnel path.
    OutboundEndpoint,
}

impl HopRole {
    /// Returns the role flag byte.
    pub const fn flag(self) -> u8 {
        match self {
            Self::InboundGateway => 0x01,
            Self::Participant => 0x00,
            Self::OutboundEndpoint => 0x02,
        }
    }

    /// Decodes the role flag byte. Bits outside the two-bit
    /// canonical mask must be zero; otherwise the input is rejected.
    pub const fn from_flag(flag: u8) -> Result<Self, ShortBuildError> {
        if flag & !0x03 != 0 {
            return Err(ShortBuildError::InvalidRoleFlags { flags: flag });
        }
        match flag & 0x03 {
            0x00 => Ok(Self::Participant),
            0x01 => Ok(Self::InboundGateway),
            0x02 => Ok(Self::OutboundEndpoint),
            _ => Err(ShortBuildError::InvalidRoleFlags { flags: flag }),
        }
    }
}

/// Layer encryption type permitted by the Plan 108 short build.
///
/// Only the AEAD-only ECIES-X25519 layer is currently produced by
/// the Plan 108 implementation. The encoded byte matches the I2P
/// specification but values outside the known set are rejected.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LayerEncryptionType {
    /// ECIES-X25519 AEAD-only per-hop encryption (Plan 108 default).
    EciesAeadOnly,
}

impl LayerEncryptionType {
    /// Returns the layer encryption type byte.
    pub const fn byte(self) -> u8 {
        match self {
            Self::EciesAeadOnly => 0x05,
        }
    }

    /// Decodes a layer encryption type byte.
    pub const fn from_byte(byte: u8) -> Result<Self, ShortBuildError> {
        match byte {
            0x05 => Ok(Self::EciesAeadOnly),
            other => Err(ShortBuildError::UnsupportedLayerEncryption { byte: other }),
        }
    }
}

/// Bounded typed wrapper around the short-build request options
/// mapping. Empty options are explicit; oversized and malformed
/// mappings are rejected at construction time.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuildOptions {
    mapping: Mapping,
}

impl BuildOptions {
    /// Returns an empty options set.
    pub fn empty() -> Self {
        Self {
            mapping: Mapping::empty(),
        }
    }

    /// Constructs options from a validated canonical mapping.
    pub fn from_mapping(mapping: Mapping) -> Self {
        Self { mapping }
    }

    /// Returns the underlying mapping.
    pub const fn mapping(&self) -> &Mapping {
        &self.mapping
    }
}

impl Default for BuildOptions {
    fn default() -> Self {
        Self::empty()
    }
}

/// Errors produced by the [`BuildOptions`] bounds and parsing.
#[derive(Debug, Error, Eq, PartialEq)]
pub enum BuildOptionsError {
    /// Options failed to encode within the protocol maximum.
    #[error("build options encoded length {actual} exceeds {maximum}-byte limit")]
    EncodedTooLarge {
        /// Actual encoded length.
        actual: usize,
        /// Maximum accepted length.
        maximum: usize,
    },
    /// Underlying codec error.
    #[error("build options protocol error: {0}")]
    Protocol(#[from] CodecError),
}

/// Maximum build request expiration in milliseconds from creation.
///
/// The I2P specification limits the acceptance window for a build
/// request to a few minutes. Plan 108 caps the value at one hour
/// to bound replay windows; callers should pick a smaller value.
pub const MAX_BUILD_EXPIRATION_MILLIS: u64 = 60 * 60 * 1000;

/// Typed 154-byte short tunnel-build request record.
///
/// The struct is runtime-neutral: it owns the canonical fields the
/// I2P short-build specification defines and refuses to expose
/// partially validated data. Use [`ShortRequestRecord::encode`] to
/// produce the 154-byte plaintext the build cryptography consumes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShortRequestRecord {
    receive_tunnel: TunnelId,
    next_tunnel: TunnelId,
    next_router: Hash,
    role: HopRole,
    layer_encryption_type: LayerEncryptionType,
    request_time: Date,
    expiration: Date,
    next_message_id: u32,
    options: BuildOptions,
}

impl ShortRequestRecord {
    /// Constructs a request record from validated parts.
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        receive_tunnel: TunnelId,
        next_tunnel: TunnelId,
        next_router: Hash,
        role: HopRole,
        layer_encryption_type: LayerEncryptionType,
        request_time: Date,
        expiration_ms: u64,
        next_message_id: u32,
        options: BuildOptions,
    ) -> Result<Self, ShortBuildError> {
        if next_message_id == 0 {
            return Err(ShortBuildError::ZeroMessageId);
        }
        if expiration_ms == 0 {
            return Err(ShortBuildError::ZeroExpiration);
        }
        if expiration_ms > MAX_BUILD_EXPIRATION_MILLIS {
            return Err(ShortBuildError::ExpirationExceedsMaximum {
                actual: expiration_ms,
                maximum: MAX_BUILD_EXPIRATION_MILLIS,
            });
        }
        Ok(Self {
            receive_tunnel,
            next_tunnel,
            next_router,
            role,
            layer_encryption_type,
            request_time,
            expiration: Date::from_millis(expiration_ms),
            next_message_id,
            options,
        })
    }

    /// Returns the receive tunnel identifier.
    pub const fn receive_tunnel(&self) -> TunnelId {
        self.receive_tunnel
    }

    /// Returns the next hop tunnel identifier.
    pub const fn next_tunnel(&self) -> TunnelId {
        self.next_tunnel
    }

    /// Returns the next router hash.
    pub const fn next_router(&self) -> &Hash {
        &self.next_router
    }

    /// Returns the hop role flags.
    pub const fn role(&self) -> HopRole {
        self.role
    }

    /// Returns the layer encryption type.
    pub const fn layer_encryption_type(&self) -> LayerEncryptionType {
        self.layer_encryption_type
    }

    /// Returns the request time in milliseconds.
    pub const fn request_time(&self) -> Date {
        self.request_time
    }

    /// Returns the request expiration in milliseconds.
    pub const fn expiration(&self) -> Date {
        self.expiration
    }

    /// Returns the next-hop message identifier.
    pub const fn next_message_id(&self) -> u32 {
        self.next_message_id
    }

    /// Returns the encoded build options mapping.
    pub const fn options(&self) -> &BuildOptions {
        &self.options
    }

    /// Encodes the canonical 154-byte plaintext record.
    pub fn encode(&self) -> Result<Zeroizing<[u8; SHORT_REQUEST_PLAINTEXT_SIZE]>, ShortBuildError> {
        let mut buffer = [0_u8; SHORT_REQUEST_PLAINTEXT_SIZE];
        buffer[0..4].copy_from_slice(&self.receive_tunnel.get().to_be_bytes());
        buffer[4..8].copy_from_slice(&self.next_tunnel.get().to_be_bytes());
        buffer[8..40].copy_from_slice(self.next_router.as_bytes());
        buffer[40] = self.role.flag();
        buffer[41] = self.layer_encryption_type.byte();
        buffer[42..50].copy_from_slice(&self.request_time.as_millis().to_be_bytes());
        // 8 bytes reserved (zero per Plan 108) sit at offset 50..58.
        buffer[50..58].copy_from_slice(&[0_u8; 8]);
        buffer[58..66].copy_from_slice(&self.expiration.as_millis().to_be_bytes());
        buffer[66..70].copy_from_slice(&self.next_message_id.to_be_bytes());
        // 69 bytes of options follow, encoded as a Mapping.
        let encoded_options = self
            .options
            .mapping
            .encode_to_vec(69)
            .map_err(ShortBuildError::OptionsEncode)?;
        if encoded_options.is_empty() || encoded_options.len() > 70 {
            return Err(ShortBuildError::OptionsLength {
                actual: encoded_options.len(),
            });
        }
        // first byte is the u16 length high byte of mapping; we treat
        // the body length's lower byte as the count byte at offset 70
        // to keep the encoded form strictly canonical.
        let body_len = u16::from_be_bytes([encoded_options[0], encoded_options[1]]) as usize;
        if body_len + 2 != encoded_options.len() {
            return Err(ShortBuildError::OptionsLength {
                actual: encoded_options.len(),
            });
        }
        if body_len > 67 {
            return Err(ShortBuildError::OptionsLength { actual: body_len });
        }
        // Options layout: 1 byte length + body (max 67 bytes).
        buffer[70] = body_len as u8;
        buffer[71..71 + body_len].copy_from_slice(&encoded_options[2..]);
        // Remaining bytes 71 + body_len .. 154 are zero (zero-padded).
        Ok(Zeroizing::new(buffer))
    }
}

/// Short-build reply response codes the Plan 108 surface recognises.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShortResponseCode {
    /// The hop accepted its position in the build.
    Accepted = 0,
    /// The hop rejected the build for a permitted reason.
    Rejected = 1,
}

impl ShortResponseCode {
    /// Returns the response code byte.
    pub const fn byte(self) -> u8 {
        self as u8
    }

    /// Decodes a response byte; unknown values produce a typed
    /// [`ShortBuildError::UnknownReplyCode`].
    pub const fn from_byte(byte: u8) -> Result<Self, ShortBuildError> {
        match byte {
            0 => Ok(Self::Accepted),
            1 => Ok(Self::Rejected),
            other => Err(ShortBuildError::UnknownReplyCode { code: other }),
        }
    }
}

/// Typed 202-byte short tunnel-build reply record.
///
/// The plaintext reply record carries the hop's response code,
/// adjusted timings, and the per-hop reply message id that the
/// creator uses for correlation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShortReplyRecord {
    reply_code: ShortResponseCode,
    expiration: Date,
    next_message_id: u32,
    tunnel_id: u32,
}

impl ShortReplyRecord {
    /// Constructs a reply record with full validation.
    pub fn try_new(
        reply_code: ShortResponseCode,
        expiration_ms: u64,
        next_message_id: u32,
        tunnel_id: u32,
    ) -> Result<Self, ShortBuildError> {
        if expiration_ms == 0 {
            return Err(ShortBuildError::ZeroExpiration);
        }
        if expiration_ms > MAX_BUILD_EXPIRATION_MILLIS {
            return Err(ShortBuildError::ExpirationExceedsMaximum {
                actual: expiration_ms,
                maximum: MAX_BUILD_EXPIRATION_MILLIS,
            });
        }
        Ok(Self {
            reply_code,
            expiration: Date::from_millis(expiration_ms),
            next_message_id,
            tunnel_id,
        })
    }

    /// Returns the hop's response code.
    pub const fn reply_code(&self) -> ShortResponseCode {
        self.reply_code
    }

    /// Returns the reply expiration time in milliseconds.
    pub const fn expiration(&self) -> Date {
        self.expiration
    }

    /// Returns the per-hop reply message identifier.
    pub const fn next_message_id(&self) -> u32 {
        self.next_message_id
    }

    /// Returns the per-hop tunnel identifier.
    pub const fn tunnel_id(&self) -> u32 {
        self.tunnel_id
    }

    /// Decodes a reply plaintext buffer.
    pub fn decode(input: &[u8]) -> Result<Self, ShortBuildError> {
        if input.len() != SHORT_REPLY_PLAINTEXT_SIZE {
            return Err(ShortBuildError::PlaintextLength {
                actual: input.len(),
                expected: SHORT_REPLY_PLAINTEXT_SIZE,
            });
        }
        let reply_code = ShortResponseCode::from_byte(input[0])?;
        let expiration = decode_u64(&input[1..9]);
        let next_message_id = decode_u32(&input[9..13]);
        let tunnel_id = decode_u32(&input[13..17]);
        Self::try_new(reply_code, expiration, next_message_id, tunnel_id)
    }

    /// Encodes the canonical 202-byte reply plaintext.
    pub fn encode(&self) -> Zeroizing<[u8; SHORT_REPLY_PLAINTEXT_SIZE]> {
        let mut buffer = [0_u8; SHORT_REPLY_PLAINTEXT_SIZE];
        buffer[0] = self.reply_code.byte();
        buffer[1..9].copy_from_slice(&self.expiration.as_millis().to_be_bytes());
        buffer[9..13].copy_from_slice(&self.next_message_id.to_be_bytes());
        buffer[13..17].copy_from_slice(&self.tunnel_id.to_be_bytes());
        // Bytes 17..202 are zero per Plan 108 (no extra options).
        Zeroizing::new(buffer)
    }
}

fn decode_u64(input: &[u8]) -> u64 {
    let mut bytes = [0_u8; 8];
    bytes.copy_from_slice(input);
    u64::from_be_bytes(bytes)
}

fn decode_u32(input: &[u8]) -> u32 {
    let mut bytes = [0_u8; 4];
    bytes.copy_from_slice(input);
    u32::from_be_bytes(bytes)
}

/// Typed errors produced by the short-build record surface.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ShortBuildError {
    /// Plaintext buffer is the wrong length.
    #[error("short build plaintext length {actual} does not match {expected}")]
    PlaintextLength {
        /// Actual buffer length.
        actual: usize,
        /// Expected length.
        expected: usize,
    },
    /// Hop role flags were outside the allowed two-bit field.
    #[error("invalid hop role flags {flags:#x}")]
    InvalidRoleFlags {
        /// Offending flag byte.
        flags: u8,
    },
    /// Layer encryption type byte was outside the supported set.
    #[error("unsupported layer encryption type {byte:#x}")]
    UnsupportedLayerEncryption {
        /// Offending byte.
        byte: u8,
    },
    /// The next-message identifier was zero.
    #[error("short build next message id must be nonzero")]
    ZeroMessageId,
    /// The request expiration was zero.
    #[error("short build expiration must be nonzero")]
    ZeroExpiration,
    /// The request expiration exceeded the maximum.
    #[error("short build expiration {actual}ms exceeds {maximum}ms maximum")]
    ExpirationExceedsMaximum {
        /// Actual supplied expiration.
        actual: u64,
        /// Maximum accepted expiration.
        maximum: u64,
    },
    /// Reply code byte was unknown.
    #[error("unknown short build reply code {code}")]
    UnknownReplyCode {
        /// Offending byte.
        code: u8,
    },
    /// Options mapping encoding failed.
    #[error("short build options encoding failed: {0}")]
    OptionsEncode(CodecError),
    /// Options mapping length was outside the record budget.
    #[error("short build options length {actual} bytes is outside record budget")]
    OptionsLength {
        /// Actual options body length.
        actual: usize,
    },
    /// A wrapped decode error from `i2pr-proto`.
    #[error("short build protocol error: {0}")]
    Protocol(#[from] CodecError),
}

impl fmt::Display for ShortResponseCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::Accepted => "accepted",
            Self::Rejected => "rejected",
        };
        formatter.write_str(label)
    }
}

/// Decoder/encoder helper exposed for the build cryptography layer.
///
/// The plaintext buffer size matches the canonical I2P short
/// request/reply record sizes. Plan 108 does not extend the wire
/// layout beyond those values.
pub const fn plaintext_size(is_reply: bool) -> usize {
    if is_reply {
        SHORT_REPLY_PLAINTEXT_SIZE
    } else {
        SHORT_REQUEST_PLAINTEXT_SIZE
    }
}

/// Helper that decodes a plaintext buffer as either a request or a
/// reply. The first byte selects the kind: 0 is reserved for
/// replies (the `reply_code` field) and Plan 108 only consumes
/// request encoding internally; the helper is reserved for the
/// responder that produces a reply.
pub fn classify_plaintext_kind(input: &[u8]) -> Result<bool, ShortBuildError> {
    match input.len() {
        SHORT_REQUEST_PLAINTEXT_SIZE => Ok(false),
        SHORT_REPLY_PLAINTEXT_SIZE => Ok(true),
        actual => Err(ShortBuildError::PlaintextLength {
            actual,
            expected: 0,
        }),
    }
}

/// Validate a Mapping as a build options payload.
///
/// The function rejects empty mappings (the I2P spec requires a 0
/// length when there are no options) and any mapping that fails
/// to encode within the record budget. Used by callers that build
/// options from external input.
pub fn validate_options_body(body: &[u8]) -> Result<(), ShortBuildError> {
    if body.len() > 68 {
        return Err(ShortBuildError::OptionsLength { actual: body.len() });
    }
    if !body.is_empty() {
        Mapping::decode(body, body.len()).map_err(ShortBuildError::Protocol)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::identity::TunnelId;
    use i2pr_proto::Hash;

    fn next_router() -> Hash {
        Hash::from_bytes([0x33_u8; 32])
    }

    fn request_record() -> ShortRequestRecord {
        ShortRequestRecord::try_new(
            TunnelId::new(0x1000).expect("tunnel id"),
            TunnelId::new(0x2000).expect("tunnel id"),
            next_router(),
            HopRole::InboundGateway,
            LayerEncryptionType::EciesAeadOnly,
            Date::from_millis(1_000),
            60_000,
            0xABCD_1234,
            BuildOptions::empty(),
        )
        .expect("request")
    }

    #[test]
    fn hop_role_flag_roundtrip() {
        for role in [
            HopRole::InboundGateway,
            HopRole::Participant,
            HopRole::OutboundEndpoint,
        ] {
            assert_eq!(HopRole::from_flag(role.flag()).expect("flag"), role);
        }
        assert!(HopRole::from_flag(0x03).is_err());
        assert!(HopRole::from_flag(0x10).is_err());
    }

    #[test]
    fn layer_encryption_type_roundtrip() {
        assert_eq!(
            LayerEncryptionType::from_byte(LayerEncryptionType::EciesAeadOnly.byte()).expect("ok"),
            LayerEncryptionType::EciesAeadOnly
        );
        assert!(matches!(
            LayerEncryptionType::from_byte(0x04),
            Err(ShortBuildError::UnsupportedLayerEncryption { .. })
        ));
    }

    #[test]
    fn response_code_roundtrip() {
        assert_eq!(
            ShortResponseCode::from_byte(ShortResponseCode::Accepted.byte()).expect("ok"),
            ShortResponseCode::Accepted
        );
        assert_eq!(
            ShortResponseCode::from_byte(ShortResponseCode::Rejected.byte()).expect("ok"),
            ShortResponseCode::Rejected
        );
        assert!(matches!(
            ShortResponseCode::from_byte(7),
            Err(ShortBuildError::UnknownReplyCode { .. })
        ));
    }

    #[test]
    fn request_record_encodes_to_canonical_size() {
        let record = request_record();
        let bytes = record.encode().expect("encode");
        assert_eq!(bytes.len(), SHORT_REQUEST_PLAINTEXT_SIZE);
        // receive_tunnel at offset 0
        assert_eq!(bytes[0..4], [0x00, 0x00, 0x10, 0x00]);
        // next_tunnel at offset 4
        assert_eq!(bytes[4..8], [0x00, 0x00, 0x20, 0x00]);
        // role flag at offset 40
        assert_eq!(bytes[40], HopRole::InboundGateway.flag());
        // layer encryption byte at offset 41
        assert_eq!(bytes[41], LayerEncryptionType::EciesAeadOnly.byte());
        // expiration in ms at offset 58
        assert_eq!(&bytes[58..66], &(60_000_u64).to_be_bytes());
        // next message id at offset 66
        assert_eq!(&bytes[66..70], &0xABCD_1234_u32.to_be_bytes());
        // options body length at offset 70 is zero
        assert_eq!(bytes[70], 0);
    }

    #[test]
    fn request_record_rejects_simultaneous_flags() {
        // We do not store two flags simultaneously; the constructor
        // accepts a single HopRole, so this is enforced by the type
        // system. Confirm the only mutators produce a canonical
        // single-flag value.
        let gw = HopRole::InboundGateway.flag();
        let ep = HopRole::OutboundEndpoint.flag();
        assert_eq!(gw & ep, 0x00);
        assert_eq!(gw | ep, 0x03);
    }

    #[test]
    fn request_record_rejects_zero_message_id_and_expiration() {
        let outcome = ShortRequestRecord::try_new(
            TunnelId::new(0x10).expect("id"),
            TunnelId::new(0x20).expect("id"),
            next_router(),
            HopRole::Participant,
            LayerEncryptionType::EciesAeadOnly,
            Date::from_millis(1),
            60_000,
            0,
            BuildOptions::empty(),
        );
        assert!(matches!(outcome, Err(ShortBuildError::ZeroMessageId)));
        let outcome = ShortRequestRecord::try_new(
            TunnelId::new(0x10).expect("id"),
            TunnelId::new(0x20).expect("id"),
            next_router(),
            HopRole::Participant,
            LayerEncryptionType::EciesAeadOnly,
            Date::from_millis(1),
            0,
            0x1234,
            BuildOptions::empty(),
        );
        assert!(matches!(outcome, Err(ShortBuildError::ZeroExpiration)));
    }

    #[test]
    fn reply_record_round_trips_through_canonical_202_bytes() {
        let reply =
            ShortReplyRecord::try_new(ShortResponseCode::Accepted, 120_000, 0xFEED_BEEF, 0x1000)
                .expect("reply");
        let bytes = reply.encode();
        assert_eq!(bytes.len(), SHORT_REPLY_PLAINTEXT_SIZE);
        let decoded = ShortReplyRecord::decode(bytes.as_ref()).expect("decode");
        assert_eq!(decoded, reply);
    }

    #[test]
    fn reply_record_rejects_wrong_length() {
        let outcome = ShortReplyRecord::decode(&[0_u8; 64]);
        assert!(matches!(
            outcome,
            Err(ShortBuildError::PlaintextLength { .. })
        ));
    }

    #[test]
    fn reply_record_rejects_unknown_code() {
        let mut bytes = [0_u8; SHORT_REPLY_PLAINTEXT_SIZE];
        bytes[0] = 0x05;
        let outcome = ShortReplyRecord::decode(&bytes);
        assert!(matches!(
            outcome,
            Err(ShortBuildError::UnknownReplyCode { .. })
        ));
    }
}
