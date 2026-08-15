//! Typed short tunnel-build request and reply records.
//!
//! Plan 109 §6 + §10 own the canonical 154-byte short request
//! plaintext and the canonical 202-byte short reply plaintext.
//! Plan 112 §5 extends the encoder so production construction
//! receives randomness explicitly via [`ShortRequestRecord::encode_with_rng`]
//! and [`ShortReplyRecord::encode_with_rng`]. The module does not
//! perform cryptography; it only validates the structure of one
//! record and produces the canonical byte buffer the
//! `build_crypto` and `short` modules consume.
//!
//! All field sizes mirror the current official I2P Tunnel
//! Creation Specification that Plan 109 implements:
//!
//! - the request plaintext uses the 154-byte layout with a fixed
//!   56-byte prefix followed by a canonical `Mapping` and
//!   random padding through byte 153;
//! - the reply plaintext uses the 202-byte layout with a
//!   canonical `Mapping` at offset 0, random padding through
//!   byte 200, and a one-byte response code at byte 201;
//! - the role flags, layer-encryption type, request-time minute
//!   encoding, and expiration encoding are exact
//!   (`Participant = 0x00`, `InboundGateway = 0x80`,
//!   `OutboundEndpoint = 0x40`, `AES = 0x00`, time/floor(unix/60),
//!   expiration seconds since creation).
//!
//! Malformed inputs are rejected; the module never exposes a
//! partially validated record.

#![forbid(unsafe_code)]

use std::fmt;

use i2pr_proto::{
    CodecError, Date, Hash, Mapping, SHORT_REPLY_PLAINTEXT_SIZE, SHORT_REQUEST_PLAINTEXT_SIZE,
};
use rand_core::{CryptoRng, RngCore, TryRngCore};
use thiserror::Error;
use zeroize::Zeroizing;

use crate::identity::TunnelId;

/// Length of the request plaintext Mapping region (Mapping + padding budget).
const MAX_REQUEST_MAPPING_AREA: usize = 98;
const MAX_REQUEST_MAPPING_BODY: usize = MAX_REQUEST_MAPPING_AREA - 2;
/// Length of the fixed prefix preceding the options Mapping in a request.
pub const REQUEST_FIXED_PREFIX_LEN: usize = 56;

/// Length of the reply plaintext Mapping region (Mapping + padding budget).
const MAX_REPLY_MAPPING_AREA: usize = 201;
const MAX_REPLY_MAPPING_BODY: usize = MAX_REPLY_MAPPING_AREA - 2;

/// The current fixed role flag byte for a participant hop.
pub const HOP_ROLE_PARTICIPANT: u8 = 0x00;
/// The current fixed role flag byte for an inbound gateway hop.
pub const HOP_ROLE_INBOUND_GATEWAY: u8 = 0x80;
/// The current fixed role flag byte for an outbound endpoint hop.
pub const HOP_ROLE_OUTBOUND_ENDPOINT: u8 = 0x40;

/// Hop role carried by the short request record.
///
/// The normative byte values follow the I2P Tunnel Creation
/// Specification: `0x00` for participants, `0x80` for an inbound
/// gateway, and `0x40` for an outbound endpoint. Setting both the
/// gateway and endpoint bits is rejected at construction time,
/// and any other undefined high bits are refused.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HopRole {
    /// Intermediate participant: decrypts one layer and forwards.
    Participant,
    /// Inbound gateway: receives the request from the inbound side.
    InboundGateway,
    /// Outbound endpoint: terminates the tunnel path.
    OutboundEndpoint,
}

impl HopRole {
    /// Returns the normative role flag byte.
    pub const fn flag(self) -> u8 {
        match self {
            Self::Participant => HOP_ROLE_PARTICIPANT,
            Self::InboundGateway => HOP_ROLE_INBOUND_GATEWAY,
            Self::OutboundEndpoint => HOP_ROLE_OUTBOUND_ENDPOINT,
        }
    }

    /// Decodes a role flag byte. The I2P specification requires
    /// that exactly one of the high-bit role flags is set (or zero
    /// for participant). Any other bit pattern is rejected.
    pub const fn from_flag(flag: u8) -> Result<Self, ShortBuildError> {
        match flag {
            HOP_ROLE_PARTICIPANT => Ok(Self::Participant),
            HOP_ROLE_INBOUND_GATEWAY => Ok(Self::InboundGateway),
            HOP_ROLE_OUTBOUND_ENDPOINT => Ok(Self::OutboundEndpoint),
            other => Err(ShortBuildError::InvalidRoleFlags { flags: other }),
        }
    }
}

/// Layer encryption type carried by the short request record.
///
/// The current specification only defines `0` (AES). Construction
/// rejects any other value; decoding mirrors the construction rule
/// without an explicit `UnsupportedLayerEncryption` category.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LayerEncryptionType {
    /// AES tunnel-layer encryption (current I2P specification value `0`).
    Aes,
}

impl LayerEncryptionType {
    /// Returns the normative layer-encryption type byte.
    pub const fn byte(self) -> u8 {
        match self {
            Self::Aes => 0x00,
        }
    }

    /// Decodes the layer-encryption type byte. Unknown values are
    /// rejected fail-closed; the surface refuses to expose a
    /// non-AES layer encryption type until the I2P specification
    /// publishes another canonical byte.
    pub const fn from_byte(byte: u8) -> Result<Self, ShortBuildError> {
        match byte {
            0x00 => Ok(Self::Aes),
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
    pub fn from_mapping(mapping: Mapping) -> Result<Self, ShortBuildError> {
        let body_len = mapping
            .encoded_body_len()
            .map_err(ShortBuildError::Protocol)?;
        if body_len > MAX_REQUEST_MAPPING_BODY {
            return Err(ShortBuildError::OptionsLength { actual: body_len });
        }
        Ok(Self { mapping })
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

/// Validation failures for [`BuildOptions`].
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

/// Fixed expiration window for a short tunnel-build request.
///
/// The I2P specification currently mandates an expiration of
/// exactly 600 seconds from the creation time. Plan 109 enforces
/// this constant for every hop the builder produces; the variable
/// lifetime work belongs to a different layer.
pub const REQUEST_EXPIRATION_SECONDS: u32 = 600;

/// Typed 154-byte short tunnel-build request record.
///
/// The struct is runtime-neutral: it owns the canonical fields
/// the I2P short-build specification defines and refuses to
/// expose partially validated data. Use
/// [`ShortRequestRecord::encode`] to produce the 154-byte
/// plaintext the build cryptography consumes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShortRequestRecord {
    receive_tunnel: TunnelId,
    next_tunnel: TunnelId,
    next_router: Hash,
    role: HopRole,
    layer_encryption_type: LayerEncryptionType,
    request_time: Date,
    expiration_seconds: u32,
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
        expiration_seconds: u32,
        next_message_id: u32,
        options: BuildOptions,
    ) -> Result<Self, ShortBuildError> {
        if next_message_id == 0 {
            return Err(ShortBuildError::ZeroMessageId);
        }
        if expiration_seconds == 0 {
            return Err(ShortBuildError::ZeroExpiration);
        }
        if expiration_seconds != REQUEST_EXPIRATION_SECONDS {
            return Err(ShortBuildError::ExpirationMismatch {
                actual: expiration_seconds,
                expected: REQUEST_EXPIRATION_SECONDS,
            });
        }
        Ok(Self {
            receive_tunnel,
            next_tunnel,
            next_router,
            role,
            layer_encryption_type,
            request_time,
            expiration_seconds,
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

    /// Returns the hop role.
    pub const fn role(&self) -> HopRole {
        self.role
    }

    /// Returns the layer encryption type.
    pub const fn layer_encryption_type(&self) -> LayerEncryptionType {
        self.layer_encryption_type
    }

    /// Returns the request time (milliseconds since the Unix epoch).
    pub const fn request_time(&self) -> Date {
        self.request_time
    }

    /// Returns the request expiration in seconds since creation.
    pub const fn expiration_seconds(&self) -> u32 {
        self.expiration_seconds
    }

    /// Returns the next-hop message identifier.
    pub const fn next_message_id(&self) -> u32 {
        self.next_message_id
    }

    /// Returns the build options mapping.
    pub const fn options(&self) -> &BuildOptions {
        &self.options
    }

    /// Computes the wire `floor(unix_seconds / 60)` minute value
    /// the build encodes. The conversion refuses truncation on
    /// overflow rather than emitting a wrong minute.
    pub const fn request_time_minutes(&self) -> Result<u32, ShortBuildError> {
        let millis = self.request_time.as_millis();
        let seconds = millis / 1_000;
        if seconds > u32::MAX as u64 {
            return Err(ShortBuildError::RequestTimeOverflow { millis });
        }
        Ok((seconds / 60) as u32)
    }

    /// Decode a 154-byte request plaintext into a validated record.
    /// The decoder validates every protocol-mandated field and
    /// rejects unknown role/layer-encryption/mapping bytes. It is
    /// the postprocessor's source of authenticated hop metadata.
    pub fn decode(input: &[u8]) -> Result<Self, ShortBuildError> {
        if input.len() != SHORT_REQUEST_PLAINTEXT_SIZE {
            return Err(ShortBuildError::PlaintextLength {
                actual: input.len(),
                expected: SHORT_REQUEST_PLAINTEXT_SIZE,
            });
        }
        let receive_tunnel_bytes: [u8; 4] =
            input[0..4]
                .try_into()
                .map_err(|_| ShortBuildError::PlaintextLength {
                    actual: input.len(),
                    expected: SHORT_REQUEST_PLAINTEXT_SIZE,
                })?;
        let receive_tunnel_value = u32::from_be_bytes(receive_tunnel_bytes);
        if receive_tunnel_value == 0 {
            return Err(ShortBuildError::ZeroTunnelId {
                field: "receive_tunnel",
            });
        }
        let receive_tunnel =
            TunnelId::new(receive_tunnel_value).map_err(|_| ShortBuildError::ZeroTunnelId {
                field: "receive_tunnel",
            })?;
        let next_tunnel_bytes: [u8; 4] =
            input[4..8]
                .try_into()
                .map_err(|_| ShortBuildError::PlaintextLength {
                    actual: input.len(),
                    expected: SHORT_REQUEST_PLAINTEXT_SIZE,
                })?;
        let next_tunnel_value = u32::from_be_bytes(next_tunnel_bytes);
        if next_tunnel_value == 0 {
            return Err(ShortBuildError::ZeroTunnelId {
                field: "next_tunnel",
            });
        }
        let next_tunnel =
            TunnelId::new(next_tunnel_value).map_err(|_| ShortBuildError::ZeroTunnelId {
                field: "next_tunnel",
            })?;
        let mut next_router_bytes = [0_u8; 32];
        next_router_bytes.copy_from_slice(&input[8..40]);
        let next_router = Hash::from_bytes(next_router_bytes);
        let role = HopRole::from_flag(input[40])?;
        // Bytes 41..42 must be zero in the canonical encoding.
        if input[41] != 0 || input[42] != 0 {
            return Err(ShortBuildError::InvalidRequestPrefixBytes);
        }
        let layer_encryption_type = LayerEncryptionType::from_byte(input[43])?;
        let minutes_bytes: [u8; 4] =
            input[44..48]
                .try_into()
                .map_err(|_| ShortBuildError::PlaintextLength {
                    actual: input.len(),
                    expected: SHORT_REQUEST_PLAINTEXT_SIZE,
                })?;
        let minutes = u32::from_be_bytes(minutes_bytes);
        let request_time_ms = (minutes as u64).saturating_mul(60).saturating_mul(1_000);
        let request_time = Date::from_millis(request_time_ms);
        let expiration_bytes: [u8; 4] =
            input[48..52]
                .try_into()
                .map_err(|_| ShortBuildError::PlaintextLength {
                    actual: input.len(),
                    expected: SHORT_REQUEST_PLAINTEXT_SIZE,
                })?;
        let expiration_seconds = u32::from_be_bytes(expiration_bytes);
        if expiration_seconds == 0 {
            return Err(ShortBuildError::ZeroExpiration);
        }
        if expiration_seconds != REQUEST_EXPIRATION_SECONDS {
            return Err(ShortBuildError::ExpirationMismatch {
                actual: expiration_seconds,
                expected: REQUEST_EXPIRATION_SECONDS,
            });
        }
        let next_message_id_bytes: [u8; 4] =
            input[52..56]
                .try_into()
                .map_err(|_| ShortBuildError::PlaintextLength {
                    actual: input.len(),
                    expected: SHORT_REQUEST_PLAINTEXT_SIZE,
                })?;
        let next_message_id = u32::from_be_bytes(next_message_id_bytes);
        if next_message_id == 0 {
            return Err(ShortBuildError::ZeroMessageId);
        }
        let mapping_area = &input[REQUEST_FIXED_PREFIX_LEN..];
        let mapping = if mapping_area.len() < 2 {
            return Err(ShortBuildError::OptionsLength {
                actual: mapping_area.len(),
            });
        } else {
            let declared_body = u16::from_be_bytes([mapping_area[0], mapping_area[1]]) as usize;
            if declared_body > MAX_REQUEST_MAPPING_BODY {
                return Err(ShortBuildError::Protocol(CodecError::LengthExceeded {
                    offset: 0,
                    declared: declared_body,
                    maximum: MAX_REQUEST_MAPPING_BODY,
                    context: "request mapping body",
                }));
            }
            let mapping_len = 2 + declared_body;
            if mapping_len > mapping_area.len() {
                return Err(ShortBuildError::Protocol(CodecError::Truncated {
                    offset: mapping_area.len(),
                    needed: mapping_len,
                    remaining: mapping_area.len(),
                }));
            }
            Mapping::decode(&mapping_area[..mapping_len], MAX_REQUEST_MAPPING_AREA)
                .map_err(ShortBuildError::Protocol)?
        };
        let options = BuildOptions::from_mapping(mapping)?;
        Ok(Self {
            receive_tunnel,
            next_tunnel,
            next_router,
            role,
            layer_encryption_type,
            request_time,
            expiration_seconds,
            next_message_id,
            options,
        })
    }

    /// Encodes the canonical 154-byte plaintext record, filling the
    /// post-Mapping random padding from the supplied CSPRNG. The
    /// final I2P Tunnel Creation Specification requires the bytes
    /// `56 + encoded_mapping_len .. 154` to be random; production
    /// callers must use this method rather than the
    /// [`Self::encode_deterministic_zero_padded`] fallback.
    pub fn encode_with_rng<R: CryptoRng + RngCore>(
        &self,
        rng: &mut R,
    ) -> Result<Zeroizing<[u8; SHORT_REQUEST_PLAINTEXT_SIZE]>, ShortBuildError> {
        let mut buffer = [0_u8; SHORT_REQUEST_PLAINTEXT_SIZE];
        buffer[0..4].copy_from_slice(&self.receive_tunnel.get().to_be_bytes());
        buffer[4..8].copy_from_slice(&self.next_tunnel.get().to_be_bytes());
        buffer[8..40].copy_from_slice(self.next_router.as_bytes());
        buffer[40] = self.role.flag();
        // Additional flag bytes 41..42 must be zero. The layer
        // encryption type occupies byte 43.
        buffer[41] = 0;
        buffer[42] = 0;
        buffer[43] = self.layer_encryption_type.byte();
        let minutes = self.request_time_minutes()?;
        buffer[44..48].copy_from_slice(&minutes.to_be_bytes());
        buffer[48..52].copy_from_slice(&self.expiration_seconds.to_be_bytes());
        buffer[52..56].copy_from_slice(&self.next_message_id.to_be_bytes());
        // Encoding at offset 56: the canonical Mapping with a
        // two-byte length prefix.
        let encoded_options = self
            .options
            .mapping
            .encode_to_vec(MAX_REQUEST_MAPPING_AREA)
            .map_err(ShortBuildError::OptionsEncode)?;
        if encoded_options.len() > MAX_REQUEST_MAPPING_AREA {
            return Err(ShortBuildError::OptionsLength {
                actual: encoded_options.len(),
            });
        }
        let mapping_end = REQUEST_FIXED_PREFIX_LEN + encoded_options.len();
        buffer[REQUEST_FIXED_PREFIX_LEN..mapping_end].copy_from_slice(&encoded_options);
        // Plan 112 §5.A2: fill the post-Mapping padding
        // (`mapping_end .. SHORT_REQUEST_PLAINTEXT_SIZE`) from the
        // supplied CSPRNG. RNG failure fails closed.
        let padding = &mut buffer[mapping_end..SHORT_REQUEST_PLAINTEXT_SIZE];
        rng.try_fill_bytes(padding)
            .map_err(|_| ShortBuildError::RandomnessUnavailable)?;
        Ok(Zeroizing::new(buffer))
    }

    /// Deterministic zero-padded request encoder. The byte layout
    /// after Mapping is filled with zeros rather than random bytes
    /// so fixture-style tests can assert exact padding bytes. This
    /// method is **not** spec-conformant and must not be used in
    /// production; it exists solely for deterministic unit tests
    /// and the legacy fixed-vector conformance fixture.
    pub fn encode_deterministic_zero_padded(
        &self,
    ) -> Result<Zeroizing<[u8; SHORT_REQUEST_PLAINTEXT_SIZE]>, ShortBuildError> {
        let mut buffer = [0_u8; SHORT_REQUEST_PLAINTEXT_SIZE];
        buffer[0..4].copy_from_slice(&self.receive_tunnel.get().to_be_bytes());
        buffer[4..8].copy_from_slice(&self.next_tunnel.get().to_be_bytes());
        buffer[8..40].copy_from_slice(self.next_router.as_bytes());
        buffer[40] = self.role.flag();
        buffer[41] = 0;
        buffer[42] = 0;
        buffer[43] = self.layer_encryption_type.byte();
        let minutes = self.request_time_minutes()?;
        buffer[44..48].copy_from_slice(&minutes.to_be_bytes());
        buffer[48..52].copy_from_slice(&self.expiration_seconds.to_be_bytes());
        buffer[52..56].copy_from_slice(&self.next_message_id.to_be_bytes());
        let encoded_options = self
            .options
            .mapping
            .encode_to_vec(MAX_REQUEST_MAPPING_AREA)
            .map_err(ShortBuildError::OptionsEncode)?;
        if encoded_options.len() > MAX_REQUEST_MAPPING_AREA {
            return Err(ShortBuildError::OptionsLength {
                actual: encoded_options.len(),
            });
        }
        buffer[REQUEST_FIXED_PREFIX_LEN..REQUEST_FIXED_PREFIX_LEN + encoded_options.len()]
            .copy_from_slice(&encoded_options);
        // Remaining bytes (mapping_end .. 154) are intentionally
        // left zero. The spec requires them to be random; this
        // method is test-only.
        Ok(Zeroizing::new(buffer))
    }

    /// Backwards-compatible alias for
    /// [`Self::encode_deterministic_zero_padded`]. Existing test
    /// callers and the legacy conformance fixtures keep using this
    /// surface; production paths must migrate to
    /// [`Self::encode_with_rng`].
    #[deprecated(
        note = "use encode_with_rng for production; encode_deterministic_zero_padded for tests"
    )]
    pub fn encode(&self) -> Result<Zeroizing<[u8; SHORT_REQUEST_PLAINTEXT_SIZE]>, ShortBuildError> {
        self.encode_deterministic_zero_padded()
    }
}

/// Short-build reply response codes the Plan 109 surface recognises.
///
/// The I2P specification currently publishes the bandwidth
/// rejection code `30`. The success code is `0`. Additional
/// response values may be rejected unless an authorised I2P
/// specification update introduces them.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShortResponseCode {
    /// The hop accepted its position in the build.
    Accepted,
    /// The hop rejected the build for the current defined reason.
    BandwidthRejected,
}

impl ShortResponseCode {
    /// Returns the normative response byte.
    pub const fn byte(self) -> u8 {
        match self {
            Self::Accepted => 0x00,
            Self::BandwidthRejected => 30,
        }
    }

    /// Decodes a response byte; unknown values produce a typed
    /// [`ShortBuildError::UnknownReplyCode`].
    pub const fn from_byte(byte: u8) -> Result<Self, ShortBuildError> {
        match byte {
            0x00 => Ok(Self::Accepted),
            30 => Ok(Self::BandwidthRejected),
            other => Err(ShortBuildError::UnknownReplyCode { code: other }),
        }
    }
}

impl fmt::Display for ShortResponseCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Accepted => formatter.write_str("accepted"),
            Self::BandwidthRejected => formatter.write_str("bandwidth-rejected"),
        }
    }
}

/// Typed 202-byte short tunnel-build reply record.
///
/// The plaintext reply record carries the hop's response code
/// and the hop's own canonical options Mapping. The reply byte
/// lives at offset 201; the Mapping begins at offset 0.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShortReplyRecord {
    options: BuildOptions,
    response: ShortResponseCode,
}

impl ShortReplyRecord {
    /// Constructs a reply record with full validation.
    pub fn new(options: BuildOptions, response: ShortResponseCode) -> Self {
        Self { options, response }
    }

    /// Returns the reply options mapping.
    pub fn options(&self) -> &BuildOptions {
        &self.options
    }

    /// Returns the reply response code.
    pub const fn response(&self) -> ShortResponseCode {
        self.response
    }

    /// Decodes a reply plaintext buffer.
    pub fn decode(input: &[u8]) -> Result<Self, ShortBuildError> {
        if input.len() != SHORT_REPLY_PLAINTEXT_SIZE {
            return Err(ShortBuildError::PlaintextLength {
                actual: input.len(),
                expected: SHORT_REPLY_PLAINTEXT_SIZE,
            });
        }
        let response = ShortResponseCode::from_byte(input[SHORT_REPLY_PLAINTEXT_SIZE - 1])?;
        // The Mapping covers bytes `0..201` and the response byte is
        // at index 201.  For an empty Mapping the canonical encoding
        // is exactly two bytes `00 00`; any remaining bytes in the
        // 201-byte mapping area are random padding and not validated.
        // Slice only the encoded prefix so the strict codec does not
        // reject the trailing random padding.
        let mapping_area = &input[..SHORT_REPLY_PLAINTEXT_SIZE - 1];
        let mapping = if mapping_area.len() < 2 {
            Mapping::empty()
        } else {
            // Read the declared body length from the first two bytes,
            // then take exactly that many bytes for the Mapping body.
            let declared_body = u16::from_be_bytes([mapping_area[0], mapping_area[1]]) as usize;
            if declared_body > MAX_REPLY_MAPPING_BODY {
                return Err(ShortBuildError::Protocol(CodecError::LengthExceeded {
                    offset: 0,
                    declared: declared_body,
                    maximum: MAX_REPLY_MAPPING_BODY,
                    context: "reply mapping body",
                }));
            }
            let mapping_len = 2 + declared_body;
            if mapping_len > mapping_area.len() {
                return Err(ShortBuildError::Protocol(CodecError::Truncated {
                    offset: mapping_area.len(),
                    needed: mapping_len,
                    remaining: mapping_area.len(),
                }));
            }
            Mapping::decode(&mapping_area[..mapping_len], MAX_REPLY_MAPPING_AREA)
                .map_err(ShortBuildError::Protocol)?
        };
        let options = BuildOptions::from_mapping(mapping)?;
        Ok(Self { options, response })
    }

    /// Encodes the canonical 202-byte reply plaintext, filling the
    /// post-Mapping random padding from the supplied CSPRNG. The
    /// final I2P Tunnel Creation Specification requires the bytes
    /// `encoded_mapping_len .. 201` to be random; production
    /// callers must use this method rather than the
    /// [`Self::encode_deterministic_zero_padded`] fallback.
    pub fn encode_with_rng<R: CryptoRng + RngCore>(
        &self,
        rng: &mut R,
    ) -> Result<Zeroizing<[u8; SHORT_REPLY_PLAINTEXT_SIZE]>, ShortBuildError> {
        let mut buffer = [0_u8; SHORT_REPLY_PLAINTEXT_SIZE];
        let encoded = self
            .options
            .mapping
            .encode_to_vec(MAX_REPLY_MAPPING_AREA)
            .map_err(ShortBuildError::OptionsEncode)?;
        if encoded.len() > SHORT_REPLY_PLAINTEXT_SIZE - 1 {
            return Err(ShortBuildError::OptionsLength {
                actual: encoded.len(),
            });
        }
        let mapping_end = encoded.len();
        buffer[..mapping_end].copy_from_slice(&encoded);
        // Plan 112 §5.A3: fill the post-Mapping padding
        // (`mapping_end .. SHORT_REPLY_PLAINTEXT_SIZE - 1`) from
        // the supplied CSPRNG. The response byte lives at offset
        // `SHORT_REPLY_PLAINTEXT_SIZE - 1` and is **not** part of
        // the random region. RNG failure fails closed.
        let padding = &mut buffer[mapping_end..SHORT_REPLY_PLAINTEXT_SIZE - 1];
        rng.try_fill_bytes(padding)
            .map_err(|_| ShortBuildError::RandomnessUnavailable)?;
        buffer[SHORT_REPLY_PLAINTEXT_SIZE - 1] = self.response.byte();
        Ok(Zeroizing::new(buffer))
    }

    /// Deterministic zero-padded reply encoder. The byte layout
    /// after Mapping is filled with zeros rather than random bytes
    /// so fixture-style tests can assert exact padding bytes. This
    /// method is **not** spec-conformant and must not be used in
    /// production; it exists solely for deterministic unit tests
    /// and the legacy fixed-vector conformance fixture.
    pub fn encode_deterministic_zero_padded(&self) -> Zeroizing<[u8; SHORT_REPLY_PLAINTEXT_SIZE]> {
        let mut buffer = [0_u8; SHORT_REPLY_PLAINTEXT_SIZE];
        let encoded = self
            .options
            .mapping
            .encode_to_vec(MAX_REPLY_MAPPING_AREA)
            .map_err(ShortBuildError::OptionsEncode)
            .unwrap_or_default();
        if encoded.len() > SHORT_REPLY_PLAINTEXT_SIZE - 1 {
            return Zeroizing::new([0_u8; SHORT_REPLY_PLAINTEXT_SIZE]);
        }
        buffer[..encoded.len()].copy_from_slice(&encoded);
        // The padding bytes between the end of the Mapping and
        // the response byte are intentionally zeroed. The spec
        // requires them to be random; this method is test-only.
        buffer[SHORT_REPLY_PLAINTEXT_SIZE - 1] = self.response.byte();
        Zeroizing::new(buffer)
    }

    /// Backwards-compatible alias for
    /// [`Self::encode_deterministic_zero_padded`]. Existing test
    /// callers and the legacy conformance fixtures keep using this
    /// surface; production paths must migrate to
    /// [`Self::encode_with_rng`].
    #[deprecated(
        note = "use encode_with_rng for production; encode_deterministic_zero_padded for tests"
    )]
    pub fn encode(&self) -> Zeroizing<[u8; SHORT_REPLY_PLAINTEXT_SIZE]> {
        self.encode_deterministic_zero_padded()
    }
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
    /// Hop role flag byte did not match a normative single-bit value.
    #[error("invalid hop role flags {flags:#x}")]
    InvalidRoleFlags {
        /// Offending flag byte.
        flags: u8,
    },
    /// A non-prefix byte at offset 41..42 of the canonical
    /// request plaintext held an unexpected non-zero value.
    #[error("short build request plaintext carried unexpected prefix bytes")]
    InvalidRequestPrefixBytes,
    /// A required four-byte tunnel identifier was zero in the
    /// decoded request plaintext.
    #[error("short build request plaintext carried a zero {field} tunnel id")]
    ZeroTunnelId {
        /// Field name carrying the zero tunnel id.
        field: &'static str,
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
    /// The request expiration disagreed with the I2P-mandated 600-second window.
    #[error("short build expiration {actual}s differs from mandatory {expected}s")]
    ExpirationMismatch {
        /// Actual supplied expiration.
        actual: u32,
        /// Mandated expiration.
        expected: u32,
    },
    /// The request time overflowed the wire minutes conversion.
    #[error("short build request time {millis}ms overflows the minute wire conversion")]
    RequestTimeOverflow {
        /// Original millisecond timestamp.
        millis: u64,
    },
    /// Reply code byte was not in the recognised set.
    #[error("unknown short build reply code {code}")]
    UnknownReplyCode {
        /// Offending byte.
        code: u8,
    },
    /// Options mapping encoding failed.
    #[error("short build options encoding failed: {0}")]
    OptionsEncode(CodecError),
    /// Options mapping length was outside the record budget.
    #[error("short build options encoded length {actual} is outside record budget")]
    OptionsLength {
        /// Actual encoded options length.
        actual: usize,
    },
    /// The cryptographic RNG was unable to produce output for the
    /// protocol padding bytes. Plan 112 §5.A4 mandates that any
    /// `try_fill_bytes` failure fail closed rather than silently
    /// fall back to zero padding.
    #[error("short build protocol RNG unavailable")]
    RandomnessUnavailable,
    /// A wrapped decode error from `i2pr-proto`.
    #[error("short build protocol error: {0}")]
    Protocol(#[from] CodecError),
}

/// Decoder/encoder helper exposed for the build cryptography layer.
pub const fn plaintext_size(is_reply: bool) -> usize {
    if is_reply {
        SHORT_REPLY_PLAINTEXT_SIZE
    } else {
        SHORT_REQUEST_PLAINTEXT_SIZE
    }
}

/// Helper that classifies a plaintext buffer by length.
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

/// Validate a Mapping as a reply body. Used by callers that build
/// replies from external input.
pub fn validate_reply_body(body: &[u8]) -> Result<(), ShortBuildError> {
    if body.len() > MAX_REPLY_MAPPING_AREA {
        return Err(ShortBuildError::OptionsLength { actual: body.len() });
    }
    if !body.is_empty() {
        Mapping::decode(body, MAX_REPLY_MAPPING_AREA).map_err(ShortBuildError::Protocol)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand_core::SeedableRng;

    use crate::identity::TunnelId;

    fn next_router() -> Hash {
        Hash::from_bytes([0x33_u8; 32])
    }

    fn request_record() -> ShortRequestRecord {
        ShortRequestRecord::try_new(
            TunnelId::new(0x1000).expect("tunnel id"),
            TunnelId::new(0x2000).expect("tunnel id"),
            next_router(),
            HopRole::InboundGateway,
            LayerEncryptionType::Aes,
            Date::from_millis(60_000),
            REQUEST_EXPIRATION_SECONDS,
            0xABCD_1234,
            BuildOptions::empty(),
        )
        .expect("request")
    }

    #[test]
    fn hop_role_flags_round_trip() {
        assert_eq!(
            HopRole::from_flag(HOP_ROLE_PARTICIPANT).expect("flag"),
            HopRole::Participant
        );
        assert_eq!(
            HopRole::from_flag(HOP_ROLE_INBOUND_GATEWAY).expect("flag"),
            HopRole::InboundGateway
        );
        assert_eq!(
            HopRole::from_flag(HOP_ROLE_OUTBOUND_ENDPOINT).expect("flag"),
            HopRole::OutboundEndpoint
        );
        assert!(HopRole::from_flag(0xC0).is_err());
        assert!(HopRole::from_flag(0x10).is_err());
    }

    #[test]
    fn layer_encryption_type_is_aes_only() {
        assert_eq!(
            LayerEncryptionType::from_byte(LayerEncryptionType::Aes.byte()).expect("ok"),
            LayerEncryptionType::Aes
        );
        // The Plan 108 0x05 byte is rejected by the new decoder.
        assert!(matches!(
            LayerEncryptionType::from_byte(0x05),
            Err(ShortBuildError::UnsupportedLayerEncryption { byte: 0x05 })
        ));
    }

    #[test]
    fn response_code_round_trip() {
        assert_eq!(
            ShortResponseCode::from_byte(ShortResponseCode::Accepted.byte()).expect("ok"),
            ShortResponseCode::Accepted
        );
        assert_eq!(
            ShortResponseCode::from_byte(ShortResponseCode::BandwidthRejected.byte()).expect("ok"),
            ShortResponseCode::BandwidthRejected
        );
        // Plan 108's response code 1 is no longer accepted.
        assert!(matches!(
            ShortResponseCode::from_byte(1),
            Err(ShortBuildError::UnknownReplyCode { code: 1 })
        ));
    }

    #[test]
    fn request_minutes_conversion_is_exact() {
        // 60_000 ms = 60 s = 1 minute exactly.
        let record = request_record();
        assert_eq!(record.request_time_minutes().expect("minutes"), 1);
        // Now test the boundaries.
        let floor_60s = ShortRequestRecord::try_new(
            TunnelId::new(0x1000).expect("tunnel id"),
            TunnelId::new(0x2000).expect("tunnel id"),
            next_router(),
            HopRole::InboundGateway,
            LayerEncryptionType::Aes,
            Date::from_millis(59_999),
            REQUEST_EXPIRATION_SECONDS,
            0x1234_5678,
            BuildOptions::empty(),
        )
        .expect("request");
        // 59.999 s rounds down to 0 minutes.
        assert_eq!(floor_60s.request_time_minutes().expect("minutes"), 0);
        let exact_60s = ShortRequestRecord::try_new(
            TunnelId::new(0x1000).expect("tunnel id"),
            TunnelId::new(0x2000).expect("tunnel id"),
            next_router(),
            HopRole::InboundGateway,
            LayerEncryptionType::Aes,
            Date::from_millis(60_000),
            REQUEST_EXPIRATION_SECONDS,
            0x1234_5678,
            BuildOptions::empty(),
        )
        .expect("request");
        assert_eq!(exact_60s.request_time_minutes().expect("minutes"), 1);
    }

    #[test]
    fn request_record_encodes_to_canonical_layout() {
        let record = request_record();
        let bytes = record.encode_deterministic_zero_padded().expect("encode");
        assert_eq!(bytes.len(), SHORT_REQUEST_PLAINTEXT_SIZE);
        // Receive tunnel id at offset 0.
        assert_eq!(&bytes[0..4], &[0x00, 0x00, 0x10, 0x00]);
        // Next tunnel id at offset 4.
        assert_eq!(&bytes[4..8], &[0x00, 0x00, 0x20, 0x00]);
        // Next router hash at offset 8.
        assert_eq!(&bytes[8..40], &[0x33_u8; 32]);
        // Role flag at offset 40 (InboundGateway = 0x80).
        assert_eq!(bytes[40], HOP_ROLE_INBOUND_GATEWAY);
        // Bytes 41..42 are zero.
        assert_eq!(bytes[41], 0);
        assert_eq!(bytes[42], 0);
        // Layer encryption type at offset 43.
        assert_eq!(bytes[43], LayerEncryptionType::Aes.byte());
        // Request time minutes at offset 44.
        assert_eq!(&bytes[44..48], &1_u32.to_be_bytes());
        // Expiration in seconds at offset 48.
        assert_eq!(&bytes[48..52], &REQUEST_EXPIRATION_SECONDS.to_be_bytes());
        // Next message id at offset 52.
        assert_eq!(&bytes[52..56], &0xABCD_1234_u32.to_be_bytes());
        // Empty Mapping at offset 56 encodes to two zero bytes.
        assert_eq!(&bytes[56..58], &[0x00, 0x00]);
        // Remaining bytes up to 154 are zero (background padding).
        assert_eq!(bytes[58..154], [0_u8; 96]);
    }

    #[test]
    fn request_record_rejects_simultaneous_role_flags() {
        // The flag encoder never combines the two high bits, so a
        // round-trip through the canonical byte must reject 0xC0.
        let outcome = HopRole::from_flag(0xC0);
        assert!(matches!(
            outcome,
            Err(ShortBuildError::InvalidRoleFlags { flags: 0xC0 })
        ));
    }

    #[test]
    fn request_record_rejects_zero_message_id() {
        let outcome = ShortRequestRecord::try_new(
            TunnelId::new(0x10).expect("id"),
            TunnelId::new(0x20).expect("id"),
            next_router(),
            HopRole::Participant,
            LayerEncryptionType::Aes,
            Date::from_millis(60_000),
            REQUEST_EXPIRATION_SECONDS,
            0,
            BuildOptions::empty(),
        );
        assert!(matches!(outcome, Err(ShortBuildError::ZeroMessageId)));
    }

    #[test]
    fn request_record_rejects_non_standard_expiration() {
        let outcome = ShortRequestRecord::try_new(
            TunnelId::new(0x10).expect("id"),
            TunnelId::new(0x20).expect("id"),
            next_router(),
            HopRole::Participant,
            LayerEncryptionType::Aes,
            Date::from_millis(60_000),
            42,
            0x1234_5678,
            BuildOptions::empty(),
        );
        assert!(matches!(
            outcome,
            Err(ShortBuildError::ExpirationMismatch { .. })
        ));
    }

    #[test]
    fn reply_record_round_trips_through_canonical_202_bytes() {
        let reply = ShortReplyRecord::new(BuildOptions::empty(), ShortResponseCode::Accepted);
        let bytes = reply.encode_deterministic_zero_padded();
        assert_eq!(bytes.len(), SHORT_REPLY_PLAINTEXT_SIZE);
        let decoded = ShortReplyRecord::decode(bytes.as_ref()).expect("decode");
        assert_eq!(decoded, reply);
    }

    #[test]
    fn request_record_decodes_round_trip_through_canonical_154_bytes() {
        let record = request_record();
        let bytes = record.encode_deterministic_zero_padded().expect("encode");
        let decoded = ShortRequestRecord::decode(bytes.as_ref()).expect("decode");
        assert_eq!(decoded, record);
    }

    #[test]
    fn request_record_decode_rejects_zero_receive_tunnel() {
        let mut bytes = [0_u8; SHORT_REQUEST_PLAINTEXT_SIZE];
        // Bytes 0..4 remain zero (receive tunnel id = 0).
        bytes[8..40].copy_from_slice(&[0x33_u8; 32]); // next router hash
        bytes[40] = HOP_ROLE_PARTICIPANT;
        bytes[43] = LayerEncryptionType::Aes.byte();
        bytes[48..52].copy_from_slice(&REQUEST_EXPIRATION_SECONDS.to_be_bytes());
        bytes[52..56].copy_from_slice(&0x1234_5678_u32.to_be_bytes());
        let outcome = ShortRequestRecord::decode(&bytes);
        assert!(matches!(
            outcome,
            Err(ShortBuildError::ZeroTunnelId {
                field: "receive_tunnel"
            })
        ));
    }

    #[test]
    fn request_record_decode_rejects_zero_next_tunnel() {
        let record = ShortRequestRecord::try_new(
            TunnelId::new(0x1000).expect("id"),
            TunnelId::new(0x2000).expect("id"),
            next_router(),
            HopRole::InboundGateway,
            LayerEncryptionType::Aes,
            Date::from_millis(60_000),
            REQUEST_EXPIRATION_SECONDS,
            0xABCD_1234,
            BuildOptions::empty(),
        )
        .expect("ok");
        let mut bytes_vec = record
            .encode_deterministic_zero_padded()
            .expect("encode")
            .to_vec();
        // Zero out the next tunnel id at offset 4..8.
        for byte in &mut bytes_vec[4..8] {
            *byte = 0;
        }
        let outcome = ShortRequestRecord::decode(&bytes_vec);
        assert!(matches!(
            outcome,
            Err(ShortBuildError::ZeroTunnelId {
                field: "next_tunnel"
            })
        ));
    }

    #[test]
    fn request_record_decode_rejects_invalid_role_byte() {
        let mut bytes = [0_u8; SHORT_REQUEST_PLAINTEXT_SIZE];
        // Receive/next tunnel ids (nonzero).
        bytes[0..4].copy_from_slice(&0x1000_u32.to_be_bytes());
        bytes[4..8].copy_from_slice(&0x2000_u32.to_be_bytes());
        bytes[8..40].copy_from_slice(&[0x33_u8; 32]); // next router hash
        bytes[40] = 0xC0; // invalid role flag combination
        bytes[43] = LayerEncryptionType::Aes.byte();
        bytes[48..52].copy_from_slice(&REQUEST_EXPIRATION_SECONDS.to_be_bytes());
        bytes[52..56].copy_from_slice(&0x1234_5678_u32.to_be_bytes());
        let outcome = ShortRequestRecord::decode(&bytes);
        assert!(matches!(
            outcome,
            Err(ShortBuildError::InvalidRoleFlags { flags: 0xC0 })
        ));
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
    fn reply_record_rejects_unknown_response_byte() {
        let mut bytes = [0_u8; SHORT_REPLY_PLAINTEXT_SIZE];
        bytes[SHORT_REPLY_PLAINTEXT_SIZE - 1] = 0x05;
        let outcome = ShortReplyRecord::decode(&bytes);
        assert!(matches!(
            outcome,
            Err(ShortBuildError::UnknownReplyCode { .. })
        ));
    }

    #[test]
    fn reply_record_carries_bandwidth_rejection_response_byte() {
        let reply =
            ShortReplyRecord::new(BuildOptions::empty(), ShortResponseCode::BandwidthRejected);
        let bytes = reply.encode_deterministic_zero_padded();
        assert_eq!(bytes[SHORT_REPLY_PLAINTEXT_SIZE - 1], 30);
    }

    /// Plan 112 §5.G: the production RNG-padded request encoder
    /// must produce at least one non-zero byte in the post-Mapping
    /// padding region for an empty-mapping request; otherwise the
    /// padding has not been filled from the supplied CSPRNG.
    #[test]
    fn request_padding_with_rng_produces_non_zero_bytes() {
        let request = request_record();
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(0xABCD);
        let bytes = request.encode_with_rng(&mut rng).expect("encode");
        // Find a non-zero byte in the padding region (post-Mapping
        // area, before the fixed-size plaintext ends).
        let has_nonzero = bytes.iter().any(|byte| *byte != 0);
        assert!(
            has_nonzero,
            "post-Mapping padding must contain at least one non-zero byte from the CSPRNG"
        );
    }

    /// Plan 112 §5.G: the test-only zero-padded encoder must
    /// produce an all-zero post-Mapping padding region; this is
    /// the fixture-style invariant the conformance suite relies
    /// on.
    #[test]
    fn request_deterministic_zero_padding_leaves_padding_all_zero() {
        let request = request_record();
        let bytes = request.encode_deterministic_zero_padded().expect("encode");
        // The mapping area ends at REQUEST_FIXED_PREFIX_LEN;
        // padding is REQUEST_FIXED_PREFIX_LEN..SHORT_REQUEST_PLAINTEXT_SIZE.
        for index in REQUEST_FIXED_PREFIX_LEN..SHORT_REQUEST_PLAINTEXT_SIZE {
            assert_eq!(
                bytes[index], 0,
                "padding byte at offset {index} must be zero in deterministic mode"
            );
        }
    }

    /// Plan 112 §5.G: the production RNG-padded reply encoder
    /// must produce at least one non-zero byte in the post-Mapping
    /// padding region.
    #[test]
    fn reply_padding_with_rng_produces_non_zero_bytes() {
        let reply = ShortReplyRecord::new(BuildOptions::empty(), ShortResponseCode::Accepted);
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(0xBEEF);
        let bytes = reply.encode_with_rng(&mut rng).expect("encode");
        // The response byte lives at SHORT_REPLY_PLAINTEXT_SIZE-1;
        // the post-Mapping padding region ends one byte earlier.
        let last_padding = SHORT_REPLY_PLAINTEXT_SIZE - 2;
        let has_nonzero = bytes[..=last_padding].iter().any(|byte| *byte != 0);
        assert!(
            has_nonzero,
            "post-Mapping reply padding must contain at least one non-zero byte from the CSPRNG"
        );
        // The response byte at the last position must be preserved.
        assert_eq!(bytes[SHORT_REPLY_PLAINTEXT_SIZE - 1], 0);
    }
}
