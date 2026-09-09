//! I2CP message types and structural body codecs.
//!
//! Every assigned I2CP message type has an exact numeric ID, a fixed
//! direction, and an M9 disposition (see [`crate::i2cp`] for the
//! profile table). Codecs
//! exist for the implemented M9 surface only; deprecated, unsupported,
//! and unknown types are classified by [`decode_typed`] without
//! parsing their bodies, so framing never desynchronizes.
//!
//! Body codecs are structural: exact type IDs, named minima/maxima,
//! strict trailing-byte rejection, and typed malformed errors. Opaque
//! regions that later passes validate (SessionConfig signatures,
//! LeaseSet2 semantics, payload routing) still carry absolute length
//! ceilings here. No codec opens sockets, verifies signatures, or
//! touches the network.

use std::str;

use i2pr_proto::{
    CodecError, CryptoKeyType, Destination, Hash, LeaseSet2, Mapping, SignatureValue,
};
use zeroize::Zeroizing;

use super::error::I2cpError;
use super::ids::{ClientNonce, HostRequestId, MessageId, SessionId};
use super::mapping::{encode_mapping, split_mapping_lenient, split_mapping_strict};
use super::payload::{Payload, split_payload};

/// Maximum I2CP destination encoding in bytes.
///
/// Modern Ed25519/X25519 destinations encode to 391 bytes; legacy key
/// types stay far below this ceiling.
pub const MAX_I2CP_DESTINATION_BYTES: usize = 2048;

/// Fixed I2P key-area bytes preceding every destination certificate.
const DESTINATION_KEY_AREA_BYTES: usize = 384;

/// Certificate header bytes (one type byte plus two length bytes).
const CERT_HEADER_BYTES: usize = 3;

/// Maximum UTF-8 byte length of an I2CP string field.
pub const MAX_I2CP_STRING_BYTES: usize = u8::MAX as usize;

/// Maximum leases one `RequestVariableLeaseSet` may carry.
///
/// Matches the Standard LeaseSet2 lease ceiling: requesting more
/// tunnels than a LeaseSet2 can publish is meaningless.
pub const MAX_VARIABLE_LEASES: usize = 16;

/// Maximum aggregate I2CP private-key bytes in one `CreateLeaseSet2`.
///
/// Mirrors the LeaseSet2 aggregate encryption-key ceiling.
pub const MAX_I2CP_PRIVATE_KEY_BYTES: usize = 8 * 1024;

/// Maximum private keys in one `CreateLeaseSet2` (mirrors LeaseSet2).
pub const MAX_I2CP_PRIVATE_KEYS: usize = 8;

/// Exact `BandwidthLimits` body length: sixteen four-byte integers.
///
/// Seven named limits followed by nine reserved integers, exactly as
/// the specification lists them.
pub const BANDWIDTH_LIMITS_BODY_LEN: usize = 16 * 4;

/// Allowed `SessionStatus` codes are 0–4; allowed `MessageStatus`
/// codes are 0–23 with higher values reserved.
pub const MAX_SESSION_STATUS_CODE: u8 = 4;
/// Highest assigned `MessageStatus` code (higher values are reserved failures).
pub const MAX_MESSAGE_STATUS_CODE: u8 = 23;

/// SessionConfig creation-time skew window in milliseconds (±30 s).
///
/// Recorded here; enforcement belongs to the Plan 165 session state
/// machine, not to this structural layer.
pub const SESSION_CONFIG_MAX_SKEW_MS: u64 = 30_000;

/// Lease-set type code for the deprecated classic LeaseSet.
pub const LEASE_SET_TYPE_CLASSIC: u8 = 1;
/// Lease-set type code for Standard LeaseSet2 (the M9 profile).
pub const LEASE_SET_TYPE_STANDARD_V2: u8 = 3;
/// Lease-set type code for EncryptedLeaseSet (unsupported in M9).
pub const LEASE_SET_TYPE_ENCRYPTED: u8 = 5;
/// Lease-set type code for MetaLeaseSet (unsupported in M9).
pub const LEASE_SET_TYPE_META: u8 = 7;

/// `SendMessageExpires` reserved flag bits 15–11 (must be zero).
pub const SEND_FLAGS_RESERVED_MASK: u16 = 0xf800;
/// `SendMessageExpires` reliability-override bits 10–9 (unimplemented).
pub const SEND_FLAGS_RELIABILITY_MASK: u16 = 0x0600;
/// `SendMessageExpires` do-not-bundle flag, bit 8.
pub const SEND_FLAGS_NO_BUNDLE: u16 = 0x0100;
/// `SendMessageExpires` low-tag-threshold bits 7–4 (ElGamal only).
pub const SEND_FLAGS_TAG_THRESHOLD_MASK: u16 = 0x00f0;
/// `SendMessageExpires` tags-to-send bits 3–0 (ElGamal only).
pub const SEND_FLAGS_TAGS_TO_SEND_MASK: u16 = 0x000f;

fn malformed(message: &'static str, source: CodecError) -> I2cpError {
    I2cpError::malformed(message, source)
}

fn truncated(message: &'static str, offset: usize, needed: usize, remaining: usize) -> I2cpError {
    malformed(
        message,
        CodecError::Truncated {
            offset,
            needed,
            remaining,
        },
    )
}

/// Wire direction of an I2CP message.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Direction {
    /// Client to router.
    ClientToRouter,
    /// Router to client.
    RouterToClient,
    /// Either direction (Disconnect).
    Bidirectional,
}

/// M9 disposition of an assigned I2CP message type.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MessageDisposition {
    /// Structural codec implemented in this pass.
    ImplementedM9,
    /// Legacy type the M9 profile rejects without parsing.
    LegacyDeprecated,
    /// Assigned type outside the M9 profile (blinded destinations).
    ExplicitlyUnsupported,
}

/// Every assigned I2CP message type with its exact numeric ID.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum MessageType {
    /// Client to router: open a destination session.
    CreateSession = 1,
    /// Client to router: adjust session options.
    ReconfigureSession = 2,
    /// Client to router: destroy one session.
    DestroySession = 3,
    /// Deprecated classic LeaseSet submission.
    CreateLeaseSet = 4,
    /// Client to router: submit an outbound application message.
    SendMessage = 5,
    /// Deprecated non-fast-receive acknowledgement.
    ReceiveMessageBegin = 6,
    /// Deprecated non-fast-receive acknowledgement.
    ReceiveMessageEnd = 7,
    /// Client to router: request bandwidth limits.
    GetBandwidthLimits = 8,
    /// Router to client: session lifecycle outcome.
    SessionStatus = 20,
    /// Deprecated fixed-lease request.
    RequestLeaseSet = 21,
    /// Router to client: outbound/inbound message outcome.
    MessageStatus = 22,
    /// Router to client: bandwidth limits.
    BandwidthLimits = 23,
    /// Deprecated abuse report.
    ReportAbuse = 29,
    /// Either direction: connection teardown with a reason.
    Disconnect = 30,
    /// Router to client: inbound application payload.
    MessagePayload = 31,
    /// Client to router: version handshake.
    GetDate = 32,
    /// Router to client: clock and version handshake.
    SetDate = 33,
    /// Client to router: destination hash lookup.
    DestLookup = 34,
    /// Router to client: destination lookup reply.
    DestReply = 35,
    /// Client to router: expiring outbound application message.
    SendMessageExpires = 36,
    /// Router to client: request a variable-lease set.
    RequestVariableLeaseSet = 37,
    /// Client to router: hostname/hash lookup with request ID.
    HostLookup = 38,
    /// Router to client: hostname/hash lookup reply.
    HostReply = 39,
    /// Client to router: publish a LeaseSet2 family member.
    CreateLeaseSet2 = 41,
    /// Blinded-destination announcement (unsupported in M9).
    BlindingInfo = 42,
}

impl MessageType {
    /// Classifies a raw type byte; `None` means unassigned.
    ///
    /// Type 40 (the abandoned preliminary CreateLeaseSet2) is
    /// deliberately unassigned and classifies as unknown.
    pub const fn from_u8(raw: u8) -> Option<Self> {
        match raw {
            1 => Some(Self::CreateSession),
            2 => Some(Self::ReconfigureSession),
            3 => Some(Self::DestroySession),
            4 => Some(Self::CreateLeaseSet),
            5 => Some(Self::SendMessage),
            6 => Some(Self::ReceiveMessageBegin),
            7 => Some(Self::ReceiveMessageEnd),
            8 => Some(Self::GetBandwidthLimits),
            20 => Some(Self::SessionStatus),
            21 => Some(Self::RequestLeaseSet),
            22 => Some(Self::MessageStatus),
            23 => Some(Self::BandwidthLimits),
            29 => Some(Self::ReportAbuse),
            30 => Some(Self::Disconnect),
            31 => Some(Self::MessagePayload),
            32 => Some(Self::GetDate),
            33 => Some(Self::SetDate),
            34 => Some(Self::DestLookup),
            35 => Some(Self::DestReply),
            36 => Some(Self::SendMessageExpires),
            37 => Some(Self::RequestVariableLeaseSet),
            38 => Some(Self::HostLookup),
            39 => Some(Self::HostReply),
            41 => Some(Self::CreateLeaseSet2),
            42 => Some(Self::BlindingInfo),
            _ => None,
        }
    }

    /// Returns the exact numeric type ID.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Returns the wire direction.
    pub const fn direction(self) -> Direction {
        match self {
            Self::CreateSession
            | Self::ReconfigureSession
            | Self::DestroySession
            | Self::CreateLeaseSet
            | Self::SendMessage
            | Self::ReceiveMessageBegin
            | Self::ReceiveMessageEnd
            | Self::GetBandwidthLimits
            | Self::GetDate
            | Self::DestLookup
            | Self::SendMessageExpires
            | Self::HostLookup
            | Self::CreateLeaseSet2
            | Self::BlindingInfo => Direction::ClientToRouter,
            Self::SessionStatus
            | Self::RequestLeaseSet
            | Self::MessageStatus
            | Self::BandwidthLimits
            | Self::MessagePayload
            | Self::SetDate
            | Self::DestReply
            | Self::RequestVariableLeaseSet
            | Self::HostReply => Direction::RouterToClient,
            Self::ReportAbuse | Self::Disconnect => Direction::Bidirectional,
        }
    }

    /// Returns the M9 disposition.
    pub const fn disposition(self) -> MessageDisposition {
        match self {
            Self::CreateLeaseSet
            | Self::ReceiveMessageBegin
            | Self::ReceiveMessageEnd
            | Self::RequestLeaseSet
            | Self::ReportAbuse => MessageDisposition::LegacyDeprecated,
            Self::BlindingInfo => MessageDisposition::ExplicitlyUnsupported,
            _ => MessageDisposition::ImplementedM9,
        }
    }
}

/// Splits one one-byte-length-prefixed UTF-8 string off `input`.
fn split_string<'a>(
    message: &'static str,
    input: &'a [u8],
) -> Result<(String, &'a [u8]), I2cpError> {
    if input.is_empty() {
        return Err(truncated(message, 0, 1, 0));
    }
    let length = usize::from(input[0]);
    if input.len() - 1 < length {
        return Err(truncated(message, 1, length, input.len() - 1));
    }
    let text = str::from_utf8(&input[1..1 + length])
        .map_err(|_| malformed(message, CodecError::InvalidUtf8 { offset: 1 }))?;
    Ok((text.to_owned(), &input[1 + length..]))
}

/// Encodes one one-byte-length-prefixed UTF-8 string.
fn encode_string(message: &'static str, text: &str) -> Result<Vec<u8>, I2cpError> {
    if text.len() > MAX_I2CP_STRING_BYTES {
        return Err(malformed(
            message,
            CodecError::LengthExceeded {
                offset: 0,
                declared: text.len(),
                maximum: MAX_I2CP_STRING_BYTES,
                context: "i2cp string",
            },
        ));
    }
    let mut out = Vec::with_capacity(1 + text.len());
    out.push(text.len() as u8);
    out.extend_from_slice(text.as_bytes());
    Ok(out)
}

/// Splits one Destination off `input` using the fixed 384-byte key
/// area plus the certificate length header.
///
/// The length comes from the wire bytes themselves, so no canonical
/// re-encoding assumption is needed to find the boundary.
fn split_destination<'a>(
    message: &'static str,
    input: &'a [u8],
) -> Result<(Destination, &'a [u8]), I2cpError> {
    const PREFIX: usize = DESTINATION_KEY_AREA_BYTES + CERT_HEADER_BYTES;
    if input.len() < PREFIX {
        return Err(truncated(message, 0, PREFIX, input.len()));
    }
    let certificate_len = usize::from(u16::from_be_bytes([input[385], input[386]]));
    let total = PREFIX.checked_add(certificate_len).ok_or(malformed(
        message,
        CodecError::ArithmeticOverflow {
            offset: PREFIX,
            context: "destination total length",
        },
    ))?;
    if total > MAX_I2CP_DESTINATION_BYTES {
        return Err(malformed(
            message,
            CodecError::LengthExceeded {
                offset: PREFIX,
                declared: total,
                maximum: MAX_I2CP_DESTINATION_BYTES,
                context: "destination",
            },
        ));
    }
    if input.len() < total {
        return Err(truncated(message, 0, total, input.len()));
    }
    let destination =
        Destination::decode(&input[..total], total).map_err(|source| malformed(message, source))?;
    Ok((destination, &input[total..]))
}

/// Encodes one Destination under the I2CP destination ceiling.
fn encode_destination(
    message: &'static str,
    destination: &Destination,
) -> Result<Vec<u8>, I2cpError> {
    destination
        .encode_to_vec(MAX_I2CP_DESTINATION_BYTES)
        .map_err(|source| malformed(message, source))
}

/// Requires `input` to be fully consumed after a body decode.
fn require_empty(message: &'static str, input: &[u8], offset: usize) -> Result<(), I2cpError> {
    if input.is_empty() {
        Ok(())
    } else {
        Err(malformed(
            message,
            CodecError::TrailingBytes {
                offset,
                remaining: input.len(),
            },
        ))
    }
}

fn read_u16<'a>(message: &'static str, input: &'a [u8]) -> Result<(u16, &'a [u8]), I2cpError> {
    if input.len() < 2 {
        return Err(truncated(message, 0, 2, input.len()));
    }
    Ok((u16::from_be_bytes([input[0], input[1]]), &input[2..]))
}

fn read_u32<'a>(message: &'static str, input: &'a [u8]) -> Result<(u32, &'a [u8]), I2cpError> {
    if input.len() < 4 {
        return Err(truncated(message, 0, 4, input.len()));
    }
    Ok((
        u32::from_be_bytes([input[0], input[1], input[2], input[3]]),
        &input[4..],
    ))
}

fn read_u64<'a>(message: &'static str, input: &'a [u8]) -> Result<(u64, &'a [u8]), I2cpError> {
    if input.len() < 8 {
        return Err(truncated(message, 0, 8, input.len()));
    }
    Ok((
        u64::from_be_bytes([
            input[0], input[1], input[2], input[3], input[4], input[5], input[6], input[7],
        ]),
        &input[8..],
    ))
}

/// A `GetDate` body: API version string plus optional authentication mapping.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GetDate {
    /// Client I2CP API version string (for example `"0.9.67"`).
    pub version: String,
    /// Optional `i2cp.username`/`i2cp.password` mapping (M9: absent).
    pub auth: Option<Mapping>,
}

impl GetDate {
    const NAME: &'static str = "get-date";

    /// Decodes a strict body.
    pub fn decode(body: &[u8]) -> Result<Self, I2cpError> {
        let (version, rest) = split_string(Self::NAME, body)?;
        if rest.is_empty() {
            return Ok(Self {
                version,
                auth: None,
            });
        }
        let offset = body.len() - rest.len();
        let (mapping, consumed) = split_mapping_lenient(rest).map_err(|source| match source {
            I2cpError::Malformed { source, .. } => malformed(Self::NAME, source),
            other => other,
        })?;
        require_empty(Self::NAME, &rest[consumed..], offset + consumed)?;
        Ok(Self {
            version,
            auth: Some(mapping),
        })
    }

    /// Encodes the body.
    pub fn encode(&self) -> Result<Vec<u8>, I2cpError> {
        let mut out = encode_string(Self::NAME, &self.version)?;
        if let Some(auth) = &self.auth {
            out.extend(encode_mapping(auth)?);
        }
        Ok(out)
    }
}

/// A `SetDate` body: router clock plus API version string.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SetDate {
    /// Router time in milliseconds since the Unix epoch.
    pub date_ms: u64,
    /// Router I2CP API version string.
    pub version: String,
}

impl SetDate {
    const NAME: &'static str = "set-date";

    /// Decodes a strict body.
    pub fn decode(body: &[u8]) -> Result<Self, I2cpError> {
        let (date_ms, rest) = read_u64(Self::NAME, body)?;
        let (version, rest) = split_string(Self::NAME, rest)?;
        require_empty(Self::NAME, rest, body.len() - rest.len())?;
        Ok(Self { date_ms, version })
    }

    /// Encodes the body.
    pub fn encode(&self) -> Result<Vec<u8>, I2cpError> {
        let mut out = Vec::with_capacity(8 + 1 + self.version.len());
        out.extend_from_slice(&self.date_ms.to_be_bytes());
        out.extend(encode_string(Self::NAME, &self.version)?);
        Ok(out)
    }
}

/// A signed session configuration (CreateSession/ReconfigureSession).
///
/// The `signed_region` retains the exact received
/// Destination || Mapping || Date bytes so Plan 165/166 verify the
/// signature over wire bytes rather than a reserialization.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionConfig {
    destination: Destination,
    options: Mapping,
    creation_ms: u64,
    signature: SignatureValue,
    signed_region: Vec<u8>,
}

impl SessionConfig {
    const NAME: &'static str = "session-config";

    /// Decodes a strict session configuration.
    pub fn decode(input: &[u8]) -> Result<Self, I2cpError> {
        let (destination, rest) = split_destination(Self::NAME, input)?;
        let (options, mapping_len) = split_mapping_strict(rest).map_err(|source| match source {
            I2cpError::Malformed { source, .. } => malformed(Self::NAME, source),
            other => other,
        })?;
        let rest = &rest[mapping_len..];
        if rest.len() < 8 {
            return Err(truncated(
                Self::NAME,
                input.len() - rest.len(),
                8,
                rest.len(),
            ));
        }
        let (creation_ms, rest) = read_u64(Self::NAME, rest)?;
        let signature_len = destination
            .signing_key()
            .key_type()
            .signature_len()
            .ok_or_else(|| {
                malformed(
                    Self::NAME,
                    CodecError::Unsupported {
                        offset: 0,
                        context: "session config signature type",
                        value: u64::from(destination.signing_key().key_type().code()),
                    },
                )
            })?;
        if rest.len() != signature_len {
            return Err(if rest.len() < signature_len {
                truncated(
                    Self::NAME,
                    input.len() - rest.len(),
                    signature_len,
                    rest.len(),
                )
            } else {
                malformed(
                    Self::NAME,
                    CodecError::TrailingBytes {
                        offset: input.len() - rest.len() + signature_len,
                        remaining: rest.len() - signature_len,
                    },
                )
            });
        }
        let signature = SignatureValue::new(destination.signing_key().key_type(), rest.to_vec())
            .map_err(|source| malformed(Self::NAME, source))?;
        let signed_len = input.len() - signature_len;
        Ok(Self {
            destination,
            options,
            creation_ms,
            signature,
            signed_region: input[..signed_len].to_vec(),
        })
    }

    /// Encodes the configuration canonically.
    pub fn encode(&self) -> Result<Vec<u8>, I2cpError> {
        let mut out = encode_destination(Self::NAME, &self.destination)?;
        out.extend(encode_mapping(&self.options)?);
        out.extend_from_slice(&self.creation_ms.to_be_bytes());
        out.extend_from_slice(self.signature.as_bytes());
        Ok(out)
    }

    /// Returns the claimed destination.
    pub const fn destination(&self) -> &Destination {
        &self.destination
    }

    /// Returns the session options.
    pub const fn options(&self) -> &Mapping {
        &self.options
    }

    /// Returns the creation time in milliseconds since the Unix epoch.
    pub const fn creation_ms(&self) -> u64 {
        self.creation_ms
    }

    /// Returns the session-config signature.
    pub const fn signature(&self) -> &SignatureValue {
        &self.signature
    }

    /// Returns the exact received bytes the signature covers.
    pub fn signed_region(&self) -> &[u8] {
        &self.signed_region
    }
}

/// A `CreateSession` body: exactly one session configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateSession {
    /// The signed session configuration.
    pub config: SessionConfig,
}

impl CreateSession {
    const NAME: &'static str = "create-session";

    /// Decodes a strict body.
    pub fn decode(body: &[u8]) -> Result<Self, I2cpError> {
        Ok(Self {
            config: SessionConfig::decode(body).map_err(|source| match source {
                I2cpError::Malformed { source, .. } => malformed(Self::NAME, source),
                other => other,
            })?,
        })
    }

    /// Encodes the body.
    pub fn encode(&self) -> Result<Vec<u8>, I2cpError> {
        self.config.encode()
    }
}

/// A `ReconfigureSession` body: session ID plus a new configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReconfigureSession {
    /// Target session.
    pub session: SessionId,
    /// The replacement signed configuration.
    pub config: SessionConfig,
}

impl ReconfigureSession {
    const NAME: &'static str = "reconfigure-session";

    /// Decodes a strict body.
    pub fn decode(body: &[u8]) -> Result<Self, I2cpError> {
        let (raw, rest) = read_u16(Self::NAME, body)?;
        let config = SessionConfig::decode(rest).map_err(|source| match source {
            I2cpError::Malformed { source, .. } => malformed(Self::NAME, source),
            other => other,
        })?;
        Ok(Self {
            session: SessionId::new(raw),
            config,
        })
    }

    /// Encodes the body.
    pub fn encode(&self) -> Result<Vec<u8>, I2cpError> {
        let mut out = Vec::with_capacity(2);
        out.extend_from_slice(&self.session.get().to_be_bytes());
        out.extend(self.config.encode()?);
        Ok(out)
    }
}

/// A `DestroySession` body: exactly one session ID.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DestroySession {
    /// Session to destroy.
    pub session: SessionId,
}

impl DestroySession {
    const NAME: &'static str = "destroy-session";

    /// Decodes a strict body.
    pub fn decode(body: &[u8]) -> Result<Self, I2cpError> {
        let (raw, rest) = read_u16(Self::NAME, body)?;
        require_empty(Self::NAME, rest, 2)?;
        Ok(Self {
            session: SessionId::new(raw),
        })
    }

    /// Encodes the body.
    pub fn encode(&self) -> Result<Vec<u8>, I2cpError> {
        Ok(self.session.get().to_be_bytes().to_vec())
    }
}

/// `SessionStatus` outcome codes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum SessionStatusCode {
    /// Session terminated.
    Destroyed = 0,
    /// New session is active.
    Created = 1,
    /// Session was reconfigured.
    Updated = 2,
    /// Configuration invalid; the session ID must be ignored.
    Invalid = 3,
    /// Router refused the session; the session ID must be ignored.
    Refused = 4,
}

impl SessionStatusCode {
    /// Decodes an outcome code; anything above 4 is malformed.
    pub const fn from_u8(raw: u8) -> Option<Self> {
        match raw {
            0 => Some(Self::Destroyed),
            1 => Some(Self::Created),
            2 => Some(Self::Updated),
            3 => Some(Self::Invalid),
            4 => Some(Self::Refused),
            _ => None,
        }
    }
}

/// A `SessionStatus` body: session ID plus outcome code.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SessionStatus {
    /// Session the status refers to (ignored for Invalid/Refused).
    pub session: SessionId,
    /// Outcome code.
    pub status: SessionStatusCode,
}

impl SessionStatus {
    const NAME: &'static str = "session-status";

    /// Decodes a strict body.
    pub fn decode(body: &[u8]) -> Result<Self, I2cpError> {
        let (raw_session, rest) = read_u16(Self::NAME, body)?;
        if rest.len() != 1 {
            return Err(if rest.is_empty() {
                truncated(Self::NAME, 2, 1, 0)
            } else {
                malformed(
                    Self::NAME,
                    CodecError::TrailingBytes {
                        offset: 3,
                        remaining: rest.len() - 1,
                    },
                )
            });
        }
        let status = SessionStatusCode::from_u8(rest[0]).ok_or_else(|| {
            malformed(
                Self::NAME,
                CodecError::InvalidFieldValue {
                    offset: 2,
                    context: "session status code",
                },
            )
        })?;
        Ok(Self {
            session: SessionId::new(raw_session),
            status,
        })
    }

    /// Encodes the body.
    pub fn encode(&self) -> Result<Vec<u8>, I2cpError> {
        Ok(vec![
            (self.session.get() >> 8) as u8,
            self.session.get() as u8,
            self.status as u8,
        ])
    }
}

/// One requested tunnel lease: gateway hash, tunnel ID, plus end date.
///
/// The wire form is the 44-byte I2CP `Lease` layout (32-byte gateway
/// hash, 4-byte big-endian tunnel ID, 8-byte big-endian millisecond
/// end date) so exact-pinned Java I2P (`Lease.readBytes`) and go-i2cp
/// (`NewLeaseFromStream`) parse our `RequestVariableLeaseSet` without
/// modification. Plan 172 §5 requires this compatibility for the
/// counted `I2PSession.connect()` lifecycle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RequestedLease {
    /// Gateway router hash.
    pub gateway: Hash,
    /// Tunnel ID at that gateway.
    pub tunnel_id: u32,
    /// Lease end date in milliseconds since the Unix epoch.
    pub end_date_ms: u64,
}

/// A `RequestVariableLeaseSet` body: session plus requested leases.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RequestVariableLeaseSet {
    /// Owning session.
    pub session: SessionId,
    /// Requested leases (at most [`MAX_VARIABLE_LEASES`]).
    pub leases: Vec<RequestedLease>,
}

impl RequestVariableLeaseSet {
    const NAME: &'static str = "request-variable-lease-set";

    /// Decodes a strict body.
    pub fn decode(body: &[u8]) -> Result<Self, I2cpError> {
        let (raw_session, rest) = read_u16(Self::NAME, body)?;
        if rest.is_empty() {
            return Err(truncated(Self::NAME, 2, 1, 0));
        }
        let count = usize::from(rest[0]);
        if count > MAX_VARIABLE_LEASES {
            return Err(malformed(
                Self::NAME,
                CodecError::LengthExceeded {
                    offset: 2,
                    declared: count,
                    maximum: MAX_VARIABLE_LEASES,
                    context: "requested lease count",
                },
            ));
        }
        let mut leases = Vec::with_capacity(count);
        let mut rest = &rest[1..];
        for _ in 0..count {
            if rest.len() < 44 {
                return Err(truncated(
                    Self::NAME,
                    body.len() - rest.len(),
                    44,
                    rest.len(),
                ));
            }
            let mut gateway = [0u8; 32];
            gateway.copy_from_slice(&rest[..32]);
            let tunnel_id = u32::from_be_bytes([rest[32], rest[33], rest[34], rest[35]]);
            let end_date_ms = u64::from_be_bytes([
                rest[36], rest[37], rest[38], rest[39], rest[40], rest[41], rest[42], rest[43],
            ]);
            leases.push(RequestedLease {
                gateway: Hash::from_bytes(gateway),
                tunnel_id,
                end_date_ms,
            });
            rest = &rest[44..];
        }
        require_empty(Self::NAME, rest, body.len() - rest.len())?;
        Ok(Self {
            session: SessionId::new(raw_session),
            leases,
        })
    }

    /// Encodes the body.
    pub fn encode(&self) -> Result<Vec<u8>, I2cpError> {
        if self.leases.len() > MAX_VARIABLE_LEASES {
            return Err(malformed(
                Self::NAME,
                CodecError::LengthExceeded {
                    offset: 0,
                    declared: self.leases.len(),
                    maximum: MAX_VARIABLE_LEASES,
                    context: "requested lease count",
                },
            ));
        }
        let mut out = Vec::with_capacity(3 + 44 * self.leases.len());
        out.extend_from_slice(&self.session.get().to_be_bytes());
        out.push(self.leases.len() as u8);
        for lease in &self.leases {
            out.extend_from_slice(lease.gateway.as_bytes());
            out.extend_from_slice(&lease.tunnel_id.to_be_bytes());
            out.extend_from_slice(&lease.end_date_ms.to_be_bytes());
        }
        Ok(out)
    }
}

/// One client-supplied LeaseSet2 decryption private key.
///
/// Secret-bearing: non-`Clone`, redacted `Debug`, zeroized on drop.
pub struct SessionDecryptionKey {
    key_type: CryptoKeyType,
    bytes: Zeroizing<Vec<u8>>,
}

impl SessionDecryptionKey {
    /// Wraps validated key material.
    pub fn new(key_type: CryptoKeyType, bytes: Vec<u8>) -> Result<Self, I2cpError> {
        if bytes.is_empty() || bytes.len() > MAX_I2CP_PRIVATE_KEY_BYTES {
            return Err(malformed(
                "create-lease-set2",
                CodecError::LengthExceeded {
                    offset: 0,
                    declared: bytes.len(),
                    maximum: MAX_I2CP_PRIVATE_KEY_BYTES,
                    context: "decryption private key",
                },
            ));
        }
        Ok(Self {
            key_type,
            bytes: Zeroizing::new(bytes),
        })
    }

    /// Returns the encryption key type.
    pub const fn key_type(&self) -> CryptoKeyType {
        self.key_type
    }

    /// Returns the private key bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
}

impl std::fmt::Debug for SessionDecryptionKey {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SessionDecryptionKey")
            .field("key_type", &self.key_type)
            .field("bytes", &"<redacted>")
            .finish()
    }
}

impl PartialEq for SessionDecryptionKey {
    fn eq(&self, other: &Self) -> bool {
        self.key_type == other.key_type && self.bytes.as_slice() == other.bytes.as_slice()
    }
}

impl Eq for SessionDecryptionKey {}

/// A `CreateLeaseSet2` body: session, Standard LeaseSet2, and the
/// matching decryption private keys in LeaseSet2 key order.
pub struct CreateLeaseSet2 {
    /// Owning session.
    pub session: SessionId,
    /// The signed Standard LeaseSet2 (semantic validation is Plan 166).
    pub lease_set: LeaseSet2,
    /// One private key per LeaseSet2 encryption key, in order.
    pub private_keys: Vec<SessionDecryptionKey>,
}

impl PartialEq for CreateLeaseSet2 {
    fn eq(&self, other: &Self) -> bool {
        self.session == other.session
            && self.lease_set == other.lease_set
            && self.private_keys == other.private_keys
    }
}

impl Eq for CreateLeaseSet2 {}

impl std::fmt::Debug for CreateLeaseSet2 {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CreateLeaseSet2")
            .field("session", &self.session)
            .field("lease_set", &self.lease_set)
            .field("private_keys", &"<redacted>")
            .finish()
    }
}

impl CreateLeaseSet2 {
    const NAME: &'static str = "create-lease-set2";

    /// Decodes a strict body.
    ///
    /// Only Standard LeaseSet2 (`type 3`) is structurally decoded;
    /// classic (`1`), encrypted (`5`), and meta (`7`) members fail
    /// with a typed unsupported-lease-set error, and any other type
    /// code fails the same way for forward compatibility.
    pub fn decode(body: &[u8]) -> Result<Self, I2cpError> {
        let (raw_session, rest) = read_u16(Self::NAME, body)?;
        if rest.is_empty() {
            return Err(truncated(Self::NAME, 2, 1, 0));
        }
        let lease_type = rest[0];
        if lease_type != LEASE_SET_TYPE_STANDARD_V2 {
            return Err(malformed(
                Self::NAME,
                CodecError::Unsupported {
                    offset: 2,
                    context: "lease-set type",
                    value: u64::from(lease_type),
                },
            ));
        }
        let rest = &rest[1..];
        // The canonical LeaseSet2 decoder is exact-only, so decode
        // against the whole remainder: success is impossible here
        // (trailing key bytes must remain), and the reported trailing
        // offset is exactly the LeaseSet2 boundary.
        let lease_len = match LeaseSet2::decode(rest, rest.len().max(1)) {
            Ok(_) => {
                return Err(truncated(Self::NAME, body.len() - rest.len(), 1, 0));
            }
            Err(CodecError::TrailingBytes { offset, .. }) => offset,
            Err(source) => return Err(malformed(Self::NAME, source)),
        };
        let lease_set = LeaseSet2::decode(&rest[..lease_len], lease_len.max(1))
            .map_err(|source| malformed(Self::NAME, source))?;
        let rest = &rest[lease_len..];
        if rest.is_empty() {
            return Err(truncated(Self::NAME, body.len(), 1, 0));
        }
        let key_count = usize::from(rest[0]);
        if key_count != lease_set.encryption_keys().len() {
            return Err(malformed(
                Self::NAME,
                CodecError::InvalidFieldValue {
                    offset: body.len() - rest.len(),
                    context: "decryption key count",
                },
            ));
        }
        if key_count > MAX_I2CP_PRIVATE_KEYS {
            return Err(malformed(
                Self::NAME,
                CodecError::LengthExceeded {
                    offset: body.len() - rest.len(),
                    declared: key_count,
                    maximum: MAX_I2CP_PRIVATE_KEYS,
                    context: "decryption key count",
                },
            ));
        }
        let mut private_keys = Vec::with_capacity(key_count);
        let mut aggregate = 0usize;
        let mut rest = &rest[1..];
        for expected in lease_set.encryption_keys() {
            if rest.len() < 4 {
                return Err(truncated(
                    Self::NAME,
                    body.len() - rest.len(),
                    4,
                    rest.len(),
                ));
            }
            let key_type = CryptoKeyType::from_code(u16::from_be_bytes([rest[0], rest[1]]));
            let key_len = usize::from(u16::from_be_bytes([rest[2], rest[3]]));
            if key_type != expected.key_type() {
                return Err(malformed(
                    Self::NAME,
                    CodecError::InvalidFieldValue {
                        offset: body.len() - rest.len(),
                        context: "decryption key order",
                    },
                ));
            }
            if key_len == 0 || key_len > MAX_I2CP_PRIVATE_KEY_BYTES {
                return Err(malformed(
                    Self::NAME,
                    CodecError::LengthExceeded {
                        offset: body.len() - rest.len() + 2,
                        declared: key_len,
                        maximum: MAX_I2CP_PRIVATE_KEY_BYTES,
                        context: "decryption private key",
                    },
                ));
            }
            aggregate = aggregate.checked_add(key_len).ok_or(malformed(
                Self::NAME,
                CodecError::ArithmeticOverflow {
                    offset: 0,
                    context: "decryption key aggregate",
                },
            ))?;
            if aggregate > MAX_I2CP_PRIVATE_KEY_BYTES {
                return Err(malformed(
                    Self::NAME,
                    CodecError::LengthExceeded {
                        offset: 0,
                        declared: aggregate,
                        maximum: MAX_I2CP_PRIVATE_KEY_BYTES,
                        context: "decryption key aggregate",
                    },
                ));
            }
            rest = &rest[4..];
            if rest.len() < key_len {
                return Err(truncated(
                    Self::NAME,
                    body.len() - rest.len(),
                    key_len,
                    rest.len(),
                ));
            }
            private_keys.push(
                SessionDecryptionKey::new(key_type, rest[..key_len].to_vec()).map_err(|_| {
                    malformed(
                        Self::NAME,
                        CodecError::InvalidFieldValue {
                            offset: body.len() - rest.len(),
                            context: "decryption private key",
                        },
                    )
                })?,
            );
            rest = &rest[key_len..];
        }
        require_empty(Self::NAME, rest, body.len() - rest.len())?;
        Ok(Self {
            session: SessionId::new(raw_session),
            lease_set,
            private_keys,
        })
    }

    /// Encodes the body.
    pub fn encode(&self) -> Result<Vec<u8>, I2cpError> {
        if self.private_keys.len() != self.lease_set.encryption_keys().len() {
            return Err(malformed(
                Self::NAME,
                CodecError::InvalidFieldValue {
                    offset: 0,
                    context: "decryption key count",
                },
            ));
        }
        let lease_bytes = self
            .lease_set
            .encode_to_vec(i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)
            .map_err(|source| malformed(Self::NAME, source))?;
        let mut out = Vec::with_capacity(4 + lease_bytes.len());
        out.extend_from_slice(&self.session.get().to_be_bytes());
        out.push(LEASE_SET_TYPE_STANDARD_V2);
        out.extend_from_slice(&lease_bytes);
        out.push(self.private_keys.len() as u8);
        for key in &self.private_keys {
            out.extend_from_slice(&key.key_type().code().to_be_bytes());
            out.extend_from_slice(&(key.as_bytes().len() as u16).to_be_bytes());
            out.extend_from_slice(key.as_bytes());
        }
        Ok(out)
    }

    /// Returns the owning session.
    pub const fn session(&self) -> SessionId {
        self.session
    }
}

/// A `SendMessage` body: session, target, payload, and nonce.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SendMessage {
    /// Sending session.
    pub session: SessionId,
    /// Target destination.
    pub destination: Destination,
    /// Application payload.
    pub payload: Payload,
    /// Client nonce (zero suppresses status replies).
    pub nonce: ClientNonce,
}

impl SendMessage {
    const NAME: &'static str = "send-message";

    /// Decodes a strict body.
    pub fn decode(body: &[u8]) -> Result<Self, I2cpError> {
        let (raw_session, rest) = read_u16(Self::NAME, body)?;
        let (destination, rest) = split_destination(Self::NAME, rest)?;
        let (payload, rest) = split_payload(rest).map_err(Self::remap_payload)?;
        if rest.len() != 4 {
            return Err(if rest.len() < 4 {
                truncated(Self::NAME, body.len() - rest.len(), 4, rest.len())
            } else {
                malformed(
                    Self::NAME,
                    CodecError::TrailingBytes {
                        offset: body.len() - rest.len() + 4,
                        remaining: rest.len() - 4,
                    },
                )
            });
        }
        let nonce = ClientNonce::new(u32::from_be_bytes([rest[0], rest[1], rest[2], rest[3]]));
        Ok(Self {
            session: SessionId::new(raw_session),
            destination,
            payload,
            nonce,
        })
    }

    /// Encodes the body.
    pub fn encode(&self) -> Result<Vec<u8>, I2cpError> {
        let mut out = Vec::with_capacity(2 + 391 + 4 + 4);
        out.extend_from_slice(&self.session.get().to_be_bytes());
        out.extend(encode_destination(Self::NAME, &self.destination)?);
        out.extend(self.payload.encode().map_err(Self::remap_payload)?);
        out.extend_from_slice(&self.nonce.get().to_be_bytes());
        Ok(out)
    }

    fn remap_payload(source: I2cpError) -> I2cpError {
        match source {
            I2cpError::Malformed { source, .. } => malformed(Self::NAME, source),
            I2cpError::Incomplete { needed } => truncated(Self::NAME, 0, needed, 0),
            other => other,
        }
    }
}

/// `SendMessageExpires` flag word with the specified bit layout.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SendFlags(u16);

impl SendFlags {
    /// Wraps raw flags after rejecting reserved bits 15–11.
    pub const fn new(raw: u16) -> Option<Self> {
        if raw & SEND_FLAGS_RESERVED_MASK != 0 {
            None
        } else {
            Some(Self(raw))
        }
    }

    /// Returns the raw flag word.
    pub const fn get(self) -> u16 {
        self.0
    }

    /// Returns the reliability-override bits (unimplemented; always ignored).
    pub const fn reliability_override(self) -> u8 {
        ((self.0 & SEND_FLAGS_RELIABILITY_MASK) >> 9) as u8
    }

    /// Reports the do-not-bundle hint (bit 8).
    pub const fn no_bundle(self) -> bool {
        self.0 & SEND_FLAGS_NO_BUNDLE != 0
    }

    /// Returns the low-tag-threshold nibble (ElGamal only).
    pub const fn tag_threshold(self) -> u8 {
        ((self.0 & SEND_FLAGS_TAG_THRESHOLD_MASK) >> 4) as u8
    }

    /// Returns the tags-to-send nibble (ElGamal only).
    pub const fn tags_to_send(self) -> u8 {
        (self.0 & SEND_FLAGS_TAGS_TO_SEND_MASK) as u8
    }
}

/// A `SendMessageExpires` body: session, target, payload, nonce,
/// flag word, and six-byte expiration instant.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SendMessageExpires {
    /// Sending session.
    pub session: SessionId,
    /// Target destination.
    pub destination: Destination,
    /// Application payload.
    pub payload: Payload,
    /// Client nonce.
    pub nonce: ClientNonce,
    /// Flag word (reserved bits must be zero).
    pub flags: SendFlags,
    /// Expiration instant in milliseconds since the Unix epoch,
    /// truncated to 48 bits (the upper two bytes carry the flags).
    pub expiration_ms: u64,
}

impl SendMessageExpires {
    const NAME: &'static str = "send-message-expires";

    /// Decodes a strict body.
    pub fn decode(body: &[u8]) -> Result<Self, I2cpError> {
        let (raw_session, rest) = read_u16(Self::NAME, body)?;
        let (destination, rest) = split_destination(Self::NAME, rest)?;
        let (payload, rest) = split_payload(rest).map_err(SendMessage::remap_payload)?;
        // Tail is exactly nonce (4) + flags (2) + expiration (6).
        if rest.len() != 12 {
            return Err(if rest.len() < 12 {
                truncated(Self::NAME, body.len() - rest.len(), 12, rest.len())
            } else {
                malformed(
                    Self::NAME,
                    CodecError::TrailingBytes {
                        offset: body.len() - rest.len() + 12,
                        remaining: rest.len() - 12,
                    },
                )
            });
        }
        let nonce = ClientNonce::new(u32::from_be_bytes([rest[0], rest[1], rest[2], rest[3]]));
        let raw_flags = u16::from_be_bytes([rest[4], rest[5]]);
        let flags = SendFlags::new(raw_flags).ok_or_else(|| {
            malformed(
                Self::NAME,
                CodecError::InvalidFieldValue {
                    offset: body.len() - rest.len() + 4,
                    context: "send-message-expires reserved flag bits",
                },
            )
        })?;
        let mut expiration = [0u8; 8];
        expiration[2..].copy_from_slice(&rest[6..12]);
        Ok(Self {
            session: SessionId::new(raw_session),
            destination,
            payload,
            nonce,
            flags,
            expiration_ms: u64::from_be_bytes(expiration),
        })
    }

    /// Encodes the body.
    pub fn encode(&self) -> Result<Vec<u8>, I2cpError> {
        if self.expiration_ms >> 48 != 0 {
            return Err(malformed(
                Self::NAME,
                CodecError::InvalidFieldValue {
                    offset: 0,
                    context: "send-message-expires expiration range",
                },
            ));
        }
        let mut out = Vec::with_capacity(2 + 391 + 4 + 4 + 2 + 6);
        out.extend_from_slice(&self.session.get().to_be_bytes());
        out.extend(encode_destination(Self::NAME, &self.destination)?);
        out.extend(self.payload.encode().map_err(SendMessage::remap_payload)?);
        out.extend_from_slice(&self.nonce.get().to_be_bytes());
        out.extend_from_slice(&self.flags.get().to_be_bytes());
        out.extend_from_slice(&self.expiration_ms.to_be_bytes()[2..]);
        Ok(out)
    }
}

/// A `MessagePayload` body: session, router message ID, and payload.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MessagePayload {
    /// Owning session.
    pub session: SessionId,
    /// Router-assigned message ID.
    pub message_id: MessageId,
    /// Application payload.
    pub payload: Payload,
}

impl MessagePayload {
    const NAME: &'static str = "message-payload";

    /// Decodes a strict body.
    pub fn decode(body: &[u8]) -> Result<Self, I2cpError> {
        let (raw_session, rest) = read_u16(Self::NAME, body)?;
        let (raw_id, rest) = read_u32(Self::NAME, rest)?;
        let (payload, rest) = split_payload(rest).map_err(SendMessage::remap_payload)?;
        require_empty(Self::NAME, rest, body.len() - rest.len())?;
        Ok(Self {
            session: SessionId::new(raw_session),
            message_id: MessageId::new(raw_id),
            payload,
        })
    }

    /// Encodes the body.
    pub fn encode(&self) -> Result<Vec<u8>, I2cpError> {
        let mut out = Vec::with_capacity(6 + 4);
        out.extend_from_slice(&self.session.get().to_be_bytes());
        out.extend_from_slice(&self.message_id.get().to_be_bytes());
        out.extend(self.payload.encode().map_err(SendMessage::remap_payload)?);
        Ok(out)
    }
}

/// `MessageStatus` outcome codes with success/failure semantics.
///
/// Codes 0–23 are assigned; higher values are reserved failures kept
/// for forward compatibility.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum MessageStatusCode {
    /// Incoming message is available (deprecated fast-receive path).
    Available = 0,
    /// Outgoing message accepted.
    Accepted = 1,
    /// Best-effort probable success (unused).
    BestEffortSuccess = 2,
    /// Best-effort probable failure.
    BestEffortFailure = 3,
    /// Guaranteed probable success.
    GuaranteedSuccess = 4,
    /// Generic failure.
    GuaranteedFailure = 5,
    /// Local delivery to a same-router client succeeded.
    LocalSuccess = 6,
    /// Local delivery to a same-router client failed.
    LocalFailure = 7,
    /// Router not ready, shut down, or in major trouble.
    RouterFailure = 8,
    /// No network connectivity.
    NetworkFailure = 9,
    /// Session invalid or closed.
    BadSession = 10,
    /// Invalid, zero-length, or oversized payload.
    BadMessage = 11,
    /// Invalid options or out-of-range expiration.
    BadOptions = 12,
    /// Router queue or buffer full; message dropped.
    OverflowFailure = 13,
    /// Message expired before sending.
    MessageExpired = 14,
    /// No signed local LeaseSet, invalid keys, expired, or empty.
    BadLocalLeaseSet = 15,
    /// No outbound tunnel (or no inbound tunnel for a reply).
    NoLocalTunnels = 16,
    /// Destination or LeaseSet uses unsupported encryption.
    UnsupportedEncryption = 17,
    /// Bad far-end format, options, or certificates.
    BadDestination = 18,
    /// Far-end LeaseSet has unsupported options, certificates, or tunnels.
    BadLeaseSet = 19,
    /// Far-end LeaseSet expired and no replacement is available.
    ExpiredLeaseSet = 20,
    /// Far-end LeaseSet cannot be found.
    NoLeaseSet = 21,
    /// Far end is a meta LeaseSet; use HostLookup for contents.
    MetaLeaseSet = 22,
    /// Loopback message from and to the same destination or session.
    LoopbackDenied = 23,
    /// Reserved failure code (forward compatibility).
    Reserved(u8) = 255,
}

impl MessageStatusCode {
    /// Decodes a status code; unassigned values become reserved failures.
    pub const fn from_u8(raw: u8) -> Self {
        match raw {
            0 => Self::Available,
            1 => Self::Accepted,
            2 => Self::BestEffortSuccess,
            3 => Self::BestEffortFailure,
            4 => Self::GuaranteedSuccess,
            5 => Self::GuaranteedFailure,
            6 => Self::LocalSuccess,
            7 => Self::LocalFailure,
            8 => Self::RouterFailure,
            9 => Self::NetworkFailure,
            10 => Self::BadSession,
            11 => Self::BadMessage,
            12 => Self::BadOptions,
            13 => Self::OverflowFailure,
            14 => Self::MessageExpired,
            15 => Self::BadLocalLeaseSet,
            16 => Self::NoLocalTunnels,
            17 => Self::UnsupportedEncryption,
            18 => Self::BadDestination,
            19 => Self::BadLeaseSet,
            20 => Self::ExpiredLeaseSet,
            21 => Self::NoLeaseSet,
            22 => Self::MetaLeaseSet,
            23 => Self::LoopbackDenied,
            reserved => Self::Reserved(reserved),
        }
    }

    /// Returns the numeric code.
    pub const fn code(self) -> u8 {
        match self {
            Self::Available => 0,
            Self::Accepted => 1,
            Self::BestEffortSuccess => 2,
            Self::BestEffortFailure => 3,
            Self::GuaranteedSuccess => 4,
            Self::GuaranteedFailure => 5,
            Self::LocalSuccess => 6,
            Self::LocalFailure => 7,
            Self::RouterFailure => 8,
            Self::NetworkFailure => 9,
            Self::BadSession => 10,
            Self::BadMessage => 11,
            Self::BadOptions => 12,
            Self::OverflowFailure => 13,
            Self::MessageExpired => 14,
            Self::BadLocalLeaseSet => 15,
            Self::NoLocalTunnels => 16,
            Self::UnsupportedEncryption => 17,
            Self::BadDestination => 18,
            Self::BadLeaseSet => 19,
            Self::ExpiredLeaseSet => 20,
            Self::NoLeaseSet => 21,
            Self::MetaLeaseSet => 22,
            Self::LoopbackDenied => 23,
            Self::Reserved(raw) => raw,
        }
    }

    /// Reports outgoing-message success (codes 1, 2, 4, 6).
    ///
    /// Reserved codes conservatively report failure.
    pub const fn is_success(self) -> bool {
        matches!(
            self,
            Self::Accepted | Self::BestEffortSuccess | Self::GuaranteedSuccess | Self::LocalSuccess
        )
    }
}

/// A `MessageStatus` body: session, message ID, outcome, size, nonce.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MessageStatus {
    /// Owning session.
    pub session: SessionId,
    /// Router-assigned message ID.
    pub message_id: MessageId,
    /// Outcome code.
    pub status: MessageStatusCode,
    /// Available message size (meaningful only for `Available`).
    pub size: u32,
    /// Client nonce echoed from the outbound submission.
    pub nonce: ClientNonce,
}

impl MessageStatus {
    const NAME: &'static str = "message-status";

    /// Decodes a strict fifteen-byte body.
    pub fn decode(body: &[u8]) -> Result<Self, I2cpError> {
        if body.len() != 15 {
            return Err(if body.len() < 15 {
                truncated(Self::NAME, 0, 15, body.len())
            } else {
                malformed(
                    Self::NAME,
                    CodecError::TrailingBytes {
                        offset: 15,
                        remaining: body.len() - 15,
                    },
                )
            });
        }
        Ok(Self {
            session: SessionId::new(u16::from_be_bytes([body[0], body[1]])),
            message_id: MessageId::new(u32::from_be_bytes([body[2], body[3], body[4], body[5]])),
            status: MessageStatusCode::from_u8(body[6]),
            size: u32::from_be_bytes([body[7], body[8], body[9], body[10]]),
            nonce: ClientNonce::new(u32::from_be_bytes([body[11], body[12], body[13], body[14]])),
        })
    }

    /// Encodes the body.
    pub fn encode(&self) -> Result<Vec<u8>, I2cpError> {
        let mut out = Vec::with_capacity(15);
        out.extend_from_slice(&self.session.get().to_be_bytes());
        out.extend_from_slice(&self.message_id.get().to_be_bytes());
        out.push(self.status.code());
        out.extend_from_slice(&self.size.to_be_bytes());
        out.extend_from_slice(&self.nonce.get().to_be_bytes());
        Ok(out)
    }
}

/// A `GetBandwidthLimits` body: always empty.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GetBandwidthLimits;

impl GetBandwidthLimits {
    const NAME: &'static str = "get-bandwidth-limits";

    /// Decodes the empty body.
    pub fn decode(body: &[u8]) -> Result<Self, I2cpError> {
        require_empty(Self::NAME, body, 0)?;
        Ok(Self)
    }

    /// Encodes the empty body.
    pub fn encode(&self) -> Result<Vec<u8>, I2cpError> {
        Ok(Vec::new())
    }
}

/// A `BandwidthLimits` body: sixteen four-byte integers.
///
/// Client limits may be the only meaningful values; router limits may
/// be zero depending on implementation policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BandwidthLimits {
    /// Client inbound limit (KBps).
    pub client_inbound: u32,
    /// Client outbound limit (KBps).
    pub client_outbound: u32,
    /// Router inbound limit (KBps).
    pub router_inbound: u32,
    /// Router inbound burst limit (KBps).
    pub router_inbound_burst: u32,
    /// Router outbound limit (KBps).
    pub router_outbound: u32,
    /// Router outbound burst limit (KBps).
    pub router_outbound_burst: u32,
    /// Router burst time (seconds).
    pub router_burst_time: u32,
    /// Nine reserved integers.
    pub reserved: [u32; 9],
}

impl BandwidthLimits {
    const NAME: &'static str = "bandwidth-limits";

    /// Decodes the strict 64-byte body.
    pub fn decode(body: &[u8]) -> Result<Self, I2cpError> {
        if body.len() != BANDWIDTH_LIMITS_BODY_LEN {
            return Err(if body.len() < BANDWIDTH_LIMITS_BODY_LEN {
                truncated(Self::NAME, 0, BANDWIDTH_LIMITS_BODY_LEN, body.len())
            } else {
                malformed(
                    Self::NAME,
                    CodecError::TrailingBytes {
                        offset: BANDWIDTH_LIMITS_BODY_LEN,
                        remaining: body.len() - BANDWIDTH_LIMITS_BODY_LEN,
                    },
                )
            });
        }
        let mut words = [0u32; 16];
        for (index, word) in words.iter_mut().enumerate() {
            let offset = index * 4;
            *word = u32::from_be_bytes([
                body[offset],
                body[offset + 1],
                body[offset + 2],
                body[offset + 3],
            ]);
        }
        let mut reserved = [0u32; 9];
        reserved.copy_from_slice(&words[7..]);
        Ok(Self {
            client_inbound: words[0],
            client_outbound: words[1],
            router_inbound: words[2],
            router_inbound_burst: words[3],
            router_outbound: words[4],
            router_outbound_burst: words[5],
            router_burst_time: words[6],
            reserved,
        })
    }

    /// Encodes the body.
    pub fn encode(&self) -> Result<Vec<u8>, I2cpError> {
        let mut out = Vec::with_capacity(BANDWIDTH_LIMITS_BODY_LEN);
        for word in [
            self.client_inbound,
            self.client_outbound,
            self.router_inbound,
            self.router_inbound_burst,
            self.router_outbound,
            self.router_outbound_burst,
            self.router_burst_time,
        ]
        .into_iter()
        .chain(self.reserved)
        {
            out.extend_from_slice(&word.to_be_bytes());
        }
        Ok(out)
    }
}

/// A `DestLookup` body: exactly one destination hash.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DestLookup {
    /// Requested destination hash.
    pub hash: Hash,
}

impl DestLookup {
    const NAME: &'static str = "dest-lookup";

    /// Decodes a strict body.
    pub fn decode(body: &[u8]) -> Result<Self, I2cpError> {
        if body.len() != 32 {
            return Err(if body.len() < 32 {
                truncated(Self::NAME, 0, 32, body.len())
            } else {
                malformed(
                    Self::NAME,
                    CodecError::TrailingBytes {
                        offset: 32,
                        remaining: body.len() - 32,
                    },
                )
            });
        }
        let mut hash = [0u8; 32];
        hash.copy_from_slice(body);
        Ok(Self {
            hash: Hash::from_bytes(hash),
        })
    }

    /// Encodes the body.
    pub fn encode(&self) -> Result<Vec<u8>, I2cpError> {
        Ok(self.hash.as_bytes().to_vec())
    }
}

/// A `DestReply` body: success destination, failure hash echo, or the
/// legacy empty failure (pre-0.8.3 routers).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DestReplyBody {
    /// Legacy empty failure response.
    LegacyEmpty,
    /// Requested hash echoed so outstanding lookups correlate.
    Hash(Hash),
    /// Resolved destination.
    Destination(Destination),
}

/// A `DestReply` body wrapper.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DestReply {
    /// Reply payload.
    pub body: DestReplyBody,
}

impl DestReply {
    const NAME: &'static str = "dest-reply";

    /// Decodes a strict body.
    ///
    /// A 32-byte body can never be a Destination (the smallest
    /// encoding is 387 bytes), so the hash/destination split is
    /// unambiguous.
    pub fn decode(body: &[u8]) -> Result<Self, I2cpError> {
        if body.is_empty() {
            return Ok(Self {
                body: DestReplyBody::LegacyEmpty,
            });
        }
        if body.len() == 32 {
            let mut hash = [0u8; 32];
            hash.copy_from_slice(body);
            return Ok(Self {
                body: DestReplyBody::Hash(Hash::from_bytes(hash)),
            });
        }
        let destination = Destination::decode(body, body.len())
            .map_err(|source| malformed(Self::NAME, source))?;
        Ok(Self {
            body: DestReplyBody::Destination(destination),
        })
    }

    /// Encodes the body.
    pub fn encode(&self) -> Result<Vec<u8>, I2cpError> {
        match &self.body {
            DestReplyBody::LegacyEmpty => Ok(Vec::new()),
            DestReplyBody::Hash(hash) => Ok(hash.as_bytes().to_vec()),
            DestReplyBody::Destination(destination) => encode_destination(Self::NAME, destination),
        }
    }
}

/// A `Disconnect` body: a human-readable reason string.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Disconnect {
    /// Teardown reason.
    pub reason: String,
}

impl Disconnect {
    const NAME: &'static str = "disconnect";

    /// Decodes a strict body.
    pub fn decode(body: &[u8]) -> Result<Self, I2cpError> {
        let (reason, rest) = split_string(Self::NAME, body)?;
        require_empty(Self::NAME, rest, body.len() - rest.len())?;
        Ok(Self { reason })
    }

    /// Encodes the body.
    pub fn encode(&self) -> Result<Vec<u8>, I2cpError> {
        encode_string(Self::NAME, &self.reason)
    }
}

/// `HostLookup` request types with their lookup-key shapes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HostLookupKey {
    /// Hash lookup.
    Hash(Hash),
    /// Hostname lookup.
    Hostname(String),
    /// Hash lookup with LeaseSet options returned (0.9.66).
    HashWithOptions(Hash),
    /// Hostname lookup with LeaseSet options returned (0.9.66).
    HostnameWithOptions(String),
    /// Destination lookup with LeaseSet options returned (0.9.66).
    DestinationWithOptions(Destination),
}

impl HostLookupKey {
    /// Returns the request type code.
    pub const fn request_type(&self) -> u8 {
        match self {
            Self::Hash(_) => 0,
            Self::Hostname(_) => 1,
            Self::HashWithOptions(_) => 2,
            Self::HostnameWithOptions(_) => 3,
            Self::DestinationWithOptions(_) => 4,
        }
    }
}

/// A `HostLookup` body: session, request ID, timeout, type, and key.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HostLookup {
    /// Requesting session (`NO_SESSION` when session-less).
    pub session: SessionId,
    /// Client request ID echoed in the reply.
    pub request_id: HostRequestId,
    /// Lookup timeout in milliseconds.
    pub timeout_ms: u32,
    /// Typed lookup key.
    pub key: HostLookupKey,
}

impl HostLookup {
    const NAME: &'static str = "host-lookup";

    /// Decodes a strict body.
    pub fn decode(body: &[u8]) -> Result<Self, I2cpError> {
        let (raw_session, rest) = read_u16(Self::NAME, body)?;
        let (raw_request, rest) = read_u32(Self::NAME, rest)?;
        let (timeout_ms, rest) = read_u32(Self::NAME, rest)?;
        if rest.is_empty() {
            return Err(truncated(Self::NAME, body.len(), 1, 0));
        }
        let request_type = rest[0];
        let rest = &rest[1..];
        let key = match request_type {
            0 | 2 => {
                if rest.len() != 32 {
                    return Err(if rest.len() < 32 {
                        truncated(Self::NAME, body.len() - rest.len(), 32, rest.len())
                    } else {
                        malformed(
                            Self::NAME,
                            CodecError::TrailingBytes {
                                offset: body.len() - rest.len() + 32,
                                remaining: rest.len() - 32,
                            },
                        )
                    });
                }
                let mut hash = [0u8; 32];
                hash.copy_from_slice(rest);
                let hash = Hash::from_bytes(hash);
                if request_type == 0 {
                    HostLookupKey::Hash(hash)
                } else {
                    HostLookupKey::HashWithOptions(hash)
                }
            }
            1 | 3 => {
                let (hostname, rest) = split_string(Self::NAME, rest)?;
                require_empty(Self::NAME, rest, body.len() - rest.len())?;
                if request_type == 1 {
                    HostLookupKey::Hostname(hostname)
                } else {
                    HostLookupKey::HostnameWithOptions(hostname)
                }
            }
            4 => {
                let (destination, rest) = split_destination(Self::NAME, rest)?;
                require_empty(Self::NAME, rest, body.len() - rest.len())?;
                HostLookupKey::DestinationWithOptions(destination)
            }
            other => {
                return Err(malformed(
                    Self::NAME,
                    CodecError::Unsupported {
                        offset: body.len() - rest.len() - 1,
                        context: "host lookup type",
                        value: u64::from(other),
                    },
                ));
            }
        };
        Ok(Self {
            session: SessionId::new(raw_session),
            request_id: HostRequestId::new(raw_request),
            timeout_ms,
            key,
        })
    }

    /// Encodes the body.
    pub fn encode(&self) -> Result<Vec<u8>, I2cpError> {
        let mut out = Vec::with_capacity(11);
        out.extend_from_slice(&self.session.get().to_be_bytes());
        out.extend_from_slice(&self.request_id.get().to_be_bytes());
        out.extend_from_slice(&self.timeout_ms.to_be_bytes());
        out.push(self.key.request_type());
        match &self.key {
            HostLookupKey::Hash(hash) | HostLookupKey::HashWithOptions(hash) => {
                out.extend_from_slice(hash.as_bytes());
            }
            HostLookupKey::Hostname(hostname) | HostLookupKey::HostnameWithOptions(hostname) => {
                out.extend(encode_string(Self::NAME, hostname)?);
            }
            HostLookupKey::DestinationWithOptions(destination) => {
                out.extend(encode_destination(Self::NAME, destination)?);
            }
        }
        Ok(out)
    }
}

/// `HostReply` result codes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum HostReplyResult {
    /// Lookup succeeded.
    Success = 0,
    /// Generic failure.
    Failure = 1,
    /// A lookup password is required (0.9.43 blinded flow).
    LookupPasswordRequired = 2,
    /// A decryption private key is required (0.9.43 blinded flow).
    PrivateKeyRequired = 3,
    /// Both password and private key are required.
    PasswordAndKeyRequired = 4,
    /// LeaseSet decryption failed.
    LeasesetDecryptionFailure = 5,
    /// LeaseSet lookup failed (0.9.66 type 2–4 flow).
    LeasesetLookupFailure = 6,
    /// Lookup type unsupported.
    LookupTypeUnsupported = 7,
    /// Reserved failure code (forward compatibility).
    Reserved(u8) = 255,
}

impl HostReplyResult {
    /// Decodes a result code; unassigned values become reserved failures.
    pub const fn from_u8(raw: u8) -> Self {
        match raw {
            0 => Self::Success,
            1 => Self::Failure,
            2 => Self::LookupPasswordRequired,
            3 => Self::PrivateKeyRequired,
            4 => Self::PasswordAndKeyRequired,
            5 => Self::LeasesetDecryptionFailure,
            6 => Self::LeasesetLookupFailure,
            7 => Self::LookupTypeUnsupported,
            reserved => Self::Reserved(reserved),
        }
    }

    /// Returns the numeric code.
    pub const fn code(self) -> u8 {
        match self {
            Self::Success => 0,
            Self::Failure => 1,
            Self::LookupPasswordRequired => 2,
            Self::PrivateKeyRequired => 3,
            Self::PasswordAndKeyRequired => 4,
            Self::LeasesetDecryptionFailure => 5,
            Self::LeasesetLookupFailure => 6,
            Self::LookupTypeUnsupported => 7,
            Self::Reserved(raw) => raw,
        }
    }
}

/// A `HostReply` body: session, request ID, result, optional
/// destination, and optional LeaseSet options (type 2–4 lookups).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HostReply {
    /// Replying session.
    pub session: SessionId,
    /// Echoed client request ID.
    pub request_id: HostRequestId,
    /// Result code.
    pub result: HostReplyResult,
    /// Resolved destination (present on success; may also appear
    /// with type 2–4 lookups on `LeasesetLookupFailure`).
    pub destination: Option<Destination>,
    /// LeaseSet options for type 2–4 lookups (may be empty).
    pub options: Option<Mapping>,
}

impl HostReply {
    const NAME: &'static str = "host-reply";

    /// Decodes a strict body.
    pub fn decode(body: &[u8]) -> Result<Self, I2cpError> {
        let (raw_session, rest) = read_u16(Self::NAME, body)?;
        let (raw_request, rest) = read_u32(Self::NAME, rest)?;
        if rest.is_empty() {
            return Err(truncated(Self::NAME, 6, 1, 0));
        }
        let result = HostReplyResult::from_u8(rest[0]);
        let mut rest = &rest[1..];
        let mut destination = None;
        let mut options = None;
        if !rest.is_empty() {
            let (dest, remaining) = split_destination(Self::NAME, rest)?;
            destination = Some(dest);
            rest = remaining;
        }
        if !rest.is_empty() {
            let (mapping, consumed) = split_mapping_lenient(rest)?;
            require_empty(
                Self::NAME,
                &rest[consumed..],
                body.len() - rest.len() + consumed,
            )?;
            options = Some(mapping);
        }
        Ok(Self {
            session: SessionId::new(raw_session),
            request_id: HostRequestId::new(raw_request),
            result,
            destination,
            options,
        })
    }

    /// Encodes the body.
    pub fn encode(&self) -> Result<Vec<u8>, I2cpError> {
        let mut out = Vec::with_capacity(7);
        out.extend_from_slice(&self.session.get().to_be_bytes());
        out.extend_from_slice(&self.request_id.get().to_be_bytes());
        out.push(self.result.code());
        if let Some(destination) = &self.destination {
            out.extend(encode_destination(Self::NAME, destination)?);
        }
        if let Some(options) = &self.options {
            out.extend(encode_mapping(options)?);
        }
        Ok(out)
    }
}

/// Any implemented M9 message body.
#[derive(Debug, PartialEq)]
pub enum Message {
    /// Version handshake from the client.
    GetDate(GetDate),
    /// Clock and version handshake from the router.
    SetDate(SetDate),
    /// Destination session creation.
    CreateSession(CreateSession),
    /// Session option adjustment.
    ReconfigureSession(ReconfigureSession),
    /// Session destruction.
    DestroySession(DestroySession),
    /// Session lifecycle outcome.
    SessionStatus(SessionStatus),
    /// Variable-lease request from the router.
    RequestVariableLeaseSet(RequestVariableLeaseSet),
    /// Standard LeaseSet2 publication from the client.
    CreateLeaseSet2(CreateLeaseSet2),
    /// Outbound application message.
    SendMessage(SendMessage),
    /// Expiring outbound application message.
    SendMessageExpires(SendMessageExpires),
    /// Inbound application payload.
    MessagePayload(MessagePayload),
    /// Message outcome notification.
    MessageStatus(MessageStatus),
    /// Bandwidth limit request.
    GetBandwidthLimits(GetBandwidthLimits),
    /// Bandwidth limits.
    BandwidthLimits(BandwidthLimits),
    /// Destination hash lookup.
    DestLookup(DestLookup),
    /// Destination lookup reply.
    DestReply(DestReply),
    /// Connection teardown.
    Disconnect(Disconnect),
    /// Hostname/hash lookup with request ID.
    HostLookup(HostLookup),
    /// Hostname/hash lookup reply.
    HostReply(HostReply),
}

impl Eq for Message {}

impl Message {
    /// Returns the message type.
    pub const fn message_type(&self) -> MessageType {
        match self {
            Self::GetDate(_) => MessageType::GetDate,
            Self::SetDate(_) => MessageType::SetDate,
            Self::CreateSession(_) => MessageType::CreateSession,
            Self::ReconfigureSession(_) => MessageType::ReconfigureSession,
            Self::DestroySession(_) => MessageType::DestroySession,
            Self::SessionStatus(_) => MessageType::SessionStatus,
            Self::RequestVariableLeaseSet(_) => MessageType::RequestVariableLeaseSet,
            Self::CreateLeaseSet2(_) => MessageType::CreateLeaseSet2,
            Self::SendMessage(_) => MessageType::SendMessage,
            Self::SendMessageExpires(_) => MessageType::SendMessageExpires,
            Self::MessagePayload(_) => MessageType::MessagePayload,
            Self::MessageStatus(_) => MessageType::MessageStatus,
            Self::GetBandwidthLimits(_) => MessageType::GetBandwidthLimits,
            Self::BandwidthLimits(_) => MessageType::BandwidthLimits,
            Self::DestLookup(_) => MessageType::DestLookup,
            Self::DestReply(_) => MessageType::DestReply,
            Self::Disconnect(_) => MessageType::Disconnect,
            Self::HostLookup(_) => MessageType::HostLookup,
            Self::HostReply(_) => MessageType::HostReply,
        }
    }

    /// Encodes the message body.
    pub fn encode_body(&self) -> Result<Vec<u8>, I2cpError> {
        match self {
            Self::GetDate(body) => body.encode(),
            Self::SetDate(body) => body.encode(),
            Self::CreateSession(body) => body.encode(),
            Self::ReconfigureSession(body) => body.encode(),
            Self::DestroySession(body) => body.encode(),
            Self::SessionStatus(body) => body.encode(),
            Self::RequestVariableLeaseSet(body) => body.encode(),
            Self::CreateLeaseSet2(body) => body.encode(),
            Self::SendMessage(body) => body.encode(),
            Self::SendMessageExpires(body) => body.encode(),
            Self::MessagePayload(body) => body.encode(),
            Self::MessageStatus(body) => body.encode(),
            Self::GetBandwidthLimits(body) => body.encode(),
            Self::BandwidthLimits(body) => body.encode(),
            Self::DestLookup(body) => body.encode(),
            Self::DestReply(body) => body.encode(),
            Self::Disconnect(body) => body.encode(),
            Self::HostLookup(body) => body.encode(),
            Self::HostReply(body) => body.encode(),
        }
    }
}

/// Decodes one typed message body.
///
/// Unknown types fail with [`I2cpError::UnknownMessageType`],
/// deprecated types with [`I2cpError::DeprecatedMessageType`], and
/// M9-unsupported types with [`I2cpError::UnsupportedMessageType`],
/// all without inspecting the body. Implemented types decode
/// strictly and reject trailing bytes.
pub fn decode_typed(raw_type: u8, body: &[u8]) -> Result<Message, I2cpError> {
    let message_type =
        MessageType::from_u8(raw_type).ok_or(I2cpError::UnknownMessageType { raw: raw_type })?;
    match message_type.disposition() {
        MessageDisposition::LegacyDeprecated => {
            Err(I2cpError::DeprecatedMessageType { raw: raw_type })
        }
        MessageDisposition::ExplicitlyUnsupported => {
            Err(I2cpError::UnsupportedMessageType { raw: raw_type })
        }
        MessageDisposition::ImplementedM9 => {
            let message = match message_type {
                MessageType::GetDate => Message::GetDate(GetDate::decode(body)?),
                MessageType::SetDate => Message::SetDate(SetDate::decode(body)?),
                MessageType::CreateSession => Message::CreateSession(CreateSession::decode(body)?),
                MessageType::ReconfigureSession => {
                    Message::ReconfigureSession(ReconfigureSession::decode(body)?)
                }
                MessageType::DestroySession => {
                    Message::DestroySession(DestroySession::decode(body)?)
                }
                MessageType::SessionStatus => Message::SessionStatus(SessionStatus::decode(body)?),
                MessageType::RequestVariableLeaseSet => {
                    Message::RequestVariableLeaseSet(RequestVariableLeaseSet::decode(body)?)
                }
                MessageType::CreateLeaseSet2 => {
                    Message::CreateLeaseSet2(CreateLeaseSet2::decode(body)?)
                }
                MessageType::SendMessage => Message::SendMessage(SendMessage::decode(body)?),
                MessageType::SendMessageExpires => {
                    Message::SendMessageExpires(SendMessageExpires::decode(body)?)
                }
                MessageType::MessagePayload => {
                    Message::MessagePayload(MessagePayload::decode(body)?)
                }
                MessageType::MessageStatus => Message::MessageStatus(MessageStatus::decode(body)?),
                MessageType::GetBandwidthLimits => {
                    Message::GetBandwidthLimits(GetBandwidthLimits::decode(body)?)
                }
                MessageType::BandwidthLimits => {
                    Message::BandwidthLimits(BandwidthLimits::decode(body)?)
                }
                MessageType::DestLookup => Message::DestLookup(DestLookup::decode(body)?),
                MessageType::DestReply => Message::DestReply(DestReply::decode(body)?),
                MessageType::Disconnect => Message::Disconnect(Disconnect::decode(body)?),
                MessageType::HostLookup => Message::HostLookup(HostLookup::decode(body)?),
                MessageType::HostReply => Message::HostReply(HostReply::decode(body)?),
                MessageType::CreateLeaseSet
                | MessageType::ReceiveMessageBegin
                | MessageType::ReceiveMessageEnd
                | MessageType::RequestLeaseSet
                | MessageType::ReportAbuse
                | MessageType::BlindingInfo => {
                    debug_assert!(false, "disposition mismatch");
                    return Err(I2cpError::UnsupportedMessageType { raw: raw_type });
                }
            };
            Ok(message)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i2cp::mapping::MAX_I2CP_MAPPING_BODY_BYTES;
    use i2pr_proto::{
        Certificate, CryptoKeyType, Date32, KeyAndCert, KeyCertificate, Lease2,
        LeaseSet2EncryptionKey, LeaseSet2Flags, LeaseSet2Header, PublicKey, SignatureValue,
        SigningKeyType, SigningPublicKey,
    };

    fn test_destination() -> Destination {
        let public = PublicKey::new(CryptoKeyType::X25519, vec![0x11; 32]).expect("public");
        let signing = SigningPublicKey::new(SigningKeyType::EdDsaSha512Ed25519, vec![0x22; 32])
            .expect("signing");
        let certificate = Certificate::Key(
            KeyCertificate::for_types(SigningKeyType::EdDsaSha512Ed25519, CryptoKeyType::X25519)
                .expect("cert"),
        );
        Destination::new(
            KeyAndCert::new(public, signing, vec![0x33; 320], certificate).expect("keys"),
        )
        .expect("destination")
    }

    fn test_options() -> Mapping {
        Mapping::from_entries(vec![
            ("i2cp.leaseSetEncType".to_owned(), "4".to_owned()),
            ("inbound.length".to_owned(), "2".to_owned()),
        ])
        .expect("options")
    }

    fn test_session_config() -> SessionConfig {
        let destination = test_destination();
        let options = test_options();
        let mut config = destination
            .encode_to_vec(MAX_I2CP_DESTINATION_BYTES)
            .expect("dest bytes");
        config.extend(
            options
                .encode_to_vec(MAX_I2CP_MAPPING_BODY_BYTES + 2)
                .expect("mapping bytes"),
        );
        let creation_ms = 1_786_000_000_000u64;
        config.extend_from_slice(&creation_ms.to_be_bytes());
        let signature = vec![0x44u8; 64];
        config.extend_from_slice(&signature);
        SessionConfig::decode(&config).expect("session config")
    }

    fn test_lease_set2() -> LeaseSet2 {
        let destination = test_destination();
        let header =
            LeaseSet2Header::new(destination, 1_786_000_000, 600, LeaseSet2Flags::from_raw(0))
                .expect("header");
        let options =
            Mapping::from_entries(vec![("g".to_owned(), "0".to_owned())]).expect("ls2 options");
        let encryption_keys =
            vec![LeaseSet2EncryptionKey::new(CryptoKeyType::X25519, vec![0x55; 32]).expect("key")];
        let leases = vec![Lease2::new(
            Hash::from_bytes([0x66; 32]),
            7,
            Date32::from_seconds(1_786_000_100),
        )];
        let signature =
            SignatureValue::new(SigningKeyType::EdDsaSha512Ed25519, vec![0x77; 64]).expect("sig");
        LeaseSet2::new(header, options, encryption_keys, leases, signature).expect("lease set2")
    }

    fn round_trip(message: &Message) {
        let body = message.encode_body().expect("encode");
        let decoded = decode_typed(message.message_type().code(), &body).expect("decode");
        assert_eq!(&decoded, message);
    }

    fn with_trailing(message: &Message) -> Vec<u8> {
        let mut body = message.encode_body().expect("encode");
        body.push(0xff);
        body
    }

    #[test]
    fn type_ids_match_spec() {
        let cases = [
            (MessageType::CreateSession, 1u8),
            (MessageType::ReconfigureSession, 2),
            (MessageType::DestroySession, 3),
            (MessageType::CreateLeaseSet, 4),
            (MessageType::SendMessage, 5),
            (MessageType::ReceiveMessageBegin, 6),
            (MessageType::ReceiveMessageEnd, 7),
            (MessageType::GetBandwidthLimits, 8),
            (MessageType::SessionStatus, 20),
            (MessageType::RequestLeaseSet, 21),
            (MessageType::MessageStatus, 22),
            (MessageType::BandwidthLimits, 23),
            (MessageType::ReportAbuse, 29),
            (MessageType::Disconnect, 30),
            (MessageType::MessagePayload, 31),
            (MessageType::GetDate, 32),
            (MessageType::SetDate, 33),
            (MessageType::DestLookup, 34),
            (MessageType::DestReply, 35),
            (MessageType::SendMessageExpires, 36),
            (MessageType::RequestVariableLeaseSet, 37),
            (MessageType::HostLookup, 38),
            (MessageType::HostReply, 39),
            (MessageType::CreateLeaseSet2, 41),
            (MessageType::BlindingInfo, 42),
        ];
        for (ty, code) in cases {
            assert_eq!(ty.code(), code);
            assert_eq!(MessageType::from_u8(code), Some(ty));
        }
        // The abandoned preliminary type has no assignment.
        assert_eq!(MessageType::from_u8(40), None);
        assert_eq!(MessageType::from_u8(0), None);
        assert_eq!(MessageType::from_u8(255), None);
    }

    #[test]
    fn dispositions_match_m9_profile() {
        for raw in [4u8, 6, 7, 21, 29] {
            let ty = MessageType::from_u8(raw).expect("assigned");
            assert_eq!(ty.disposition(), MessageDisposition::LegacyDeprecated);
            assert!(matches!(
                decode_typed(raw, &[]),
                Err(I2cpError::DeprecatedMessageType { .. })
            ));
        }
        assert_eq!(
            MessageType::BlindingInfo.disposition(),
            MessageDisposition::ExplicitlyUnsupported
        );
        assert!(matches!(
            decode_typed(42, &[]),
            Err(I2cpError::UnsupportedMessageType { .. })
        ));
        for raw in [0u8, 9, 40, 43, 255] {
            assert_eq!(MessageType::from_u8(raw), None);
            assert!(matches!(
                decode_typed(raw, &[]),
                Err(I2cpError::UnknownMessageType { .. })
            ));
        }
    }

    #[test]
    fn get_date_round_trip_without_auth() {
        let message = Message::GetDate(GetDate {
            version: "0.9.67".to_owned(),
            auth: None,
        });
        round_trip(&message);
    }

    #[test]
    fn get_date_round_trip_with_auth_mapping() {
        let auth = Mapping::from_entries(vec![
            ("i2cp.password".to_owned(), "secret".to_owned()),
            ("i2cp.username".to_owned(), "user".to_owned()),
        ])
        .expect("auth");
        let message = Message::GetDate(GetDate {
            version: "0.9.67".to_owned(),
            auth: Some(auth),
        });
        round_trip(&message);
    }

    #[test]
    fn get_date_rejects_trailing_garbage() {
        let message = Message::GetDate(GetDate {
            version: "0.9.67".to_owned(),
            auth: None,
        });
        let body = with_trailing(&message);
        assert!(matches!(
            decode_typed(32, &body),
            Err(I2cpError::Malformed { .. })
        ));
    }

    #[test]
    fn set_date_round_trip() {
        let message = Message::SetDate(SetDate {
            date_ms: 1_786_000_000_123,
            version: "0.9.67".to_owned(),
        });
        round_trip(&message);
        assert!(matches!(
            SetDate::decode(&[0u8; 7]),
            Err(I2cpError::Malformed { .. })
        ));
    }

    #[test]
    fn session_config_retains_signed_region() {
        let config = test_session_config();
        assert_eq!(config.creation_ms(), 1_786_000_000_000);
        assert_eq!(config.signature().as_bytes(), &[0x44u8; 64]);
        let region = config.signed_region();
        let re_encoded = config.encode().expect("encode");
        assert_eq!(&re_encoded[..region.len()], region);
        assert_eq!(region.len() + 64, re_encoded.len());
        // Round-trip through decode preserves every field.
        let decoded = SessionConfig::decode(&re_encoded).expect("decode");
        assert_eq!(decoded, config);
    }

    #[test]
    fn session_config_rejects_short_signature() {
        let config = test_session_config();
        let mut encoded = config.encode().expect("encode");
        encoded.pop();
        assert!(matches!(
            SessionConfig::decode(&encoded),
            Err(I2cpError::Malformed { .. })
        ));
    }

    #[test]
    fn session_config_rejects_trailing_byte() {
        let config = test_session_config();
        let mut encoded = config.encode().expect("encode");
        encoded.push(0x00);
        assert!(matches!(
            SessionConfig::decode(&encoded),
            Err(I2cpError::Malformed { .. })
        ));
    }

    #[test]
    fn session_config_rejects_unsorted_options() {
        // Rebuild a configuration whose mapping entries arrive in
        // reverse canonical order; the strict posture must fail.
        let destination = test_destination();
        let mut raw = destination
            .encode_to_vec(MAX_I2CP_DESTINATION_BYTES)
            .expect("dest");
        // `z` sorts after `a`, so `z`-first wire order is
        // non-canonical and the strict posture must fail.
        let first = Mapping::from_entries(vec![("z".to_owned(), "1".to_owned())]).expect("m1");
        let second = Mapping::from_entries(vec![("a".to_owned(), "2".to_owned())]).expect("m2");
        let first_bytes = first
            .encode_to_vec(MAX_I2CP_MAPPING_BODY_BYTES + 2)
            .expect("m1 bytes");
        let second_bytes = second
            .encode_to_vec(MAX_I2CP_MAPPING_BODY_BYTES + 2)
            .expect("m2 bytes");
        // Strip each two-byte length prefix, join entries, re-prefix.
        let mut body = Vec::new();
        body.extend_from_slice(&first_bytes[2..]);
        body.extend_from_slice(&second_bytes[2..]);
        let mut mapping_bytes = ((body.len() as u16).to_be_bytes()).to_vec();
        mapping_bytes.extend_from_slice(&body);
        raw.extend_from_slice(&mapping_bytes);
        raw.extend_from_slice(&1_786_000_000_000u64.to_be_bytes());
        raw.extend_from_slice(&[0x44u8; 64]);
        assert!(matches!(
            SessionConfig::decode(&raw),
            Err(I2cpError::Malformed { .. })
        ));
    }

    #[test]
    fn create_reconfigure_destroy_round_trip() {
        let config = test_session_config();
        round_trip(&Message::CreateSession(CreateSession {
            config: config.clone(),
        }));
        round_trip(&Message::ReconfigureSession(ReconfigureSession {
            session: SessionId::new(3),
            config,
        }));
        round_trip(&Message::DestroySession(DestroySession {
            session: SessionId::new(3),
        }));
        assert!(matches!(
            DestroySession::decode(&[0x00]),
            Err(I2cpError::Malformed { .. })
        ));
        assert!(matches!(
            DestroySession::decode(&[0x00, 0x03, 0xff]),
            Err(I2cpError::Malformed { .. })
        ));
    }

    #[test]
    fn session_status_codes_round_trip() {
        for (raw, expected) in [
            (0u8, SessionStatusCode::Destroyed),
            (1, SessionStatusCode::Created),
            (2, SessionStatusCode::Updated),
            (3, SessionStatusCode::Invalid),
            (4, SessionStatusCode::Refused),
        ] {
            let message = Message::SessionStatus(SessionStatus {
                session: SessionId::new(9),
                status: expected,
            });
            round_trip(&message);
            assert_eq!(SessionStatusCode::from_u8(raw), Some(expected));
        }
        assert_eq!(SessionStatusCode::from_u8(5), None);
        assert!(matches!(
            SessionStatus::decode(&[0x00, 0x09, 0x05]),
            Err(I2cpError::Malformed { .. })
        ));
        assert!(matches!(
            SessionStatus::decode(&[0x00, 0x09]),
            Err(I2cpError::Malformed { .. })
        ));
    }

    #[test]
    fn request_variable_lease_set_bounds() {
        let message = Message::RequestVariableLeaseSet(RequestVariableLeaseSet {
            session: SessionId::new(1),
            leases: vec![
                RequestedLease {
                    gateway: Hash::from_bytes([0x01; 32]),
                    tunnel_id: 11,
                    end_date_ms: 1_786_000_000_000,
                },
                RequestedLease {
                    gateway: Hash::from_bytes([0x02; 32]),
                    tunnel_id: 12,
                    end_date_ms: 1_786_000_000_000,
                },
            ],
        });
        round_trip(&message);
        // Empty lease list is structurally valid.
        round_trip(&Message::RequestVariableLeaseSet(RequestVariableLeaseSet {
            session: SessionId::new(1),
            leases: Vec::new(),
        }));
        // Sixteen leases (the LeaseSet2 ceiling) round-trip.
        let many = RequestVariableLeaseSet {
            session: SessionId::new(1),
            leases: (0u32..16)
                .map(|index| RequestedLease {
                    gateway: Hash::from_bytes([index as u8; 32]),
                    tunnel_id: index,
                    end_date_ms: 1_786_000_000_000 + u64::from(index),
                })
                .collect(),
        };
        round_trip(&Message::RequestVariableLeaseSet(many));
        // Seventeen leases exceed the ceiling on encode and decode.
        let mut over = vec![0x00u8, 0x01, 17];
        over.extend_from_slice(&[0xabu8; 44]);
        assert!(matches!(
            RequestVariableLeaseSet::decode(&over),
            Err(I2cpError::Malformed { .. })
        ));
        // Truncated lease entry.
        assert!(matches!(
            RequestVariableLeaseSet::decode(&[0x00, 0x01, 0x01, 0xcc]),
            Err(I2cpError::Malformed { .. })
        ));
    }

    fn test_create_lease_set2() -> Message {
        Message::CreateLeaseSet2(CreateLeaseSet2 {
            session: SessionId::new(4),
            lease_set: test_lease_set2(),
            private_keys: vec![
                SessionDecryptionKey::new(CryptoKeyType::X25519, vec![0x99; 32]).expect("key"),
            ],
        })
    }

    #[test]
    fn create_lease_set2_round_trip() {
        round_trip(&test_create_lease_set2());
    }

    #[test]
    fn create_lease_set2_rejects_unsupported_ls_types() {
        let message = test_create_lease_set2();
        let body = message.encode_body().expect("encode");
        for ls_type in [1u8, 5, 7, 9] {
            let mut patched = body.clone();
            patched[2] = ls_type;
            assert!(
                matches!(
                    CreateLeaseSet2::decode(&patched),
                    Err(I2cpError::Malformed { .. })
                ),
                "lease-set type {ls_type}"
            );
        }
    }

    #[test]
    fn create_lease_set2_rejects_key_count_mismatch() {
        let message = test_create_lease_set2();
        let mut body = message.encode_body().expect("encode");
        // The key-count byte sits right after the LeaseSet2 bytes;
        // flipping it to zero must fail the count check.
        let lease_len = test_lease_set2()
            .encode_to_vec(i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)
            .expect("ls2 bytes")
            .len();
        body[3 + lease_len] = 0;
        assert!(matches!(
            CreateLeaseSet2::decode(&body),
            Err(I2cpError::Malformed { .. })
        ));
    }

    #[test]
    fn create_lease_set2_rejects_key_order_mismatch() {
        let message = test_create_lease_set2();
        let mut body = message.encode_body().expect("encode");
        let lease_len = test_lease_set2()
            .encode_to_vec(i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)
            .expect("ls2 bytes")
            .len();
        // Key entry starts after session(2) + type(1) + LS2 + count(1).
        let entry = 3 + lease_len + 1;
        body[entry] = 0x00;
        body[entry + 1] = 0x00; // ElGamal instead of X25519
        assert!(matches!(
            CreateLeaseSet2::decode(&body),
            Err(I2cpError::Malformed { .. })
        ));
    }

    #[test]
    fn send_message_round_trip() {
        let message = Message::SendMessage(SendMessage {
            session: SessionId::new(2),
            destination: test_destination(),
            payload: Payload::new(vec![0xde, 0xad]).expect("payload"),
            nonce: ClientNonce::new(42),
        });
        round_trip(&message);
        let body = with_trailing(&message);
        assert!(matches!(
            decode_typed(5, &body),
            Err(I2cpError::Malformed { .. })
        ));
        // Truncated destination certificate.
        let mut short = vec![0x00u8, 0x02];
        short.extend_from_slice(&[0x11u8; 100]);
        assert!(matches!(
            SendMessage::decode(&short),
            Err(I2cpError::Malformed { .. })
        ));
    }

    #[test]
    fn send_message_expires_round_trip() {
        let message = Message::SendMessageExpires(SendMessageExpires {
            session: SessionId::new(2),
            destination: test_destination(),
            payload: Payload::new(vec![0x01]).expect("payload"),
            nonce: ClientNonce::new(7),
            flags: SendFlags::new(0x0103).expect("flags"),
            expiration_ms: 0x0000_1234_5678,
        });
        round_trip(&message);
        let decoded = decode_typed(36, &message.encode_body().expect("encode")).expect("decode");
        let Message::SendMessageExpires(expires) = decoded else {
            panic!("wrong message");
        };
        assert!(expires.flags.no_bundle());
        assert_eq!(expires.flags.tags_to_send(), 3);
        assert_eq!(expires.expiration_ms, 0x0000_1234_5678);
        // Reserved flag bits must be zero.
        let mut body = message.encode_body().expect("encode");
        let flag_offset = body.len() - 8;
        body[flag_offset] |= 0x80;
        assert!(matches!(
            SendMessageExpires::decode(&body),
            Err(I2cpError::Malformed { .. })
        ));
        // Expiration beyond 48 bits cannot encode.
        let overflow = SendMessageExpires {
            session: SessionId::new(2),
            destination: test_destination(),
            payload: Payload::new(vec![0x01]).expect("payload"),
            nonce: ClientNonce::NONE,
            flags: SendFlags::new(0).expect("flags"),
            expiration_ms: 1 << 48,
        };
        assert!(matches!(
            overflow.encode(),
            Err(I2cpError::Malformed { .. })
        ));
    }

    #[test]
    fn message_payload_round_trip() {
        let message = Message::MessagePayload(MessagePayload {
            session: SessionId::new(5),
            message_id: MessageId::new(0x0102_0304),
            payload: Payload::new(vec![0x99; 16]).expect("payload"),
        });
        round_trip(&message);
    }

    #[test]
    fn message_status_round_trip_and_success_classes() {
        for raw in 0..=23u8 {
            let code = MessageStatusCode::from_u8(raw);
            assert_eq!(code.code(), raw);
            let message = Message::MessageStatus(MessageStatus {
                session: SessionId::new(5),
                message_id: MessageId::new(1),
                status: code,
                size: 64,
                nonce: ClientNonce::new(9),
            });
            round_trip(&message);
        }
        assert!(MessageStatusCode::Accepted.is_success());
        assert!(MessageStatusCode::BestEffortSuccess.is_success());
        assert!(MessageStatusCode::GuaranteedSuccess.is_success());
        assert!(MessageStatusCode::LocalSuccess.is_success());
        assert!(!MessageStatusCode::Available.is_success());
        assert!(!MessageStatusCode::GuaranteedFailure.is_success());
        assert!(!MessageStatusCode::Reserved(200).is_success());
        assert_eq!(MessageStatusCode::Reserved(200).code(), 200);
        // Fifteen bytes exactly: fourteen truncate, sixteen trail.
        assert!(matches!(
            MessageStatus::decode(&[0u8; 14]),
            Err(I2cpError::Malformed { .. })
        ));
        assert!(matches!(
            MessageStatus::decode(&[0u8; 16]),
            Err(I2cpError::Malformed { .. })
        ));
    }

    #[test]
    fn bandwidth_messages_round_trip() {
        round_trip(&Message::GetBandwidthLimits(GetBandwidthLimits));
        assert!(matches!(
            GetBandwidthLimits::decode(&[0x00]),
            Err(I2cpError::Malformed { .. })
        ));
        let message = Message::BandwidthLimits(BandwidthLimits {
            client_inbound: 128,
            client_outbound: 64,
            router_inbound: 1024,
            router_inbound_burst: 2048,
            router_outbound: 1024,
            router_outbound_burst: 2048,
            router_burst_time: 10,
            reserved: [0; 9],
        });
        round_trip(&message);
        assert!(matches!(
            BandwidthLimits::decode(&[0u8; BANDWIDTH_LIMITS_BODY_LEN - 1]),
            Err(I2cpError::Malformed { .. })
        ));
        assert!(matches!(
            BandwidthLimits::decode(&[0u8; BANDWIDTH_LIMITS_BODY_LEN + 1]),
            Err(I2cpError::Malformed { .. })
        ));
    }

    #[test]
    fn dest_lookup_round_trip() {
        let message = Message::DestLookup(DestLookup {
            hash: Hash::from_bytes([0xabu8; 32]),
        });
        round_trip(&message);
        assert!(matches!(
            DestLookup::decode(&[0u8; 31]),
            Err(I2cpError::Malformed { .. })
        ));
        assert!(matches!(
            DestLookup::decode(&[0u8; 33]),
            Err(I2cpError::Malformed { .. })
        ));
    }

    #[test]
    fn dest_reply_three_shapes() {
        round_trip(&Message::DestReply(DestReply {
            body: DestReplyBody::LegacyEmpty,
        }));
        round_trip(&Message::DestReply(DestReply {
            body: DestReplyBody::Hash(Hash::from_bytes([0x11; 32])),
        }));
        round_trip(&Message::DestReply(DestReply {
            body: DestReplyBody::Destination(test_destination()),
        }));
        // A 33-byte body is neither a hash nor a destination.
        assert!(matches!(
            DestReply::decode(&[0x77u8; 33]),
            Err(I2cpError::Malformed { .. })
        ));
    }

    #[test]
    fn disconnect_round_trip() {
        let message = Message::Disconnect(Disconnect {
            reason: "session destroyed".to_owned(),
        });
        round_trip(&message);
        round_trip(&Message::Disconnect(Disconnect {
            reason: String::new(),
        }));
    }

    #[test]
    fn string_ceiling_enforced() {
        let long = "x".repeat(MAX_I2CP_STRING_BYTES + 1);
        assert!(matches!(
            Disconnect { reason: long }.encode(),
            Err(I2cpError::Malformed { .. })
        ));
    }

    #[test]
    fn host_lookup_key_shapes_round_trip() {
        let keys = vec![
            HostLookupKey::Hash(Hash::from_bytes([0x01; 32])),
            HostLookupKey::Hostname("example.i2p".to_owned()),
            HostLookupKey::HashWithOptions(Hash::from_bytes([0x02; 32])),
            HostLookupKey::HostnameWithOptions("service.i2p".to_owned()),
            HostLookupKey::DestinationWithOptions(test_destination()),
        ];
        for key in keys {
            round_trip(&Message::HostLookup(HostLookup {
                session: SessionId::NO_SESSION,
                request_id: HostRequestId::new(0xdead_beef),
                timeout_ms: 10_000,
                key,
            }));
        }
        // Unknown request type is unsupported, not silently parsed.
        let mut bad = vec![
            0xffu8, 0xff, 0xde, 0xad, 0xbe, 0xef, 0x00, 0x00, 0x27, 0x10, 0x09,
        ];
        assert!(matches!(
            HostLookup::decode(&bad),
            Err(I2cpError::Malformed { .. })
        ));
        bad.pop();
        assert!(matches!(
            HostLookup::decode(&bad),
            Err(I2cpError::Malformed { .. })
        ));
    }

    #[test]
    fn host_reply_shapes_round_trip() {
        round_trip(&Message::HostReply(HostReply {
            session: SessionId::new(6),
            request_id: HostRequestId::new(1),
            result: HostReplyResult::Success,
            destination: Some(test_destination()),
            options: None,
        }));
        round_trip(&Message::HostReply(HostReply {
            session: SessionId::new(6),
            request_id: HostRequestId::new(2),
            result: HostReplyResult::Failure,
            destination: None,
            options: None,
        }));
        let options = Mapping::from_entries(vec![("stats.d".to_owned(), "example.i2p".to_owned())])
            .expect("options");
        round_trip(&Message::HostReply(HostReply {
            session: SessionId::new(6),
            request_id: HostRequestId::new(3),
            result: HostReplyResult::Success,
            destination: Some(test_destination()),
            options: Some(options),
        }));
        for raw in 0..=7u8 {
            assert_ne!(
                HostReplyResult::from_u8(raw),
                HostReplyResult::Reserved(raw)
            );
        }
        assert_eq!(
            HostReplyResult::from_u8(200),
            HostReplyResult::Reserved(200)
        );
    }

    #[test]
    fn malformed_destination_rejected_structurally() {
        // Declares a 391-byte destination but truncates the bytes.
        let mut body = vec![0x00u8, 0x02];
        body.extend_from_slice(&[0x11u8; 100]);
        assert!(matches!(
            SendMessage::decode(&body),
            Err(I2cpError::Malformed { .. })
        ));
        // Garbage destination bytes fail decoding, not parsing.
        let mut garbage = vec![0x00u8, 0x02];
        garbage.extend_from_slice(&[0xffu8; 391]);
        assert!(matches!(
            SendMessage::decode(&garbage),
            Err(I2cpError::Malformed { .. })
        ));
    }

    #[test]
    fn oversized_string_prefix_rejected() {
        // Declares 255 bytes but supplies fewer.
        let body = vec![0xffu8, b'a'];
        assert!(matches!(
            Disconnect::decode(&body),
            Err(I2cpError::Malformed { .. })
        ));
    }
}
