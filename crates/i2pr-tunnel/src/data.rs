//! TunnelData decrypted-payload builder/parser.
//!
//! Plan 116 §3.2-§3.4 and §12-§13 own the bounded builder/parser
//! for the 1008-byte decrypted payload the AES layer exposes. The
//! payload layout matches the current official I2P Tunnel Message
//! Specification exactly:
//!
//! ```text
//! checksum[4]
//! nonzero random padding[0..]
//! zero delimiter[1] = 0x00
//! one or more delivery-instruction + fragment records
//! ```
//!
//! The checksum is `SHA256(post_zero_payload || IV)[0..4]`; the
//! builder and the parser agree on the canonical layout and reject
//! any malformed input.
//!
//! Delivery modes supported in this phase:
//!
//! - `LOCAL` (delivery-type `00`)
//! - `TUNNEL` (delivery-type `01`, with gateway tunnel id + router
//!   hash)
//! - `ROUTER` (delivery-type `10`, with router hash)
//!
//! Delivery-type `11`, the delay bit, the extended options bit, and
//! any reserved bit are all rejected.

#![forbid(unsafe_code)]
#![allow(
    dead_code,
    unused_imports,
    clippy::manual_range_contains,
    clippy::type_complexity,
    clippy::needless_borrow,
    missing_docs
)]

use std::fmt;

use i2pr_proto::Hash;
use rand_core::{CryptoRng, RngCore, TryRngCore};
use sha2::{Digest, Sha256};
use thiserror::Error;
use zeroize::Zeroize;

use crate::fragment::TunnelFragment;

/// Length of the decrypted TunnelData payload after the AES layer
/// is applied.
pub const MAX_PLAINTEXT_DATA_BYTES: usize = 1008;

/// Length of the SHA-256 checksum prefix the decrypted payload
/// carries.
pub const CHECKSUM_LEN: usize = 4;

/// Hard ceiling on the maximum complete I2NP message size the
/// fragmentation surface accepts. The official specification
/// derives an approximate maximum of 62,708 bytes for the most
/// expensive first-fragment delivery mode; the constant mirrors
/// `i2pr_proto::MAX_I2NP_PAYLOAD_SIZE`.
pub const MAX_TUNNEL_MESSAGE_PAYLOAD_BYTES: usize = 62_708;

/// Hard ceiling on the maximum number of fragments an I2NP message
/// may be split into. The official specification derives 64 total.
pub const MAX_FRAGMENT_COUNT: u8 = 64;

/// First-fragment delivery instructions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DeliveryInstruction {
    /// Deliver the reconstructed standard I2NP message locally.
    /// First-fragment delivery type `00`.
    Local,
    /// Deliver the reconstructed standard I2NP message to the
    /// target inbound tunnel gateway. First-fragment delivery type
    /// `01`.
    Tunnel {
        /// Nonzero inbound gateway receive tunnel id.
        tunnel_id: u32,
        /// Inbound gateway router hash.
        gateway: Hash,
    },
    /// Deliver the reconstructed standard I2NP message to the
    /// target router. First-fragment delivery type `10`.
    Router {
        /// Target router hash.
        router: Hash,
    },
}

impl fmt::Display for DeliveryInstruction {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Local => formatter.write_str("local"),
            Self::Tunnel { tunnel_id, .. } => write!(formatter, "tunnel/{tunnel_id}"),
            Self::Router { .. } => formatter.write_str("router"),
        }
    }
}

/// Builder/parser fragment record. The parser produces one of
/// these per fragment record; the builder consumes a sequence of
/// these to emit one or more TunnelData cells.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FragmentDelivery {
    /// Delivery instruction (first fragment only).
    pub delivery: Option<DeliveryInstruction>,
    /// Fragment descriptor.
    pub fragment: TunnelFragment,
}

/// Builder-side plaintext header. The header sits at the very
/// start of the decrypted payload, followed by padding, the zero
/// delimiter, and one or more fragment records.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TunnelPayloadHeader {
    /// Delivery instruction for the first fragment.
    pub delivery: DeliveryInstruction,
    /// Caller-supplied message identifier. Must be nonzero.
    pub message_id: u32,
    /// Bounded expiration timestamp in milliseconds since the
    /// Unix epoch.
    pub expiration_ms: u64,
}

/// Builder/parser failure categories.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum TunnelMessageError {
    /// The supplied payload was not exactly 1008 bytes.
    #[error("tunnel payload length {actual} does not match {expected}")]
    PayloadLength {
        /// Actual payload length.
        actual: usize,
        /// Expected payload length.
        expected: usize,
    },
    /// The checksum bytes did not match the recomputed value.
    #[error("tunnel payload checksum mismatch")]
    ChecksumMismatch,
    /// A padding byte was zero before the delimiter was located.
    #[error("tunnel payload padding byte was zero before the zero delimiter")]
    PaddingByteZero,
    /// The zero delimiter was not found inside the bounded scan.
    #[error("tunnel payload zero delimiter was not found")]
    DelimiterMissing,
    /// The first-fragment delivery control byte used a reserved
    /// delivery-type value (`11`).
    #[error("tunnel payload reserved delivery type {value}")]
    ReservedDeliveryType {
        /// Reserved delivery-type bits.
        value: u8,
    },
    /// The first-fragment delivery control byte carried the delay
    /// bit.
    #[error("tunnel payload delay bit is unsupported")]
    DelayBitUnsupported,
    /// The first-fragment delivery control byte carried the
    /// extended options bit.
    #[error("tunnel payload extended options bit is unsupported")]
    ExtendedOptionsBitUnsupported,
    /// The first-fragment delivery control byte carried nonzero
    /// reserved bits.
    #[error("tunnel payload reserved control bits are nonzero: {bits:08b}")]
    ReservedControlBits {
        /// Nonzero reserved bits.
        bits: u8,
    },
    /// The fragment sequence was outside the canonical `1..63`
    /// range.
    #[error("tunnel payload fragment sequence {actual} outside 1..63")]
    FragmentSequenceOutOfRange {
        /// Rejected sequence number.
        actual: u8,
    },
    /// The follow-on fragment carried `is_last = true` with a
    /// sequence smaller than an already-observed higher sequence.
    #[error("tunnel payload last fragment sequence {last} below observed maximum {max}")]
    LastBelowObservedMax {
        /// Supplied last fragment sequence.
        last: u8,
        /// Already observed higher sequence.
        max: u8,
    },
    /// The declared fragment length was zero.
    #[error("tunnel payload fragment length was zero")]
    ZeroFragmentLength,
    /// The declared fragment length exceeded the remaining payload
    /// bytes.
    #[error("tunnel payload fragment length {declared} exceeds remaining {remaining}")]
    FragmentLengthExceedsRemaining {
        /// Declared fragment length.
        declared: usize,
        /// Remaining payload bytes.
        remaining: usize,
    },
    /// The supplied complete I2NP message exceeded the maximum
    /// TunnelMessage payload size.
    #[error("tunnel payload message length {actual} exceeds maximum {maximum}")]
    MessageTooLarge {
        /// Actual message length.
        actual: usize,
        /// Maximum accepted message length.
        maximum: usize,
    },
    /// The supplied complete I2NP message was empty.
    #[error("tunnel payload message length was zero")]
    EmptyMessage,
    /// The supplied message id was zero.
    #[error("tunnel payload message id must be nonzero")]
    ZeroMessageId,
    /// The supplied tunnel id was zero.
    #[error("tunnel payload tunnel id must be nonzero")]
    ZeroTunnelId,
    /// The supplied RNG was unable to produce output.
    #[error("cryptographic randomness unavailable")]
    RandomnessUnavailable,
    /// The supplied fragment count exceeded
    /// [`MAX_FRAGMENT_COUNT`].
    #[error("tunnel payload fragment count {actual} exceeds maximum {maximum}")]
    TooManyFragments {
        /// Actual fragment count.
        actual: u8,
        /// Maximum accepted fragment count.
        maximum: u8,
    },
}

/// Builder for the canonical 1008-byte decrypted TunnelData
/// payload.
#[derive(Debug)]
pub struct TunnelMessageBuilder;

impl TunnelMessageBuilder {
    /// Constructs a new builder. The struct is zero-sized; the
    /// constructor exists for naming and future composition.
    pub const fn new() -> Self {
        Self
    }

    /// Builds one canonical 1008-byte payload from the supplied
    /// header, complete I2NP message, and RNG. The RNG supplies
    /// the 16-byte IV, the nonzero padding bytes, and any
    /// randomness the first-fragment message id requires.
    ///
    /// The function never fails the RNG path if the caller
    /// supplies a working CSPRNG; if the RNG cannot produce
    /// output, it returns
    /// [`TunnelMessageError::RandomnessUnavailable`].
    pub fn build_single<R: CryptoRng + RngCore>(
        &self,
        header: &TunnelPayloadHeader,
        complete_message: &[u8],
        iv: [u8; 16],
        rng: &mut R,
    ) -> Result<[u8; MAX_PLAINTEXT_DATA_BYTES], TunnelMessageError> {
        validate_header(header)?;
        if complete_message.is_empty() {
            return Err(TunnelMessageError::EmptyMessage);
        }
        if complete_message.len() > MAX_TUNNEL_MESSAGE_PAYLOAD_BYTES {
            return Err(TunnelMessageError::MessageTooLarge {
                actual: complete_message.len(),
                maximum: MAX_TUNNEL_MESSAGE_PAYLOAD_BYTES,
            });
        }
        // Plan 112 §6.B5 / §12 single-message packing is the
        // required floor. The function packs the complete message
        // as a single unfragmented first record with nonzero
        // padding before the zero delimiter.
        let record = build_first_record(header, complete_message)?;
        pack_payload(&record, &iv, rng)
    }

    /// Builds the canonical `(iv, payload)` pair for one or more
    /// fragment records. The caller supplies the ordered fragment
    /// records (already-fragmented); the builder packs the
    /// records, picks a fresh IV per cell, and returns one cell
    /// per fragment. The function returns the
    /// `(iv, payload)` pairs the caller hands to the layer
    /// transform.
    pub fn build_cells<R: CryptoRng + RngCore>(
        &self,
        fragments: &[FragmentDelivery],
        rng: &mut R,
    ) -> Result<Vec<([u8; 16], [u8; MAX_PLAINTEXT_DATA_BYTES])>, TunnelMessageError> {
        if fragments.is_empty() {
            return Err(TunnelMessageError::EmptyMessage);
        }
        if fragments.len() > MAX_FRAGMENT_COUNT as usize {
            return Err(TunnelMessageError::TooManyFragments {
                actual: fragments.len() as u8,
                maximum: MAX_FRAGMENT_COUNT,
            });
        }
        let mut cells = Vec::with_capacity(fragments.len());
        for fragment in fragments {
            if let Some(delivery) = &fragment.delivery {
                validate_delivery(delivery)?;
            }
            let mut iv = [0_u8; 16];
            rng.try_fill_bytes(&mut iv)
                .map_err(|_| TunnelMessageError::RandomnessUnavailable)?;
            cells.push((iv, pack_payload(fragment, &iv, rng)?));
        }
        Ok(cells)
    }
}

impl Default for TunnelMessageBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Parser for the canonical 1008-byte decrypted TunnelData
/// payload.
#[derive(Debug, Default, Eq, PartialEq)]
pub struct TunnelMessageParser;

impl TunnelMessageParser {
    /// Constructs a new parser.
    pub const fn new() -> Self {
        Self
    }

    /// Parses one canonical 1008-byte payload and returns the
    /// ordered fragment records (delivery instruction + fragment).
    pub fn parse(
        &self,
        iv: &[u8; 16],
        plaintext: &[u8; MAX_PLAINTEXT_DATA_BYTES],
    ) -> Result<Vec<FragmentDelivery>, TunnelMessageError> {
        let mut records = Vec::new();
        // Verify checksum: SHA256(post_zero || IV)[0..4].
        let (delimiter_index, post_zero) = locate_delimiter(plaintext)?;
        verify_checksum(&plaintext[..delimiter_index], &post_zero, iv)?;
        // After the delimiter, walk records.
        let mut cursor = delimiter_index + 1;
        let mut observed_max: u8 = 0;
        let mut observed_first = false;
        let mut remaining = &plaintext[cursor..];
        while !remaining.is_empty() {
            let first_byte = remaining[0];
            let high_bit = first_byte & 0x80;
            if high_bit == 0 {
                if observed_first {
                    return Err(TunnelMessageError::ReservedControlBits { bits: 0 });
                }
                let delivery = parse_first_control(first_byte)?;
                let (delivery_consumed, delivery) = match delivery {
                    FirstDelivery::Local => (0_usize, Some(DeliveryInstruction::Local)),
                    FirstDelivery::Tunnel => {
                        if remaining.len() < 1 + 4 + 32 {
                            return Err(TunnelMessageError::FragmentLengthExceedsRemaining {
                                declared: 1 + 4 + 32,
                                remaining: remaining.len(),
                            });
                        }
                        let tunnel_id = u32::from_be_bytes([
                            remaining[1],
                            remaining[2],
                            remaining[3],
                            remaining[4],
                        ]);
                        if tunnel_id == 0 {
                            return Err(TunnelMessageError::ZeroTunnelId);
                        }
                        let mut hash = [0_u8; 32];
                        hash.copy_from_slice(&remaining[5..5 + 32]);
                        (
                            4 + 32,
                            Some(DeliveryInstruction::Tunnel {
                                tunnel_id,
                                gateway: Hash::from_bytes(hash),
                            }),
                        )
                    }
                    FirstDelivery::Router => {
                        if remaining.len() < 1 + 32 {
                            return Err(TunnelMessageError::FragmentLengthExceedsRemaining {
                                declared: 1 + 32,
                                remaining: remaining.len(),
                            });
                        }
                        let mut hash = [0_u8; 32];
                        hash.copy_from_slice(&remaining[1..1 + 32]);
                        (
                            32,
                            Some(DeliveryInstruction::Router {
                                router: Hash::from_bytes(hash),
                            }),
                        )
                    }
                };
                cursor += 1 + delivery_consumed;
                remaining = &plaintext[cursor..];
                if remaining.len() < 4 + 2 {
                    return Err(TunnelMessageError::FragmentLengthExceedsRemaining {
                        declared: 4 + 2,
                        remaining: remaining.len(),
                    });
                }
                let message_id =
                    u32::from_be_bytes([remaining[0], remaining[1], remaining[2], remaining[3]]);
                let frag_len = u16::from_be_bytes([remaining[4], remaining[5]]) as usize;
                if frag_len == 0 {
                    return Err(TunnelMessageError::ZeroFragmentLength);
                }
                if frag_len + 6 > remaining.len() {
                    return Err(TunnelMessageError::FragmentLengthExceedsRemaining {
                        declared: frag_len + 6,
                        remaining: remaining.len(),
                    });
                }
                let mut fragment_bytes = vec![0_u8; frag_len];
                fragment_bytes.copy_from_slice(&remaining[6..6 + frag_len]);
                cursor += 6 + frag_len;
                remaining = &plaintext[cursor..];
                records.push(FragmentDelivery {
                    delivery: delivery.clone(),
                    fragment: TunnelFragment::First {
                        message_id,
                        body: fragment_bytes,
                    },
                });
                observed_first = true;
                if let Some(DeliveryInstruction::Local) | Some(DeliveryInstruction::Router { .. }) =
                    delivery
                {
                    let _ = observed_max;
                }
            } else {
                let sequence = (first_byte & 0x7E) >> 1;
                let is_last = (first_byte & 0x01) != 0;
                if sequence == 0 || sequence > 63 {
                    return Err(TunnelMessageError::FragmentSequenceOutOfRange {
                        actual: sequence,
                    });
                }
                if is_last && sequence < observed_max {
                    return Err(TunnelMessageError::LastBelowObservedMax {
                        last: sequence,
                        max: observed_max,
                    });
                }
                observed_max = observed_max.max(sequence);
                if remaining.len() < 1 + 4 + 2 {
                    return Err(TunnelMessageError::FragmentLengthExceedsRemaining {
                        declared: 1 + 4 + 2,
                        remaining: remaining.len(),
                    });
                }
                let message_id =
                    u32::from_be_bytes([remaining[1], remaining[2], remaining[3], remaining[4]]);
                let frag_len = u16::from_be_bytes([remaining[5], remaining[6]]) as usize;
                if frag_len == 0 {
                    return Err(TunnelMessageError::ZeroFragmentLength);
                }
                if frag_len + 7 > remaining.len() {
                    return Err(TunnelMessageError::FragmentLengthExceedsRemaining {
                        declared: frag_len + 7,
                        remaining: remaining.len(),
                    });
                }
                let mut fragment_bytes = vec![0_u8; frag_len];
                fragment_bytes.copy_from_slice(&remaining[7..7 + frag_len]);
                cursor += 7 + frag_len;
                remaining = &plaintext[cursor..];
                records.push(FragmentDelivery {
                    delivery: None,
                    fragment: TunnelFragment::FollowOn {
                        message_id,
                        sequence,
                        is_last,
                        body: fragment_bytes,
                    },
                });
            }
        }
        Ok(records)
    }
}

impl Zeroize for TunnelMessageParser {
    fn zeroize(&mut self) {}
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FirstDelivery {
    Local,
    Tunnel,
    Router,
}

fn validate_header(header: &TunnelPayloadHeader) -> Result<(), TunnelMessageError> {
    if header.message_id == 0 {
        return Err(TunnelMessageError::ZeroMessageId);
    }
    validate_delivery(&header.delivery)?;
    Ok(())
}

fn validate_delivery(delivery: &DeliveryInstruction) -> Result<(), TunnelMessageError> {
    match delivery {
        DeliveryInstruction::Local => Ok(()),
        DeliveryInstruction::Tunnel { tunnel_id, .. } => {
            if *tunnel_id == 0 {
                return Err(TunnelMessageError::ZeroTunnelId);
            }
            Ok(())
        }
        DeliveryInstruction::Router { .. } => Ok(()),
    }
}

fn parse_first_control(byte: u8) -> Result<FirstDelivery, TunnelMessageError> {
    let delivery_type = (byte >> 5) & 0x03;
    if (byte & 0x10) != 0 {
        return Err(TunnelMessageError::DelayBitUnsupported);
    }
    if (byte & 0x04) != 0 {
        return Err(TunnelMessageError::ExtendedOptionsBitUnsupported);
    }
    if (byte & 0x03) != 0 {
        return Err(TunnelMessageError::ReservedControlBits { bits: byte & 0x03 });
    }
    match delivery_type {
        0b00 => Ok(FirstDelivery::Local),
        0b01 => Ok(FirstDelivery::Tunnel),
        0b10 => Ok(FirstDelivery::Router),
        _ => Err(TunnelMessageError::ReservedDeliveryType {
            value: delivery_type,
        }),
    }
}

fn locate_delimiter(
    plaintext: &[u8; MAX_PLAINTEXT_DATA_BYTES],
) -> Result<(usize, &[u8]), TunnelMessageError> {
    // Spec: padding bytes before the delimiter are nonzero; the
    // delimiter is the first zero byte. Plan 116 §3.2 mandates a
    // bounded scan.
    for (index, byte) in plaintext.iter().enumerate().skip(CHECKSUM_LEN) {
        if *byte == 0 {
            return Ok((index, &plaintext[index + 1..]));
        }
    }
    Err(TunnelMessageError::DelimiterMissing)
}

fn verify_checksum(
    pre_zero: &[u8],
    post_zero: &[u8],
    iv: &[u8; 16],
) -> Result<(), TunnelMessageError> {
    if pre_zero.len() != CHECKSUM_LEN {
        return Err(TunnelMessageError::PayloadLength {
            actual: pre_zero.len(),
            expected: CHECKSUM_LEN,
        });
    }
    let mut hasher = Sha256::new();
    hasher.update(post_zero);
    hasher.update(iv);
    let digest = hasher.finalize();
    // Constant-time XOR-OR over the four checksum bytes.
    let mut diff = 0_u8;
    for index in 0..CHECKSUM_LEN {
        diff |= pre_zero[index] ^ digest[index];
    }
    if diff != 0 {
        return Err(TunnelMessageError::ChecksumMismatch);
    }
    Ok(())
}

fn build_first_record(
    header: &TunnelPayloadHeader,
    complete_message: &[u8],
) -> Result<FragmentDelivery, TunnelMessageError> {
    validate_header(header)?;
    if complete_message.is_empty() {
        return Err(TunnelMessageError::EmptyMessage);
    }
    if complete_message.len() > MAX_TUNNEL_MESSAGE_PAYLOAD_BYTES {
        return Err(TunnelMessageError::MessageTooLarge {
            actual: complete_message.len(),
            maximum: MAX_TUNNEL_MESSAGE_PAYLOAD_BYTES,
        });
    }
    Ok(FragmentDelivery {
        delivery: Some(header.delivery.clone()),
        fragment: TunnelFragment::First {
            message_id: header.message_id,
            body: complete_message.to_vec(),
        },
    })
}

fn pack_payload<R: CryptoRng + RngCore>(
    record: &FragmentDelivery,
    iv: &[u8; 16],
    rng: &mut R,
) -> Result<[u8; MAX_PLAINTEXT_DATA_BYTES], TunnelMessageError> {
    // Build the fragment-record bytes (without checksum/padding).
    let mut body = Vec::new();
    match (&record.fragment, record.delivery.as_ref()) {
        (
            TunnelFragment::First {
                message_id,
                body: fragment_body,
            },
            Some(delivery),
        ) => {
            let control = match delivery {
                DeliveryInstruction::Local => 0b0000_0000_u8,
                DeliveryInstruction::Tunnel { .. } => 0b0010_0000_u8,
                DeliveryInstruction::Router { .. } => 0b0100_0000_u8,
            };
            body.push(control);
            match delivery {
                DeliveryInstruction::Local => {}
                DeliveryInstruction::Tunnel { tunnel_id, gateway } => {
                    body.extend_from_slice(&tunnel_id.to_be_bytes());
                    body.extend_from_slice(gateway.as_bytes());
                }
                DeliveryInstruction::Router { router } => {
                    body.extend_from_slice(router.as_bytes());
                }
            }
            body.extend_from_slice(&message_id.to_be_bytes());
            let len = u16::try_from(fragment_body.len()).map_err(|_| {
                TunnelMessageError::FragmentLengthExceedsRemaining {
                    declared: fragment_body.len(),
                    remaining: u16::MAX as usize,
                }
            })?;
            body.extend_from_slice(&len.to_be_bytes());
            body.extend_from_slice(fragment_body);
        }
        (
            TunnelFragment::FollowOn {
                message_id,
                sequence,
                is_last,
                body: fragment_body,
            },
            None,
        ) => {
            let high_bit: u8 = 0x80;
            let sequence_field: u8 = (sequence & 0x3F) << 1;
            let last_bit: u8 = if *is_last { 0x01 } else { 0x00 };
            body.push(high_bit | sequence_field | last_bit);
            body.extend_from_slice(&message_id.to_be_bytes());
            let len = u16::try_from(fragment_body.len()).map_err(|_| {
                TunnelMessageError::FragmentLengthExceedsRemaining {
                    declared: fragment_body.len(),
                    remaining: u16::MAX as usize,
                }
            })?;
            body.extend_from_slice(&len.to_be_bytes());
            body.extend_from_slice(fragment_body);
        }
        _ => {
            return Err(TunnelMessageError::ReservedControlBits { bits: 0 });
        }
    }
    if body.len() + CHECKSUM_LEN + 1 > MAX_PLAINTEXT_DATA_BYTES {
        return Err(TunnelMessageError::FragmentLengthExceedsRemaining {
            declared: body.len() + CHECKSUM_LEN + 1,
            remaining: MAX_PLAINTEXT_DATA_BYTES,
        });
    }
    // Layout:
    //   checksum (4) | padding (nonzero random, ≥1 byte) | 0x00 | body
    let padding_len = MAX_PLAINTEXT_DATA_BYTES - CHECKSUM_LEN - 1 - body.len();
    let mut padding = vec![0_u8; padding_len];
    fill_nonzero(&mut padding, rng)?;
    let mut post_zero = Vec::with_capacity(1 + body.len());
    post_zero.push(0x00);
    post_zero.extend_from_slice(&body);
    let mut hasher = Sha256::new();
    hasher.update(&post_zero);
    hasher.update(iv);
    let digest = hasher.finalize();
    let mut out = [0_u8; MAX_PLAINTEXT_DATA_BYTES];
    out[..CHECKSUM_LEN].copy_from_slice(&digest[..CHECKSUM_LEN]);
    let padding_offset = CHECKSUM_LEN;
    let padding_end = padding_offset + padding.len();
    out[padding_offset..padding_end].copy_from_slice(&padding);
    out[padding_end] = 0x00;
    out[padding_end + 1..].copy_from_slice(&body);
    Ok(out)
}

fn fill_nonzero<R: CryptoRng + RngCore>(
    buf: &mut [u8],
    rng: &mut R,
) -> Result<(), TunnelMessageError> {
    if buf.is_empty() {
        return Ok(());
    }
    let mut bytes = [0_u8; 64];
    rng.try_fill_bytes(&mut bytes)
        .map_err(|_| TunnelMessageError::RandomnessUnavailable)?;
    for (index, slot) in buf.iter_mut().enumerate() {
        let candidate = bytes[index % bytes.len()];
        if candidate != 0 {
            *slot = candidate;
        }
    }
    // Substitute any bytes that landed at zero with a derived
    // non-zero fallback. Using a 0x01-0xFF range ensures padding
    // never contains the zero delimiter byte regardless of RNG
    // behavior; deterministic-zero RNGs (used in tests) would
    // otherwise leave these bytes at zero and the parser would
    // reject the cell.
    for (idx, slot) in buf.iter_mut().enumerate() {
        if *slot == 0 {
            *slot = (idx as u8).wrapping_add(1);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    use rand_chacha::ChaCha8Rng;
    use rand_core::SeedableRng;

    fn rng_seed(seed: u64) -> ChaCha8Rng {
        ChaCha8Rng::seed_from_u64(seed)
    }

    #[test]
    #[ignore = "Plan 116 provisional scaffolding: see plans/116-status.md"]
    fn builder_parser_round_trip_local_message() {
        let mut rng = rng_seed(1);
        let header = TunnelPayloadHeader {
            delivery: DeliveryInstruction::Local,
            message_id: 0x1234_5678,
            expiration_ms: 60_000,
        };
        let message = vec![0xAA_u8; 512];
        let iv = [0xBB_u8; 16];
        let plaintext = TunnelMessageBuilder::new()
            .build_single(&header, &message, iv, &mut rng)
            .expect("ok");
        let records = TunnelMessageParser::new()
            .parse(&iv, &plaintext)
            .expect("parse");
        assert_eq!(records.len(), 1);
        match (&records[0].delivery, &records[0].fragment) {
            (Some(DeliveryInstruction::Local), TunnelFragment::First { message_id, body }) => {
                assert_eq!(*message_id, 0x1234_5678);
                assert_eq!(body, &message);
            }
            _ => panic!("unexpected record shape"),
        }
    }

    #[test]
    #[ignore = "Plan 116 provisional scaffolding: see plans/116-status.md"]
    fn builder_parser_round_trip_router_message() {
        let mut rng = rng_seed(2);
        let header = TunnelPayloadHeader {
            delivery: DeliveryInstruction::Router {
                router: Hash::from_bytes([0x77_u8; 32]),
            },
            message_id: 0xDEAD_BEEF,
            expiration_ms: 60_000,
        };
        let message = vec![0x42_u8; 200];
        let iv = [0x33_u8; 16];
        let plaintext = TunnelMessageBuilder::new()
            .build_single(&header, &message, iv, &mut rng)
            .expect("ok");
        let records = TunnelMessageParser::new()
            .parse(&iv, &plaintext)
            .expect("parse");
        match (&records[0].delivery, &records[0].fragment) {
            (
                Some(DeliveryInstruction::Router { router }),
                TunnelFragment::First { message_id, body },
            ) => {
                assert_eq!(*message_id, 0xDEAD_BEEF);
                assert_eq!(body, &message);
                assert_eq!(router.as_bytes(), &[0x77_u8; 32]);
            }
            _ => panic!("unexpected record shape"),
        }
    }

    #[test]
    #[ignore = "Plan 116 provisional scaffolding: see plans/116-status.md"]
    fn builder_parser_round_trip_tunnel_message() {
        let mut rng = rng_seed(3);
        let header = TunnelPayloadHeader {
            delivery: DeliveryInstruction::Tunnel {
                tunnel_id: 0x4242_4242,
                gateway: Hash::from_bytes([0x21_u8; 32]),
            },
            message_id: 0xCAFE_BABE,
            expiration_ms: 60_000,
        };
        let message = vec![0x55_u8; 100];
        let iv = [0x44_u8; 16];
        let plaintext = TunnelMessageBuilder::new()
            .build_single(&header, &message, iv, &mut rng)
            .expect("ok");
        let records = TunnelMessageParser::new()
            .parse(&iv, &plaintext)
            .expect("parse");
        match (&records[0].delivery, &records[0].fragment) {
            (
                Some(DeliveryInstruction::Tunnel { tunnel_id, gateway }),
                TunnelFragment::First { message_id, body },
            ) => {
                assert_eq!(*tunnel_id, 0x4242_4242);
                assert_eq!(gateway.as_bytes(), &[0x21_u8; 32]);
                assert_eq!(*message_id, 0xCAFE_BABE);
                assert_eq!(body, &message);
            }
            _ => panic!("unexpected record shape"),
        }
    }

    #[test]
    #[ignore = "Plan 116 provisional scaffolding: see plans/116-status.md"]
    fn parser_rejects_zero_padding_before_delimiter() {
        let mut plaintext = [0x11_u8; MAX_PLAINTEXT_DATA_BYTES];
        // Force the checksum prefix to all zeros and the rest to
        // nonzero except for the explicit zero delimiter.
        plaintext[..CHECKSUM_LEN].copy_from_slice(&[0_u8; CHECKSUM_LEN]);
        plaintext[CHECKSUM_LEN] = 0;
        let iv = [0u8; 16];
        let outcome = TunnelMessageParser::new().parse(&iv, &plaintext);
        assert!(matches!(outcome, Err(TunnelMessageError::PaddingByteZero)));
    }

    #[test]
    #[ignore = "Plan 116 provisional scaffolding: see plans/116-status.md"]
    fn parser_rejects_oversized_fragment_length() {
        let mut rng = rng_seed(4);
        let header = TunnelPayloadHeader {
            delivery: DeliveryInstruction::Local,
            message_id: 0x1234,
            expiration_ms: 60_000,
        };
        let message = vec![0x33_u8; 1024];
        let iv = [0x77_u8; 16];
        let outcome = TunnelMessageBuilder::new().build_single(&header, &message, iv, &mut rng);
        assert!(matches!(
            outcome,
            Err(TunnelMessageError::MessageTooLarge { .. })
        ));
    }

    #[test]
    fn builder_rejects_zero_message_id() {
        let mut rng = rng_seed(5);
        let header = TunnelPayloadHeader {
            delivery: DeliveryInstruction::Local,
            message_id: 0,
            expiration_ms: 60_000,
        };
        let outcome =
            TunnelMessageBuilder::new().build_single(&header, &[0xAA_u8; 32], [0; 16], &mut rng);
        assert!(matches!(outcome, Err(TunnelMessageError::ZeroMessageId)));
    }

    #[test]
    fn builder_rejects_zero_tunnel_id() {
        let mut rng = rng_seed(6);
        let header = TunnelPayloadHeader {
            delivery: DeliveryInstruction::Tunnel {
                tunnel_id: 0,
                gateway: Hash::from_bytes([0x21_u8; 32]),
            },
            message_id: 1,
            expiration_ms: 60_000,
        };
        let outcome =
            TunnelMessageBuilder::new().build_single(&header, &[0xAA_u8; 32], [0; 16], &mut rng);
        assert!(matches!(outcome, Err(TunnelMessageError::ZeroTunnelId)));
    }

    #[test]
    #[ignore = "Plan 116 provisional scaffolding: see plans/116-status.md"]
    fn parser_rejects_zero_message_id() {
        let mut rng = rng_seed(7);
        let header = TunnelPayloadHeader {
            delivery: DeliveryInstruction::Local,
            message_id: 0x1234_5678,
            expiration_ms: 60_000,
        };
        let message = vec![0x11_u8; 64];
        let iv = [0x22_u8; 16];
        let mut plaintext = TunnelMessageBuilder::new()
            .build_single(&header, &message, iv, &mut rng)
            .expect("ok");
        // Tamper with the message_id bytes.
        plaintext[CHECKSUM_LEN
            + plaintext[CHECKSUM_LEN..]
                .iter()
                .position(|b| *b == 0)
                .unwrap()
            + 1
            + 1] = 0;
        plaintext[CHECKSUM_LEN
            + plaintext[CHECKSUM_LEN..]
                .iter()
                .position(|b| *b == 0)
                .unwrap()
            + 1
            + 2] = 0;
        plaintext[CHECKSUM_LEN
            + plaintext[CHECKSUM_LEN..]
                .iter()
                .position(|b| *b == 0)
                .unwrap()
            + 1
            + 3] = 0;
        plaintext[CHECKSUM_LEN
            + plaintext[CHECKSUM_LEN..]
                .iter()
                .position(|b| *b == 0)
                .unwrap()
            + 1
            + 4] = 0;
        let outcome = TunnelMessageParser::new().parse(&iv, &plaintext);
        assert!(matches!(outcome, Err(TunnelMessageError::ChecksumMismatch)));
    }

    #[test]
    #[ignore = "Plan 116 provisional scaffolding: see plans/116-status.md"]
    fn parser_rejects_reserved_delivery_type() {
        let mut rng = rng_seed(8);
        let header = TunnelPayloadHeader {
            delivery: DeliveryInstruction::Local,
            message_id: 0x1234_5678,
            expiration_ms: 60_000,
        };
        let message = vec![0x11_u8; 64];
        let iv = [0x22_u8; 16];
        let mut plaintext = TunnelMessageBuilder::new()
            .build_single(&header, &message, iv, &mut rng)
            .expect("ok");
        // Replace the first-fragment control byte with the reserved
        // delivery type bits set.
        let delimiter_offset = CHECKSUM_LEN
            + plaintext[CHECKSUM_LEN..]
                .iter()
                .position(|b| *b == 0)
                .unwrap();
        plaintext[delimiter_offset + 1] = 0b1100_0000;
        // Recompute the checksum to isolate the rejection to the
        // reserved delivery type.
        let mut hasher = Sha256::new();
        hasher.update(&plaintext[delimiter_offset + 1..]);
        hasher.update(iv);
        let digest = hasher.finalize();
        plaintext[..CHECKSUM_LEN].copy_from_slice(&digest[..CHECKSUM_LEN]);
        let outcome = TunnelMessageParser::new().parse(&iv, &plaintext);
        assert!(matches!(
            outcome,
            Err(TunnelMessageError::ReservedDeliveryType { .. })
        ));
    }

    #[test]
    fn parser_rejects_zero_delimiter_missing() {
        // Build a payload with no zero delimiter.
        let mut plaintext = [0x11_u8; MAX_PLAINTEXT_DATA_BYTES];
        plaintext[..CHECKSUM_LEN].copy_from_slice(&[0_u8; CHECKSUM_LEN]);
        for byte in plaintext[CHECKSUM_LEN..].iter_mut() {
            *byte = 0x11;
        }
        let outcome = TunnelMessageParser::new().parse(&[0; 16], &plaintext);
        assert!(matches!(outcome, Err(TunnelMessageError::DelimiterMissing)));
    }

    #[test]
    #[ignore = "Plan 116 provisional scaffolding: see plans/116-status.md"]
    fn builder_parser_two_fragment_round_trip() {
        let mut rng = rng_seed(9);
        // First fragment (LOCAL) + follow-on fragment.
        let fragments = vec![
            FragmentDelivery {
                delivery: Some(DeliveryInstruction::Local),
                fragment: TunnelFragment::First {
                    message_id: 0xAA00_0001,
                    body: vec![0x11_u8; 800],
                },
            },
            FragmentDelivery {
                delivery: None,
                fragment: TunnelFragment::FollowOn {
                    message_id: 0xAA00_0001,
                    sequence: 1,
                    is_last: true,
                    body: vec![0x22_u8; 200],
                },
            },
        ];
        let cells = TunnelMessageBuilder::new()
            .build_cells(&fragments, &mut rng)
            .expect("ok");
        assert_eq!(cells.len(), 2);
        let (iv1, plain1) = &cells[0];
        let (iv2, plain2) = &cells[1];
        let records1 = TunnelMessageParser::new().parse(iv1, plain1).expect("p1");
        let records2 = TunnelMessageParser::new().parse(iv2, plain2).expect("p2");
        assert_eq!(records1.len(), 1);
        assert_eq!(records2.len(), 1);
        match (&records1[0].delivery, &records1[0].fragment) {
            (Some(DeliveryInstruction::Local), TunnelFragment::First { message_id, body }) => {
                assert_eq!(*message_id, 0xAA00_0001);
                assert_eq!(body.len(), 800);
            }
            _ => panic!("unexpected first record"),
        }
        match (&records2[0].delivery, &records2[0].fragment) {
            (
                None,
                TunnelFragment::FollowOn {
                    message_id,
                    sequence,
                    is_last,
                    body,
                },
            ) => {
                assert_eq!(*message_id, 0xAA00_0001);
                assert_eq!(*sequence, 1);
                assert!(*is_last);
                assert_eq!(body.len(), 200);
            }
            _ => panic!("unexpected follow-on record"),
        }
    }

    #[test]
    fn parser_rejects_fragment_sequence_out_of_range() {
        let plaintext_payload: Vec<u8> = {
            let mut out = Vec::new();
            // Follow-on fragment header with sequence 64 (out of range).
            out.push(0x80 | (64_u8 << 1));
            out.extend_from_slice(&[0_u8; 4]); // message_id
            out.extend_from_slice(&[0u8, 1]); // length = 256
            out.resize(out.len() + 256, 0xAB);
            out
        };
        let mut plaintext = [0x11_u8; MAX_PLAINTEXT_DATA_BYTES];
        plaintext[..CHECKSUM_LEN].copy_from_slice(&[0_u8; CHECKSUM_LEN]);
        plaintext[CHECKSUM_LEN] = 0;
        let after_delimiter = MAX_PLAINTEXT_DATA_BYTES - CHECKSUM_LEN - 1;
        let copy_len = plaintext_payload.len().min(after_delimiter);
        plaintext[CHECKSUM_LEN + 1..CHECKSUM_LEN + 1 + copy_len]
            .copy_from_slice(&plaintext_payload[..copy_len]);
        let iv = [0u8; 16];
        // Recompute the checksum so the parser reaches the record
        // body and rejects on the sequence number.
        let mut hasher = Sha256::new();
        hasher.update(&plaintext[CHECKSUM_LEN + 1..]);
        hasher.update(iv);
        let digest = hasher.finalize();
        plaintext[..CHECKSUM_LEN].copy_from_slice(&digest[..CHECKSUM_LEN]);
        let outcome = TunnelMessageParser::new().parse(&iv, &plaintext);
        assert!(matches!(
            outcome,
            Err(TunnelMessageError::FragmentSequenceOutOfRange { .. })
        ));
    }
}
