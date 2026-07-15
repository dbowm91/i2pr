//! Runtime-neutral NTCP2 data frames and directional state owners.
//!
//! A frame is length-obfuscated before it is sent, but the length is always
//! validated before the ciphertext is admitted.  A receive owner authenticates
//! the complete ciphertext before exposing its plaintext to the block parser.
//! No method in this module waits, opens a socket, or owns a runtime task.

use std::fmt;

use thiserror::Error;

use crate::block::{
    Block, BlockError, DecodedBlock, MAX_PADDING_BYTES, ParsedBlocks, TerminationBlock,
    encode_blocks, parse_blocks,
};
use crate::constants;
use crate::crypto::{CipherState, Ntcp2CryptoError, SipHashState, SplitKeys};

/// The minimum encrypted data-phase frame length, consisting of its tag.
pub const MIN_FRAME_LENGTH: usize = constants::AUTH_TAG_LENGTH;
/// The maximum authenticated plaintext block sequence length.
pub const MAX_PLAINTEXT_LENGTH: usize = constants::MAX_FRAME_PLAINTEXT;
/// The total wire overhead of an obfuscated length and an AEAD tag.
pub const FRAME_OVERHEAD: usize = 2 + constants::AUTH_TAG_LENGTH;

/// Typed data-phase frame failures. Variants contain only bounded metadata.
#[derive(Debug, Error, Eq, PartialEq)]
pub enum FrameError {
    /// The 2-byte length prefix was incomplete.
    #[error("truncated NTCP2 frame length")]
    TruncatedLength,
    /// The deobfuscated length was outside 16..=65535.
    #[error("invalid NTCP2 frame length")]
    InvalidLength,
    /// The supplied ciphertext did not match the authenticated length.
    #[error("truncated NTCP2 ciphertext frame")]
    TruncatedCiphertext,
    /// The authenticated ciphertext tag did not verify.
    #[error("NTCP2 data-frame authentication failed")]
    AuthenticationFailure,
    /// The nonce/counter cannot be advanced without emitting the forbidden value.
    #[error("NTCP2 data-frame counter exhausted")]
    CounterExhausted,
    /// A state owner was used after terminal transition or before its input.
    #[error("NTCP2 data-frame state violation")]
    StateViolation,
    /// A caller supplied a frame or plaintext outside its bound.
    #[error("NTCP2 data-frame payload exceeds its bound")]
    PayloadTooLarge,
    /// Authenticated plaintext blocks were malformed.
    #[error("NTCP2 authenticated block parse failed")]
    Blocks(#[source] BlockError),
    /// A reviewed primitive rejected a bounded operation.
    #[error("NTCP2 data-frame cryptographic operation failed")]
    Crypto(#[source] Ntcp2CryptoError),
}

impl From<BlockError> for FrameError {
    fn from(error: BlockError) -> Self {
        Self::Blocks(error)
    }
}

fn map_crypto(error: Ntcp2CryptoError) -> FrameError {
    match error {
        Ntcp2CryptoError::AuthenticationFailed => FrameError::AuthenticationFailure,
        Ntcp2CryptoError::NonceExhausted => FrameError::CounterExhausted,
        Ntcp2CryptoError::FieldTooLarge => FrameError::PayloadTooLarge,
        Ntcp2CryptoError::FrameLengthOutOfRange { .. } => FrameError::InvalidLength,
        other => FrameError::Crypto(other),
    }
}

/// An owned, bounded wire frame. Its Debug output does not contain bytes.
pub struct EncodedFrame {
    bytes: Vec<u8>,
}

impl EncodedFrame {
    fn new(bytes: Vec<u8>) -> Result<Self, FrameError> {
        if !(2 + MIN_FRAME_LENGTH..=constants::MAX_WIRE_FRAME_LENGTH).contains(&bytes.len()) {
            return Err(FrameError::PayloadTooLarge);
        }
        Ok(Self { bytes })
    }

    /// Borrows the complete length-prefixed frame.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Returns the complete wire length.
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    /// Returns whether the frame is empty; valid frames are never empty.
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Transfers ownership to the runtime adapter.
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

impl fmt::Debug for EncodedFrame {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EncodedFrame")
            .field("length", &self.bytes.len())
            .finish()
    }
}

/// The clear length recovered from one obfuscated prefix.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrameLength {
    /// Number of ciphertext bytes including the AEAD tag.
    pub ciphertext_length: usize,
    /// Number of authenticated plaintext bytes after opening.
    pub plaintext_length: usize,
}

impl FrameLength {
    fn from_ciphertext_length(length: u16) -> Result<Self, FrameError> {
        let ciphertext_length = usize::from(length);
        if !(MIN_FRAME_LENGTH..=constants::MAX_FRAME_LENGTH).contains(&ciphertext_length) {
            return Err(FrameError::InvalidLength);
        }
        Ok(Self {
            ciphertext_length,
            plaintext_length: ciphertext_length - constants::AUTH_TAG_LENGTH,
        })
    }
}

/// Pure inputs to the outbound padding/coalescing decision.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrameAssemblyPolicy {
    /// Maximum encrypted frame length including the AEAD tag.
    pub maximum_frame_length: usize,
    /// Minimum desired padding bytes selected by the caller's policy.
    pub minimum_padding: usize,
    /// Maximum desired padding bytes selected by the caller's policy.
    pub maximum_padding: usize,
    /// Deterministic padding choice for this assembly call.
    pub selected_padding: usize,
    /// Whether the runtime may coalesce candidates for this call.
    pub coalescing_allowed: bool,
}

impl FrameAssemblyPolicy {
    /// Creates a bounded pure assembly decision.
    pub fn new(
        maximum_frame_length: usize,
        minimum_padding: usize,
        maximum_padding: usize,
        selected_padding: usize,
        coalescing_allowed: bool,
    ) -> Result<Self, FrameError> {
        if !(MIN_FRAME_LENGTH..=constants::MAX_FRAME_LENGTH).contains(&maximum_frame_length)
            || minimum_padding > maximum_padding
            || selected_padding < minimum_padding
            || selected_padding > maximum_padding
            || maximum_padding > MAX_PADDING_BYTES
        {
            return Err(FrameError::PayloadTooLarge);
        }
        Ok(Self {
            maximum_frame_length,
            minimum_padding,
            maximum_padding,
            selected_padding,
            coalescing_allowed,
        })
    }

    /// Returns whether the caller allowed deterministic coalescing.
    pub const fn coalescing_allowed(self) -> bool {
        self.coalescing_allowed
    }
}

/// An owned authenticated plaintext frame. Block parsing borrows this owner.
pub struct AuthenticatedPlaintext {
    bytes: Vec<u8>,
}

impl AuthenticatedPlaintext {
    fn new(bytes: Vec<u8>) -> Result<Self, FrameError> {
        if bytes.len() > MAX_PLAINTEXT_LENGTH {
            return Err(FrameError::PayloadTooLarge);
        }
        // Validate before exposing the owner. The caller may parse again to
        // borrow semantic blocks without retaining any unauthenticated bytes.
        parse_blocks(&bytes)?;
        Ok(Self { bytes })
    }

    /// Borrows the authenticated plaintext bytes for one parser pass.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Returns the bounded plaintext length.
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    /// Returns whether the authenticated plaintext has no blocks.
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Parses blocks while retaining this frame as their owner.
    pub fn parse(&self) -> Result<ParsedBlocks<'_>, BlockError> {
        parse_blocks(&self.bytes)
    }
}

impl fmt::Debug for AuthenticatedPlaintext {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AuthenticatedPlaintext")
            .field("length", &self.bytes.len())
            .finish()
    }
}

/// A completed receive result with typed terminal observation.
pub struct ReceivedFrame {
    length: FrameLength,
    plaintext: AuthenticatedPlaintext,
    termination: Option<TerminationBlock>,
}

impl ReceivedFrame {
    /// Returns clear frame lengths without exposing frame bytes.
    pub const fn length(&self) -> FrameLength {
        self.length
    }

    /// Borrows the authenticated plaintext owner.
    pub const fn plaintext(&self) -> &AuthenticatedPlaintext {
        &self.plaintext
    }

    /// Returns the authenticated termination metadata, if present.
    pub const fn termination(&self) -> Option<TerminationBlock> {
        self.termination
    }
}

impl fmt::Debug for ReceivedFrame {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ReceivedFrame")
            .field("length", &self.length)
            .field("plaintext", &self.plaintext)
            .field("terminated", &self.termination.is_some())
            .finish()
    }
}

/// Directional transmit owner. It owns exactly one cipher and length state.
pub struct TransmitState {
    cipher: CipherState,
    lengths: SipHashState,
    frames_sent: u64,
    terminated: bool,
}

impl TransmitState {
    /// Creates a transmit owner from the handshake's directional primitive state.
    pub const fn new(cipher: CipherState, lengths: SipHashState) -> Self {
        Self {
            cipher,
            lengths,
            frames_sent: 0,
            terminated: false,
        }
    }

    /// Returns the number of accepted transmitted frames.
    pub const fn frames_sent(&self) -> u64 {
        self.frames_sent
    }

    /// Returns whether no further frame may be sent.
    pub const fn is_terminated(&self) -> bool {
        self.terminated
    }

    /// Seals one authenticated plaintext frame and advances each state once.
    pub fn seal_plaintext(&mut self, plaintext: &[u8]) -> Result<EncodedFrame, FrameError> {
        if self.terminated {
            return Err(FrameError::StateViolation);
        }
        if plaintext.len() > MAX_PLAINTEXT_LENGTH {
            return Err(FrameError::PayloadTooLarge);
        }
        let ciphertext_length = plaintext
            .len()
            .checked_add(constants::AUTH_TAG_LENGTH)
            .ok_or(FrameError::PayloadTooLarge)?;
        let clear_length =
            u16::try_from(ciphertext_length).map_err(|_| FrameError::PayloadTooLarge)?;
        let ciphertext = match self.cipher.seal(plaintext, &[]) {
            Ok(value) => value,
            Err(error) => {
                if matches!(error, Ntcp2CryptoError::NonceExhausted) {
                    self.terminated = true;
                }
                return Err(map_crypto(error));
            }
        };
        let obfuscated = self
            .lengths
            .obfuscate_length(clear_length)
            .map_err(map_crypto)?;
        let mut bytes = Vec::with_capacity(2 + ciphertext.len());
        bytes.extend_from_slice(&obfuscated.to_be_bytes());
        bytes.extend_from_slice(&ciphertext);
        self.frames_sent = self.frames_sent.saturating_add(1);
        let frame = EncodedFrame::new(bytes)?;
        Ok(frame)
    }

    /// Encodes blocks and seals them as one frame using a pure policy input.
    pub fn seal_blocks(
        &mut self,
        blocks: Vec<Block>,
        policy: FrameAssemblyPolicy,
    ) -> Result<EncodedFrame, FrameError> {
        let mut plaintext = encode_blocks(blocks)?;
        if policy.selected_padding > 0 {
            // Re-encode through the strict block owner so padding ordering and
            // the total plaintext ceiling remain a single checked operation.
            let blocks = parse_blocks(&plaintext)?.into_blocks();
            if blocks
                .iter()
                .any(|block| matches!(block, DecodedBlock::Padding { .. }))
            {
                return Err(FrameError::Blocks(BlockError::DuplicateBlock));
            }
            if plaintext.len() + 3 + policy.selected_padding > MAX_PLAINTEXT_LENGTH {
                return Err(FrameError::PayloadTooLarge);
            }
            // The test policy uses zero bytes only; production callers should
            // provide random padding through a later runtime-owned adapter.
            plaintext.extend_from_slice(&[crate::block::BLOCK_PADDING]);
            plaintext.extend_from_slice(&(policy.selected_padding as u16).to_be_bytes());
            plaintext.resize(plaintext.len() + policy.selected_padding, 0);
        }
        let maximum_plaintext = policy
            .maximum_frame_length
            .checked_sub(constants::AUTH_TAG_LENGTH)
            .ok_or(FrameError::PayloadTooLarge)?;
        if plaintext.len() > maximum_plaintext {
            return Err(FrameError::PayloadTooLarge);
        }
        self.seal_plaintext(&plaintext)
    }
}

impl fmt::Debug for TransmitState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TransmitState")
            .field("frames_sent", &self.frames_sent)
            .field("terminated", &self.terminated)
            .finish()
    }
}

enum ReceiveStage {
    Ready,
    AwaitingCiphertext(FrameLength),
    Terminated,
}

/// Directional receive owner. It authenticates before any block is exposed.
pub struct ReceiveState {
    cipher: CipherState,
    lengths: SipHashState,
    frames_received: u64,
    stage: ReceiveStage,
}

impl ReceiveState {
    /// Creates a receive owner from the handshake's directional primitive state.
    pub const fn new(cipher: CipherState, lengths: SipHashState) -> Self {
        Self {
            cipher,
            lengths,
            frames_received: 0,
            stage: ReceiveStage::Ready,
        }
    }

    /// Returns the number of successfully authenticated frames.
    pub const fn frames_received(&self) -> u64 {
        self.frames_received
    }

    /// Returns whether this receive owner has entered terminal state.
    pub const fn is_terminated(&self) -> bool {
        matches!(self.stage, ReceiveStage::Terminated)
    }

    /// Deobfuscates one complete 2-byte length prefix before allocation.
    pub fn decode_length(&mut self, prefix: [u8; 2]) -> Result<FrameLength, FrameError> {
        if !matches!(self.stage, ReceiveStage::Ready) {
            return Err(FrameError::StateViolation);
        }
        let clear = self
            .lengths
            .deobfuscate_length(u16::from_be_bytes(prefix))
            .map_err(|error| {
                self.stage = ReceiveStage::Terminated;
                map_crypto(error)
            })?;
        let length = match FrameLength::from_ciphertext_length(clear) {
            Ok(value) => value,
            Err(error) => {
                self.stage = ReceiveStage::Terminated;
                return Err(error);
            }
        };
        self.stage = ReceiveStage::AwaitingCiphertext(length);
        Ok(length)
    }

    /// Authenticates one ciphertext after a successful length decode.
    pub fn open_ciphertext(&mut self, ciphertext: &[u8]) -> Result<ReceivedFrame, FrameError> {
        let length = match self.stage {
            ReceiveStage::AwaitingCiphertext(length) => length,
            _ => return Err(FrameError::StateViolation),
        };
        if ciphertext.len() != length.ciphertext_length {
            self.stage = ReceiveStage::Terminated;
            return Err(FrameError::TruncatedCiphertext);
        }
        let plaintext = match self.cipher.open(ciphertext, &[]) {
            Ok(value) => value,
            Err(error) => {
                self.stage = ReceiveStage::Terminated;
                return Err(map_crypto(error));
            }
        };
        let plaintext = match AuthenticatedPlaintext::new(plaintext) {
            Ok(value) => value,
            Err(error) => {
                self.stage = ReceiveStage::Terminated;
                return Err(error);
            }
        };
        let parsed = plaintext.parse().map_err(|error| {
            self.stage = ReceiveStage::Terminated;
            FrameError::Blocks(error)
        })?;
        let termination = parsed.blocks().iter().find_map(|block| {
            if let DecodedBlock::Termination(value) = block {
                Some(*value)
            } else {
                None
            }
        });
        self.frames_received = self.frames_received.saturating_add(1);
        self.stage = if termination.is_some() {
            ReceiveStage::Terminated
        } else {
            ReceiveStage::Ready
        };
        Ok(ReceivedFrame {
            length,
            plaintext,
            termination,
        })
    }

    /// Processes one complete length-prefixed wire frame.
    pub fn open_wire_frame(&mut self, wire: &[u8]) -> Result<ReceivedFrame, FrameError> {
        if wire.len() < 2 {
            self.stage = ReceiveStage::Terminated;
            return Err(FrameError::TruncatedLength);
        }
        let length = self.decode_length([wire[0], wire[1]])?;
        if wire.len() != 2 + length.ciphertext_length {
            self.stage = ReceiveStage::Terminated;
            return Err(FrameError::TruncatedCiphertext);
        }
        self.open_ciphertext(&wire[2..])
    }
}

impl fmt::Debug for ReceiveState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ReceiveState")
            .field("frames_received", &self.frames_received)
            .field("terminated", &self.is_terminated())
            .finish()
    }
}

/// Splits the handshake-owned directional keys into independent state owners.
pub fn into_directional_states(keys: SplitKeys) -> (TransmitState, ReceiveState) {
    let (transmit_cipher, receive_cipher, transmit_lengths, receive_lengths) = keys.into_parts();
    (
        TransmitState::new(transmit_cipher, transmit_lengths),
        ReceiveState::new(receive_cipher, receive_lengths),
    )
}

/// A small typed action emitted by a runtime-neutral data-phase adapter.
pub enum FrameAction {
    /// Flush one complete owned frame to the runtime's stream writer.
    Write(EncodedFrame),
    /// Close after an authenticated or locally classified termination.
    Terminate(TerminationBlock),
}

impl fmt::Debug for FrameAction {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Write(frame) => formatter.debug_tuple("Write").field(frame).finish(),
            Self::Terminate(reason) => formatter.debug_tuple("Terminate").field(reason).finish(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::{Block, I2npMessageBlock, TimestampBlock};

    fn states() -> (TransmitState, ReceiveState) {
        (
            TransmitState::new(
                CipherState::from_key_for_test([0x11; 32]),
                SipHashState::from_material_for_test([0x22; 32]),
            ),
            ReceiveState::new(
                CipherState::from_key_for_test([0x11; 32]),
                SipHashState::from_material_for_test([0x22; 32]),
            ),
        )
    }

    fn hex_fixture(value: &str) -> Vec<u8> {
        value
            .trim()
            .as_bytes()
            .chunks_exact(2)
            .map(|pair| {
                let high = (pair[0] as char).to_digit(16).expect("fixture hex");
                let low = (pair[1] as char).to_digit(16).expect("fixture hex");
                ((high << 4) | low) as u8
            })
            .collect()
    }

    #[test]
    fn frame_length_is_obfuscated_and_partial_prefix_is_typed() {
        let (mut tx, mut rx) = states();
        let frame = tx.seal_plaintext(b"payload").expect("seal");
        assert_ne!(frame.as_bytes()[..2], [0, 23]);
        assert!(matches!(
            rx.open_wire_frame(&frame.as_bytes()[..1]),
            Err(FrameError::TruncatedLength)
        ));
        assert!(rx.is_terminated());
    }

    #[test]
    fn authenticated_frames_round_trip_and_tag_failure_is_terminal() {
        let (mut tx, mut rx) = states();
        let plaintext =
            encode_blocks(vec![Block::Timestamp(TimestampBlock::new(9))]).expect("timestamp");
        let frame = tx.seal_plaintext(&plaintext).expect("seal");
        assert_eq!(
            frame.as_bytes(),
            hex_fixture(include_str!(
                "../../../tests/fixtures/ntcp2/crypto/data-phase-frame.hex"
            ))
            .as_slice()
        );
        let received = rx.open_wire_frame(frame.as_bytes()).expect("open");
        assert_eq!(received.length().plaintext_length, plaintext.len());
        assert_eq!(received.plaintext().as_bytes(), plaintext);
        assert_eq!(tx.frames_sent(), 1);
        assert_eq!(rx.frames_received(), 1);

        let second = encode_blocks(vec![Block::Timestamp(TimestampBlock::new(10))])
            .expect("second timestamp");
        let mut mutated = tx.seal_plaintext(&second).expect("second").into_bytes();
        let last = mutated.len() - 1;
        mutated[last] ^= 1;
        assert!(matches!(
            rx.open_wire_frame(&mutated),
            Err(FrameError::AuthenticationFailure)
        ));
        assert!(rx.is_terminated());
    }

    #[test]
    fn blocks_are_assembled_with_bounded_deterministic_padding() {
        let (mut tx, mut rx) = states();
        let policy = FrameAssemblyPolicy::new(128, 2, 4, 3, true).expect("policy");
        let frame = tx
            .seal_blocks(
                vec![
                    Block::Timestamp(TimestampBlock::new(77)),
                    Block::I2np(
                        I2npMessageBlock::from_bytes(vec![3, 0, 0, 0, 1, 0, 0, 0, 2])
                            .expect("I2NP"),
                    ),
                ],
                policy,
            )
            .expect("frame");
        let received = rx.open_wire_frame(frame.as_bytes()).expect("open");
        let parsed = received.plaintext().parse().expect("blocks");
        assert_eq!(parsed.blocks().len(), 3);
        assert!(matches!(
            parsed.blocks().last(),
            Some(DecodedBlock::Padding { length: 3 })
        ));
    }
}
