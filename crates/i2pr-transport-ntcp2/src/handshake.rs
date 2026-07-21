//! Bounded NTCP2 handshake messages, policies, and authenticated payloads.
//!
//! The byte layouts in this module follow the pinned 0.9.69 NTCP2 dossier.
//! This layer is deliberately synchronous and side-effect free: it validates
//! bytes and policy inputs, but never owns a clock, replay store, socket, or
//! RouterInfo database.

#![allow(clippy::module_name_repetitions)]

use i2pr_crypto::{
    CryptoError, constant_time_eq, router_identity_hash, sha256, verify_router_info,
};
use i2pr_proto::{CodecError, Hash, RouterInfo};
use std::fmt;
use thiserror::Error;

use crate::constants;
use crate::crypto::{Ntcp2CryptoError, PublicKeyBytes};

/// The supported NTCP2 handshake protocol version.
pub const NTCP2_VERSION: u8 = 2;
/// The default I2P network identifier.
pub const DEFAULT_NETWORK_ID: u8 = 2;

/// A bounded, typed handshake failure category.
#[derive(Debug, Error, Eq, PartialEq)]
pub enum HandshakeError {
    /// The input ended before a complete field or message was available.
    #[error("truncated NTCP2 handshake message")]
    Truncated,
    /// A fixed-size region or complete message had an invalid size.
    #[error("invalid NTCP2 handshake length")]
    InvalidFixedLength,
    /// A peer-controlled padding length exceeded the selected bound.
    #[error("excessive NTCP2 handshake padding")]
    ExcessivePadding,
    /// An option field or reserved value was not valid for version 2.
    #[error("malformed NTCP2 handshake options")]
    MalformedOptions,
    /// A public ephemeral value could not be accepted.
    #[error("NTCP2 ephemeral key validation failed")]
    DeobfuscationFailure,
    /// An authenticated frame or RouterInfo was not authentic.
    #[error("NTCP2 handshake authentication failed")]
    AuthenticationFailure,
    /// The transcript did not accept the supplied message in its current stage.
    #[error("NTCP2 handshake transcript mismatch")]
    TranscriptMismatch,
    /// A key agreement was invalid, including an all-zero result.
    #[error("NTCP2 handshake key agreement failed")]
    InvalidKeyAgreement,
    /// The negotiated network identifier was not the local network.
    #[error("wrong NTCP2 network")]
    WrongNetwork,
    /// The peer timestamp is older than the accepted skew window.
    #[error("stale NTCP2 handshake timestamp")]
    StaleTimestamp,
    /// The peer timestamp is newer than the accepted skew window.
    #[error("future NTCP2 handshake timestamp")]
    FutureTimestamp,
    /// A replay decision rejected the bounded handshake token.
    #[error("replayed NTCP2 handshake")]
    ReplayDetected,
    /// The replay service could not make an admission decision.
    #[error("NTCP2 replay cache unavailable or full")]
    ReplayCacheUnavailable,
    /// The authenticated RouterIdentity did not match an expected peer.
    #[error("NTCP2 peer identity mismatch")]
    PeerIdentityMismatch,
    /// The NTCP2 static key did not match the authenticated RouterInfo.
    #[error("NTCP2 transport static-key mismatch")]
    TransportStaticKeyMismatch,
    /// The RouterInfo bytes were not a bounded complete structure.
    #[error("malformed RouterInfo in NTCP2 handshake")]
    RouterInfoMalformed,
    /// The RouterInfo signature was not valid for its signed region.
    #[error("RouterInfo signature invalid")]
    RouterInfoSignatureInvalid,
    /// The RouterInfo uses a key or signature type outside this implementation.
    #[error("unsupported peer key or signature type")]
    UnsupportedPeerKey,
    /// A public state transition was attempted in the wrong state.
    #[error("NTCP2 handshake state violation")]
    StateViolation,
    /// The runtime cancelled the handshake before authentication completed.
    #[error("NTCP2 handshake cancelled")]
    Cancelled,
    /// A runtime deadline expired before the next handshake action completed.
    #[error("NTCP2 handshake deadline expired")]
    DeadlineExpired,
    /// The transport disconnected before the handshake completed.
    #[error("NTCP2 handshake disconnected")]
    Disconnected,
    /// A local bound, source, or policy denied progress.
    #[error("local NTCP2 handshake policy or resource denial")]
    LocalPolicyDenied,
    /// A lower-level structural decoder rejected the input.
    #[error("NTCP2 handshake codec rejected input")]
    Codec(#[from] CodecError),
    /// A reviewed cryptographic wrapper rejected the operation.
    #[error("NTCP2 handshake cryptographic operation failed")]
    Crypto(#[from] Ntcp2CryptoError),
}

impl From<CryptoError> for HandshakeError {
    fn from(error: CryptoError) -> Self {
        match error {
            CryptoError::InvalidSignature => Self::RouterInfoSignatureInvalid,
            CryptoError::UnsupportedAlgorithm { .. } | CryptoError::InvalidKey { .. } => {
                Self::UnsupportedPeerKey
            }
            CryptoError::AllZeroSharedSecret => Self::InvalidKeyAgreement,
            CryptoError::Protocol(_) => Self::RouterInfoMalformed,
            CryptoError::RandomnessUnavailable => Self::LocalPolicyDenied,
        }
    }
}

fn take<const N: usize>(input: &[u8], offset: &mut usize) -> Result<[u8; N], HandshakeError> {
    let end = offset
        .checked_add(N)
        .ok_or(HandshakeError::InvalidFixedLength)?;
    let bytes = input.get(*offset..end).ok_or(HandshakeError::Truncated)?;
    *offset = end;
    bytes
        .try_into()
        .map_err(|_| HandshakeError::InvalidFixedLength)
}

fn put_u16(output: &mut Vec<u8>, value: u16) {
    output.extend_from_slice(&value.to_be_bytes());
}

fn get_u16(input: &[u8], offset: &mut usize) -> Result<u16, HandshakeError> {
    Ok(u16::from_be_bytes(take(input, offset)?))
}

fn get_u32(input: &[u8], offset: &mut usize) -> Result<u32, HandshakeError> {
    Ok(u32::from_be_bytes(take(input, offset)?))
}

fn checked_range<'a>(
    input: &'a [u8],
    offset: &mut usize,
    length: usize,
) -> Result<&'a [u8], HandshakeError> {
    let end = offset
        .checked_add(length)
        .ok_or(HandshakeError::InvalidFixedLength)?;
    let bytes = input.get(*offset..end).ok_or(HandshakeError::Truncated)?;
    *offset = end;
    Ok(bytes)
}

fn map_router_info_codec(error: CodecError) -> HandshakeError {
    match error {
        CodecError::Unsupported { .. } => HandshakeError::UnsupportedPeerKey,
        _ => HandshakeError::RouterInfoMalformed,
    }
}

/// A SessionRequest options block, all fields encoded big-endian.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SessionRequestOptions {
    /// Network identifier; nonzero values must match the local network.
    pub network_id: u8,
    /// NTCP2 protocol version.
    pub version: u8,
    /// Number of cleartext padding bytes after the AEAD frame.
    pub padding_length: u16,
    /// Exact SessionConfirmed part-two frame length, including its tag.
    pub session_confirmed_part2_length: u16,
    /// Unix timestamp in seconds.
    pub timestamp: u32,
}

impl SessionRequestOptions {
    /// Creates version-2 options after checking the representable frame size.
    pub fn new(
        network_id: u8,
        padding_length: usize,
        session_confirmed_part2_length: usize,
        timestamp: u64,
    ) -> Result<Self, HandshakeError> {
        let timestamp = u32::try_from(timestamp).map_err(|_| HandshakeError::MalformedOptions)?;
        let padding_length =
            u16::try_from(padding_length).map_err(|_| HandshakeError::ExcessivePadding)?;
        let frame_length = u16::try_from(session_confirmed_part2_length)
            .map_err(|_| HandshakeError::InvalidFixedLength)?;
        if !(constants::AUTH_TAG_LENGTH..=constants::MAX_SESSION_CONFIRMED_PART2)
            .contains(&session_confirmed_part2_length)
        {
            return Err(HandshakeError::InvalidFixedLength);
        }
        Ok(Self {
            network_id,
            version: NTCP2_VERSION,
            padding_length,
            session_confirmed_part2_length: frame_length,
            timestamp,
        })
    }

    /// Encodes the exact 16-byte wire block.
    pub fn encode(self) -> [u8; constants::OPTION_BLOCK_LENGTH] {
        let mut bytes = [0_u8; constants::OPTION_BLOCK_LENGTH];
        bytes[0] = self.network_id;
        bytes[1] = self.version;
        bytes[2..4].copy_from_slice(&self.padding_length.to_be_bytes());
        bytes[4..6].copy_from_slice(&self.session_confirmed_part2_length.to_be_bytes());
        bytes[8..12].copy_from_slice(&self.timestamp.to_be_bytes());
        bytes
    }

    /// Decodes and validates the reserved and version fields.
    pub fn decode(input: &[u8]) -> Result<Self, HandshakeError> {
        if input.len() != constants::OPTION_BLOCK_LENGTH {
            return Err(HandshakeError::InvalidFixedLength);
        }
        let mut offset = 0;
        let network_id = take::<1>(input, &mut offset)?[0];
        let version = take::<1>(input, &mut offset)?[0];
        let padding_length = get_u16(input, &mut offset)?;
        let session_confirmed_part2_length = get_u16(input, &mut offset)?;
        if get_u16(input, &mut offset)? != 0 {
            return Err(HandshakeError::MalformedOptions);
        }
        let timestamp = get_u32(input, &mut offset)?;
        if get_u32(input, &mut offset)? != 0 || version != NTCP2_VERSION {
            return Err(HandshakeError::MalformedOptions);
        }
        if !(constants::AUTH_TAG_LENGTH..=constants::MAX_SESSION_CONFIRMED_PART2)
            .contains(&(usize::from(session_confirmed_part2_length)))
        {
            return Err(HandshakeError::InvalidFixedLength);
        }
        Ok(Self {
            network_id,
            version,
            padding_length,
            session_confirmed_part2_length,
            timestamp,
        })
    }
}

/// A SessionCreated options block, all fields encoded big-endian.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SessionCreatedOptions {
    /// Number of cleartext padding bytes after the AEAD frame.
    pub padding_length: u16,
    /// Unix timestamp in seconds.
    pub timestamp: u32,
}

impl SessionCreatedOptions {
    /// Creates version-2-compatible options.
    pub fn new(padding_length: usize, timestamp: u64) -> Result<Self, HandshakeError> {
        Ok(Self {
            padding_length: u16::try_from(padding_length)
                .map_err(|_| HandshakeError::ExcessivePadding)?,
            timestamp: u32::try_from(timestamp).map_err(|_| HandshakeError::MalformedOptions)?,
        })
    }

    /// Encodes the exact 16-byte wire block.
    pub fn encode(self) -> [u8; constants::OPTION_BLOCK_LENGTH] {
        let mut bytes = [0_u8; constants::OPTION_BLOCK_LENGTH];
        bytes[2..4].copy_from_slice(&self.padding_length.to_be_bytes());
        bytes[8..12].copy_from_slice(&self.timestamp.to_be_bytes());
        bytes
    }

    /// Decodes and validates every reserved field.
    pub fn decode(input: &[u8]) -> Result<Self, HandshakeError> {
        if input.len() != constants::OPTION_BLOCK_LENGTH {
            return Err(HandshakeError::InvalidFixedLength);
        }
        let mut offset = 0;
        if get_u16(input, &mut offset)? != 0 {
            return Err(HandshakeError::MalformedOptions);
        }
        let padding_length = get_u16(input, &mut offset)?;
        if get_u16(input, &mut offset)? != 0 {
            return Err(HandshakeError::MalformedOptions);
        }
        if get_u16(input, &mut offset)? != 0 {
            return Err(HandshakeError::MalformedOptions);
        }
        let timestamp = get_u32(input, &mut offset)?;
        if get_u32(input, &mut offset)? != 0 {
            return Err(HandshakeError::MalformedOptions);
        }
        Ok(Self {
            padding_length,
            timestamp,
        })
    }
}

/// The first NTCP2 handshake message, with its authenticated padding retained.
#[derive(Clone, Eq, PartialEq)]
pub struct SessionRequest {
    encrypted_ephemeral: [u8; constants::KEY_LENGTH],
    encrypted_options: Vec<u8>,
    padding: Vec<u8>,
}

impl fmt::Debug for SessionRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SessionRequest")
            .field("length", &self.encoded_len())
            .field("padding_length", &self.padding.len())
            .finish()
    }
}

impl SessionRequest {
    /// Creates a message after enforcing fixed ciphertext and padding bounds.
    pub fn new(
        encrypted_ephemeral: [u8; constants::KEY_LENGTH],
        encrypted_options: Vec<u8>,
        padding: Vec<u8>,
    ) -> Result<Self, HandshakeError> {
        if encrypted_options.len() != constants::HANDSHAKE_OPTIONS_FRAME_LENGTH {
            return Err(HandshakeError::InvalidFixedLength);
        }
        if padding.len() > constants::MAX_SESSION_REQUEST_PADDING {
            return Err(HandshakeError::ExcessivePadding);
        }
        let message = Self {
            encrypted_ephemeral,
            encrypted_options,
            padding,
        };
        if message.encoded_len() > constants::MAX_HANDSHAKE_MESSAGE_LENGTH {
            return Err(HandshakeError::InvalidFixedLength);
        }
        Ok(message)
    }

    /// Decodes one complete SessionRequest and rejects trailing bytes.
    pub fn decode(input: &[u8], maximum: usize) -> Result<Self, HandshakeError> {
        if input.len() < constants::MIN_HANDSHAKE_MESSAGE_LENGTH {
            return Err(HandshakeError::Truncated);
        }
        if input.len() > maximum || input.len() > constants::MAX_HANDSHAKE_MESSAGE_LENGTH {
            return Err(HandshakeError::InvalidFixedLength);
        }
        let mut offset = 0;
        let encrypted_ephemeral = take(input, &mut offset)?;
        let encrypted_options = checked_range(
            input,
            &mut offset,
            constants::HANDSHAKE_OPTIONS_FRAME_LENGTH,
        )?
        .to_vec();
        let padding = input
            .get(offset..)
            .ok_or(HandshakeError::Truncated)?
            .to_vec();
        Self::new(encrypted_ephemeral, encrypted_options, padding)
    }

    /// Encodes the exact message bytes.
    pub fn encode(&self) -> Vec<u8> {
        let mut output = Vec::with_capacity(self.encoded_len());
        output.extend_from_slice(&self.encrypted_ephemeral);
        output.extend_from_slice(&self.encrypted_options);
        output.extend_from_slice(&self.padding);
        output
    }

    /// Returns the complete encoded length without allocating.
    pub fn encoded_len(&self) -> usize {
        constants::HANDSHAKE_EPHEMERAL_LENGTH + self.encrypted_options.len() + self.padding.len()
    }

    /// Returns the obfuscated ephemeral bytes used by the AES state.
    pub const fn encrypted_ephemeral(&self) -> &[u8; constants::KEY_LENGTH] {
        &self.encrypted_ephemeral
    }

    /// Returns the encrypted 16-byte options frame including its tag.
    pub fn encrypted_options(&self) -> &[u8] {
        &self.encrypted_options
    }

    /// Returns the cleartext padding bytes.
    pub fn padding(&self) -> &[u8] {
        &self.padding
    }
}

/// The second NTCP2 handshake message, with its authenticated padding retained.
#[derive(Clone, Eq, PartialEq)]
pub struct SessionCreated {
    encrypted_ephemeral: [u8; constants::KEY_LENGTH],
    encrypted_options: Vec<u8>,
    padding: Vec<u8>,
}

impl fmt::Debug for SessionCreated {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SessionCreated")
            .field("length", &self.encoded_len())
            .field("padding_length", &self.padding.len())
            .finish()
    }
}

impl SessionCreated {
    /// Creates a message after enforcing fixed ciphertext and padding bounds.
    pub fn new(
        encrypted_ephemeral: [u8; constants::KEY_LENGTH],
        encrypted_options: Vec<u8>,
        padding: Vec<u8>,
    ) -> Result<Self, HandshakeError> {
        if encrypted_options.len() != constants::HANDSHAKE_OPTIONS_FRAME_LENGTH {
            return Err(HandshakeError::InvalidFixedLength);
        }
        if padding.len() > constants::MAX_SESSION_CREATED_PADDING {
            return Err(HandshakeError::ExcessivePadding);
        }
        let message = Self {
            encrypted_ephemeral,
            encrypted_options,
            padding,
        };
        if message.encoded_len() > constants::MAX_HANDSHAKE_MESSAGE_LENGTH {
            return Err(HandshakeError::InvalidFixedLength);
        }
        Ok(message)
    }

    /// Decodes one complete SessionCreated and rejects trailing bytes.
    pub fn decode(input: &[u8], maximum: usize) -> Result<Self, HandshakeError> {
        if input.len() < constants::MIN_HANDSHAKE_MESSAGE_LENGTH {
            return Err(HandshakeError::Truncated);
        }
        if input.len() > maximum || input.len() > constants::MAX_HANDSHAKE_MESSAGE_LENGTH {
            return Err(HandshakeError::InvalidFixedLength);
        }
        let mut offset = 0;
        let encrypted_ephemeral = take(input, &mut offset)?;
        let encrypted_options = checked_range(
            input,
            &mut offset,
            constants::HANDSHAKE_OPTIONS_FRAME_LENGTH,
        )?
        .to_vec();
        let padding = input
            .get(offset..)
            .ok_or(HandshakeError::Truncated)?
            .to_vec();
        Self::new(encrypted_ephemeral, encrypted_options, padding)
    }

    /// Encodes the exact message bytes.
    pub fn encode(&self) -> Vec<u8> {
        let mut output = Vec::with_capacity(self.encoded_len());
        output.extend_from_slice(&self.encrypted_ephemeral);
        output.extend_from_slice(&self.encrypted_options);
        output.extend_from_slice(&self.padding);
        output
    }

    /// Returns the complete encoded length without allocating.
    pub fn encoded_len(&self) -> usize {
        constants::HANDSHAKE_EPHEMERAL_LENGTH + self.encrypted_options.len() + self.padding.len()
    }

    /// Returns the obfuscated ephemeral bytes used by the AES state.
    pub const fn encrypted_ephemeral(&self) -> &[u8; constants::KEY_LENGTH] {
        &self.encrypted_ephemeral
    }

    /// Returns the encrypted 16-byte options frame including its tag.
    pub fn encrypted_options(&self) -> &[u8] {
        &self.encrypted_options
    }

    /// Returns the cleartext padding bytes.
    pub fn padding(&self) -> &[u8] {
        &self.padding
    }
}

/// The third NTCP2 handshake message containing the two authenticated frames.
#[derive(Clone, Eq, PartialEq)]
pub struct SessionConfirmed {
    static_frame: Vec<u8>,
    payload_frame: Vec<u8>,
}

impl fmt::Debug for SessionConfirmed {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SessionConfirmed")
            .field("part1_length", &self.static_frame.len())
            .field("part2_length", &self.payload_frame.len())
            .finish()
    }
}

impl SessionConfirmed {
    /// Creates message 3 after checking both fixed and negotiated lengths.
    pub fn new(static_frame: Vec<u8>, payload_frame: Vec<u8>) -> Result<Self, HandshakeError> {
        if static_frame.len() != constants::SESSION_CONFIRMED_PART1_LENGTH
            || !(constants::AUTH_TAG_LENGTH..=constants::MAX_SESSION_CONFIRMED_PART2)
                .contains(&payload_frame.len())
            || static_frame
                .len()
                .checked_add(payload_frame.len())
                .ok_or(HandshakeError::InvalidFixedLength)?
                > constants::MAX_SESSION_CONFIRMED_LENGTH
        {
            return Err(HandshakeError::InvalidFixedLength);
        }
        Ok(Self {
            static_frame,
            payload_frame,
        })
    }

    /// Decodes a complete message 3 with the expected part-two length.
    pub fn decode(
        input: &[u8],
        expected_part2_length: usize,
        maximum: usize,
    ) -> Result<Self, HandshakeError> {
        if input.len() > maximum || input.len() > constants::MAX_SESSION_CONFIRMED_LENGTH {
            return Err(HandshakeError::InvalidFixedLength);
        }
        let expected = constants::SESSION_CONFIRMED_PART1_LENGTH
            .checked_add(expected_part2_length)
            .ok_or(HandshakeError::InvalidFixedLength)?;
        if input.len() != expected {
            return Err(if input.len() < expected {
                HandshakeError::Truncated
            } else {
                HandshakeError::InvalidFixedLength
            });
        }
        let static_frame = input[..constants::SESSION_CONFIRMED_PART1_LENGTH].to_vec();
        let payload_frame = input[constants::SESSION_CONFIRMED_PART1_LENGTH..].to_vec();
        Self::new(static_frame, payload_frame)
    }

    /// Encodes the two frames without a data-phase length prefix.
    pub fn encode(&self) -> Vec<u8> {
        let mut output = Vec::with_capacity(self.encoded_len());
        output.extend_from_slice(&self.static_frame);
        output.extend_from_slice(&self.payload_frame);
        output
    }

    /// Returns the encoded message length.
    pub fn encoded_len(&self) -> usize {
        self.static_frame.len() + self.payload_frame.len()
    }

    /// Borrows the fixed first frame.
    pub fn static_frame(&self) -> &[u8] {
        &self.static_frame
    }

    /// Borrows the negotiated second frame.
    pub fn payload_frame(&self) -> &[u8] {
        &self.payload_frame
    }
}

/// A strict RouterInfo/options/padding payload for SessionConfirmed part two.
#[derive(Clone, Eq, PartialEq)]
pub struct ConfirmedPayload {
    router_info: Vec<u8>,
    options: Option<Vec<u8>>,
    padding: Option<Vec<u8>>,
}

impl fmt::Debug for ConfirmedPayload {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ConfirmedPayload")
            .field("router_info_length", &self.router_info.len())
            .field("options_length", &self.options.as_ref().map(Vec::len))
            .field("padding_length", &self.padding.as_ref().map(Vec::len))
            .finish()
    }
}

impl ConfirmedPayload {
    /// Creates a bounded message-3 payload.
    pub fn new(
        router_info: Vec<u8>,
        options: Option<Vec<u8>>,
        padding: Option<Vec<u8>>,
    ) -> Result<Self, HandshakeError> {
        if router_info.is_empty() || router_info.len() > constants::MAX_ROUTER_INFO_PAYLOAD {
            return Err(HandshakeError::RouterInfoMalformed);
        }
        if options
            .as_ref()
            .is_some_and(|value| value.len() > constants::MAX_CONFIRMED_OPTIONS)
            || padding
                .as_ref()
                .is_some_and(|value| value.len() > constants::MAX_SESSION_CONFIRMED_PART2_PLAINTEXT)
        {
            return Err(HandshakeError::ExcessivePadding);
        }
        let payload = Self {
            router_info,
            options,
            padding,
        };
        if payload.encoded_len() > constants::MAX_SESSION_CONFIRMED_PART2_PLAINTEXT {
            return Err(HandshakeError::InvalidFixedLength);
        }
        Ok(payload)
    }

    /// Decodes exactly the permitted ordered block sequence.
    pub fn decode(input: &[u8], maximum_router_info: usize) -> Result<Self, HandshakeError> {
        if input.is_empty() || input.len() > constants::MAX_SESSION_CONFIRMED_PART2_PLAINTEXT {
            return Err(HandshakeError::InvalidFixedLength);
        }
        let mut offset = 0;
        let router_type = take::<1>(input, &mut offset)?[0];
        let router_length = usize::from(get_u16(input, &mut offset)?);
        if router_type != 2 || router_length < 1 {
            return Err(HandshakeError::MalformedOptions);
        }
        let router_block = checked_range(input, &mut offset, router_length)?;
        if router_block[0] & 0xfe != 0 {
            return Err(HandshakeError::MalformedOptions);
        }
        let router_info = router_block[1..].to_vec();
        if router_info.is_empty()
            || router_info.len() > maximum_router_info
            || router_info.len() > constants::MAX_ROUTER_INFO_PAYLOAD
        {
            return Err(HandshakeError::RouterInfoMalformed);
        }
        let mut options = None;
        let mut padding = None;
        while offset < input.len() {
            let block_type = take::<1>(input, &mut offset)?[0];
            let block_length = usize::from(get_u16(input, &mut offset)?);
            let block = checked_range(input, &mut offset, block_length)?.to_vec();
            match block_type {
                1 => {
                    if options.is_some()
                        || padding.is_some()
                        || block.len() < 12
                        || block.len() > constants::MAX_CONFIRMED_OPTIONS
                    {
                        return Err(HandshakeError::MalformedOptions);
                    }
                    options = Some(block);
                }
                254 => {
                    if padding.is_some() {
                        return Err(HandshakeError::MalformedOptions);
                    }
                    padding = Some(block);
                }
                _ => return Err(HandshakeError::MalformedOptions),
            }
        }
        Self::new(router_info, options, padding)
    }

    /// Encodes RouterInfo, optional Options, then optional Padding blocks.
    pub fn encode(&self) -> Vec<u8> {
        let mut output = Vec::with_capacity(self.encoded_len());
        output.push(2);
        put_u16(
            &mut output,
            u16::try_from(self.router_info.len() + 1).expect("bounded RI"),
        );
        output.push(0);
        output.extend_from_slice(&self.router_info);
        if let Some(options) = &self.options {
            output.push(1);
            put_u16(
                &mut output,
                u16::try_from(options.len()).expect("bounded options"),
            );
            output.extend_from_slice(options);
        }
        if let Some(padding) = &self.padding {
            output.push(254);
            put_u16(
                &mut output,
                u16::try_from(padding.len()).expect("bounded padding"),
            );
            output.extend_from_slice(padding);
        }
        output
    }

    /// Returns the plaintext length before the AEAD authentication tag.
    pub fn encoded_len(&self) -> usize {
        3 + 1
            + self.router_info.len()
            + self.options.as_ref().map_or(0, |value| 3 + value.len())
            + self.padding.as_ref().map_or(0, |value| 3 + value.len())
    }

    /// Borrows the complete RouterInfo bytes.
    pub fn router_info(&self) -> &[u8] {
        &self.router_info
    }

    /// Borrows the optional negotiated options bytes.
    pub fn options(&self) -> Option<&[u8]> {
        self.options.as_deref()
    }

    /// Borrows optional in-frame padding.
    pub fn padding(&self) -> Option<&[u8]> {
        self.padding.as_deref()
    }
}

/// An explicit timestamp skew policy, with seconds in the same domain as NTCP2.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClockSkewPolicy {
    maximum_delta: u64,
    replay_retention: u64,
}

impl ClockSkewPolicy {
    /// Creates a policy; replay retention must be at least twice the skew.
    pub const fn new(maximum_delta: u64, replay_retention: u64) -> Option<Self> {
        if maximum_delta == 0 || replay_retention < maximum_delta.saturating_mul(2) {
            None
        } else {
            Some(Self {
                maximum_delta,
                replay_retention,
            })
        }
    }

    /// The pinned compatibility default: ±60 seconds and 2-minute replay retention.
    pub const fn default_compatibility() -> Self {
        Self {
            maximum_delta: 60,
            replay_retention: 120,
        }
    }

    /// Returns the accepted skew window in seconds.
    pub const fn maximum_delta(self) -> u64 {
        self.maximum_delta
    }

    /// Returns the minimum replay retention in seconds.
    pub const fn replay_retention(self) -> u64 {
        self.replay_retention
    }

    /// Classifies a peer timestamp without using wall-clock APIs.
    pub fn classify(self, local: u64, peer: u32) -> Result<(), HandshakeError> {
        let delta = i128::from(peer) - i128::from(local);
        if delta < -i128::from(self.maximum_delta) {
            Err(HandshakeError::StaleTimestamp)
        } else if delta > i128::from(self.maximum_delta) {
            Err(HandshakeError::FutureTimestamp)
        } else {
            Ok(())
        }
    }
}

/// A fixed-size replay token with redacted diagnostics.
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct ReplayToken([u8; constants::HASH_LENGTH]);

impl ReplayToken {
    /// Derives a token from the encrypted ephemeral field.
    pub fn from_ephemeral_bytes(bytes: &[u8]) -> Self {
        Self(*sha256(bytes).as_bytes())
    }

    /// Borrows the exact cache key bytes for an injected cache.
    pub const fn as_bytes(&self) -> &[u8; constants::HASH_LENGTH] {
        &self.0
    }
}

impl fmt::Debug for ReplayToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ReplayToken(<redacted>)")
    }
}

/// A replay-cache admission result supplied by the runtime adapter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReplayDecision {
    /// The token was not present and has been reserved for this handshake.
    Fresh,
    /// The token was already observed within the retention window.
    Replayed,
    /// The bounded cache has no admission capacity.
    CacheFull,
    /// The cache could not make a decision.
    Unavailable,
}

/// A deterministic bounded replay cache for local tests and reference adapters.
pub struct ReferenceReplayCache {
    retention: u64,
    maximum_entries: usize,
    entries: Vec<(ReplayToken, u64)>,
}

impl ReferenceReplayCache {
    /// Creates an empty cache with an explicit nonzero capacity.
    pub fn new(maximum_entries: usize, retention: u64) -> Result<Self, HandshakeError> {
        if maximum_entries == 0
            || retention == 0
            || maximum_entries > constants::MAX_HANDSHAKE_ACTIONS * 32
        {
            return Err(HandshakeError::LocalPolicyDenied);
        }
        Ok(Self {
            retention,
            maximum_entries,
            entries: Vec::new(),
        })
    }

    /// Checks and reserves a token, expiring entries deterministically first.
    pub fn check_and_record(&mut self, token: ReplayToken, now: u64) -> ReplayDecision {
        self.entries
            .retain(|(_, seen)| now.saturating_sub(*seen) < self.retention);
        if self.entries.iter().any(|(known, _)| *known == token) {
            return ReplayDecision::Replayed;
        }
        if self.entries.len() >= self.maximum_entries {
            return ReplayDecision::CacheFull;
        }
        self.entries.push((token, now));
        ReplayDecision::Fresh
    }

    /// Returns the current bounded entry count.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns whether the cache has no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// The authenticated identity and NTCP2 static key discovered in RouterInfo.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AuthenticatedPeer {
    /// Canonical RouterIdentity hash.
    pub router_hash: Hash,
    /// Published NTCP2 static public key.
    pub transport_static_key: PublicKeyBytes,
}

/// Validates a complete RouterInfo and binds its NTCP2 static key.
pub fn validate_router_info(
    bytes: &[u8],
    maximum: usize,
    expected_hash: Option<Hash>,
    expected_static_key: PublicKeyBytes,
) -> Result<AuthenticatedPeer, HandshakeError> {
    if bytes.is_empty() || bytes.len() > maximum || bytes.len() > constants::MAX_ROUTER_INFO_PAYLOAD
    {
        return Err(HandshakeError::RouterInfoMalformed);
    }
    let info = RouterInfo::decode(bytes, maximum).map_err(map_router_info_codec)?;
    verify_router_info(&info).map_err(HandshakeError::from)?;
    let router_hash = router_identity_hash(info.router_identity()).map_err(HandshakeError::from)?;
    if expected_hash.is_some_and(|expected| expected != router_hash) {
        return Err(HandshakeError::PeerIdentityMismatch);
    }
    if info.router_identity().public_key().key_type() != i2pr_proto::CryptoKeyType::X25519 {
        return Err(HandshakeError::UnsupportedPeerKey);
    }
    let mut found = None;
    for address in info.addresses() {
        if !matches!(address.transport_style(), "NTCP" | "NTCP2") {
            continue;
        }
        let Some(version) = address.options().get("v") else {
            continue;
        };
        if !version.split(',').any(|value| value == "2") {
            continue;
        }
        let Some(encoded_key) = address.options().get("s") else {
            continue;
        };
        let key = decode_i2p_base64(encoded_key, constants::KEY_LENGTH)
            .map_err(|_| HandshakeError::TransportStaticKeyMismatch)?;
        let key: [u8; constants::KEY_LENGTH] = key
            .try_into()
            .map_err(|_| HandshakeError::TransportStaticKeyMismatch)?;
        let key =
            PublicKeyBytes::new(key).map_err(|_| HandshakeError::TransportStaticKeyMismatch)?;
        if found.is_some_and(|old: PublicKeyBytes| old != key) {
            return Err(HandshakeError::TransportStaticKeyMismatch);
        }
        found = Some(key);
    }
    let found = found.ok_or(HandshakeError::TransportStaticKeyMismatch)?;
    if !constant_time_eq(found.as_bytes(), expected_static_key.as_bytes()) {
        return Err(HandshakeError::TransportStaticKeyMismatch);
    }
    Ok(AuthenticatedPeer {
        router_hash,
        transport_static_key: found,
    })
}

fn decode_i2p_base64(value: &str, expected_length: usize) -> Result<Vec<u8>, ()> {
    if expected_length == 0 {
        return Err(());
    }
    let remainder = expected_length % 3;
    let unpadded_length = (expected_length / 3) * 4
        + match remainder {
            0 => 0,
            1 => 2,
            _ => 3,
        };
    let padded_length = unpadded_length + usize::from(remainder != 0);
    let padding_length = value.bytes().skip_while(|byte| *byte != b'=').count();
    let unpadded = padding_length == 0 && value.len() == unpadded_length;
    let padded = value.len() == padded_length && padding_length == usize::from(remainder != 0);
    if !unpadded && !padded {
        return Err(());
    }
    if padding_length > 0 && !value.bytes().skip(unpadded_length).all(|byte| byte == b'=') {
        return Err(());
    }
    let mut output = Vec::with_capacity(expected_length);
    let mut accumulator = 0_u32;
    let mut bits = 0_u8;
    for byte in value.bytes().take(unpadded_length) {
        let digit = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'-' | b'+' => 62,
            b'~' | b'/' => 63,
            _ => return Err(()),
        };
        accumulator = (accumulator << 6) | u32::from(digit);
        bits = bits.saturating_add(6);
        if bits >= 8 {
            bits -= 8;
            output.push(((accumulator >> bits) & 0xff) as u8);
            if output.len() > expected_length {
                return Err(());
            }
        }
    }
    if output.len() != expected_length || (bits > 0 && accumulator & ((1 << bits) - 1) != 0) {
        return Err(());
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b64(bytes: &[u8]) -> String {
        const ALPHABET: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-~";
        let mut output = String::new();
        let mut index = 0;
        while index < bytes.len() {
            let left = bytes.len() - index;
            let a = bytes[index];
            let b = if left > 1 { bytes[index + 1] } else { 0 };
            let c = if left > 2 { bytes[index + 2] } else { 0 };
            output.push(ALPHABET[(a >> 2) as usize] as char);
            output.push(ALPHABET[((a & 3) << 4 | b >> 4) as usize] as char);
            if left > 1 {
                output.push(ALPHABET[((b & 15) << 2 | c >> 6) as usize] as char);
            } else {
                output.push('=');
            }
            if left > 2 {
                output.push(ALPHABET[(c & 63) as usize] as char);
            } else {
                output.push('=');
            }
            index += 3;
        }
        output
    }

    #[test]
    fn options_are_exact_and_reserved_bytes_are_rejected() {
        let options = SessionRequestOptions::new(2, 7, 64, 100).expect("options");
        assert_eq!(
            SessionRequestOptions::decode(&options.encode()),
            Ok(options)
        );
        let mut changed = options.encode();
        changed[15] = 1;
        assert_eq!(
            SessionRequestOptions::decode(&changed),
            Err(HandshakeError::MalformedOptions)
        );
    }

    #[test]
    fn message_codecs_reject_truncation_and_preserve_padding() {
        let request = SessionRequest::new([1; 32], vec![2; 32], vec![3; 9]).expect("request");
        let encoded = request.encode();
        assert_eq!(
            SessionRequest::decode(&encoded, 1_000).expect("decode"),
            request
        );
        assert_eq!(
            SessionRequest::decode(&encoded[..63], 1_000),
            Err(HandshakeError::Truncated)
        );
        let created = SessionCreated::new([4; 32], vec![5; 32], vec![6; 8]).expect("created");
        assert_eq!(
            SessionCreated::decode(&created.encode(), 1_000).expect("decode"),
            created
        );
    }

    #[test]
    fn message_bounds_accept_limits_and_reject_limit_plus_one() {
        let request = SessionRequest::new(
            [1; 32],
            vec![2; 32],
            vec![3; constants::MAX_SESSION_REQUEST_PADDING],
        )
        .expect("maximum request padding");
        assert_eq!(
            SessionRequest::decode(&request.encode(), constants::MAX_HANDSHAKE_MESSAGE_LENGTH)
                .expect("request limit"),
            request
        );
        assert_eq!(
            SessionRequest::new(
                [1; 32],
                vec![2; 32],
                vec![3; constants::MAX_SESSION_REQUEST_PADDING + 1]
            ),
            Err(HandshakeError::ExcessivePadding)
        );

        let created = SessionCreated::new(
            [4; 32],
            vec![5; 32],
            vec![6; constants::MAX_SESSION_CREATED_PADDING],
        )
        .expect("maximum created padding");
        assert_eq!(
            SessionCreated::decode(&created.encode(), constants::MAX_HANDSHAKE_MESSAGE_LENGTH)
                .expect("created limit"),
            created
        );
        assert_eq!(
            SessionCreated::new(
                [4; 32],
                vec![5; 32],
                vec![6; constants::MAX_SESSION_CREATED_PADDING + 1]
            ),
            Err(HandshakeError::ExcessivePadding)
        );

        let confirmed = SessionConfirmed::new(
            vec![8; constants::SESSION_CONFIRMED_PART1_LENGTH],
            vec![9; constants::MAX_SESSION_CONFIRMED_PART2],
        )
        .expect("maximum confirmed");
        assert_eq!(
            confirmed.encoded_len(),
            constants::MAX_SESSION_CONFIRMED_LENGTH
        );
        assert_eq!(
            SessionConfirmed::new(
                vec![8; constants::SESSION_CONFIRMED_PART1_LENGTH],
                vec![9; constants::MAX_SESSION_CONFIRMED_PART2 + 1]
            ),
            Err(HandshakeError::InvalidFixedLength)
        );
    }

    #[test]
    fn confirmed_payload_requires_ordered_known_blocks() {
        let payload = ConfirmedPayload::new(vec![9; 12], Some(vec![1; 12]), Some(vec![2; 3]))
            .expect("payload");
        assert_eq!(payload.encoded_len(), payload.encode().len());
        assert_eq!(
            ConfirmedPayload::decode(&payload.encode(), 100).expect("decode"),
            payload
        );
        let mut malformed = payload.encode();
        malformed[0] = 254;
        assert_eq!(
            ConfirmedPayload::decode(&malformed, 100),
            Err(HandshakeError::MalformedOptions)
        );
    }

    #[test]
    fn replay_and_clock_boundaries_are_deterministic() {
        let policy = ClockSkewPolicy::default_compatibility();
        assert!(policy.classify(100, 40).is_ok());
        assert_eq!(
            policy.classify(100, 39),
            Err(HandshakeError::StaleTimestamp)
        );
        assert_eq!(
            policy.classify(100, 161),
            Err(HandshakeError::FutureTimestamp)
        );
        let token = ReplayToken::from_ephemeral_bytes(b"token");
        let mut cache = ReferenceReplayCache::new(1, policy.replay_retention()).expect("cache");
        assert_eq!(cache.check_and_record(token, 1), ReplayDecision::Fresh);
        assert_eq!(cache.check_and_record(token, 2), ReplayDecision::Replayed);
        assert_eq!(cache.len(), 1);
        let other = ReplayToken::from_ephemeral_bytes(b"other");
        assert_eq!(cache.check_and_record(other, 3), ReplayDecision::CacheFull);
        assert_eq!(
            cache.check_and_record(other, 2 + policy.replay_retention()),
            ReplayDecision::Fresh
        );
        assert!(ReferenceReplayCache::new(1, 0).is_err());
    }

    #[test]
    fn router_info_static_key_is_base64_decoded_without_exposing_bytes() {
        let key = [7_u8; 32];
        assert_eq!(decode_i2p_base64(&b64(&key), 32).expect("base64"), key);
        let padded = b64(&key);
        assert_eq!(
            decode_i2p_base64(padded.trim_end_matches('='), 32).expect("unpadded base64"),
            key
        );
        assert!(decode_i2p_base64("=bad", 32).is_err());
        assert!(decode_i2p_base64(&format!("{padded}="), 32).is_err());
        assert!(format!("{:?}", ReplayToken::from_ephemeral_bytes(b"x")).contains("redacted"));
    }
}
