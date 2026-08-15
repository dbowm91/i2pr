//! Plan 110 multi-record short tunnel-build construction surface.
//!
//! Plan 110 §6-§17 own the typed representation, fake-record
//! construction, raw-ChaCha20 per-record transform, creator request
//! preprocessing, deterministic per-hop processing, creator reply
//! postprocessing, exact STBM/OTBRM one-byte-count payload codec,
//! and the independent multi-hop conformance fixture that closes
//! the local short-build construction conformance for the current
//! official I2P Tunnel Creation Specification.
//!
//! The module is runtime-neutral. It owns:
//!
//! - typed `SlotIndex` (validated 0..=7) and the [`RecordOwner`]
//!   classification for real hops and the two fake-record variants;
//! - the bounded [`ShortBuildRecordSet`] that ties canonical path
//!   order to randomized wire slots;
//! - the privacy-preserving record-count policy that fills each
//!   `ShortBuildRecordSet` to at least four records and reserves
//!   the inbound originator fake;
//! - raw [`chacha20_transform`] and [`chacha20_xor`] primitives
//!   over the existing RustCrypto `chacha20` crate;
//! - the inbound [`OriginatorFake`] helper that the creator uses to
//!   embed its own truncated identity hash and a fresh X25519
//!   ephemeral key into the dedicated fake slot;
//! - `preprocess_creator_request` that applies the iterative
//!   symmetric ChaCha20 transforms required so each real hop only
//!   sees its own record at its stage;
//! - [`MessageHopProcessor`] (the Plan 110 replacement for the
//!   Plan 109 single-record `DeterministicResponder`) that scans a
//!   multi-record payload, opens its own request, seals a reply,
//!   and transforms every other record once;
//! - [`CreatorReplyPostprocessor`] that undoes the accumulated
//!   later-hop transforms before authenticating every real-hop
//!   reply;
//! - `encode_short_tunnel_build_payload` / `encode_outbound_tunnel_build_reply`
//!   for the exact `1 + count * 218` byte framing;
//! - [`MultiHopReferenceFixture`] that derives expected per-hop
//!   reply bytes from a small specification-shaped reference
//!   implementation.
//!
//! The module deliberately remains runtime-neutral: no sockets, no
//! Tokio runtime, no DNS, no filesystem access. The multi-record
//! construction is local conformance only; live mixed-router
//! execution is a later delivery plan.

#![forbid(unsafe_code)]

use std::fmt;

use chacha20::cipher::{KeyIvInit, StreamCipher};
use rand_core::{CryptoRng, RngCore, TryRngCore};
use sha2::{Digest, Sha256};
use thiserror::Error;
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use i2pr_proto::{
    Hash, SHORT_BUILD_RECORD_SIZE, SHORT_REPLY_PLAINTEXT_SIZE, SHORT_REQUEST_PLAINTEXT_SIZE,
};

use crate::build_crypto::{
    AEAD_KEY_LEN, AEAD_NONCE_LEN, BuildCryptography, BuildCryptographyError, EPHEMERAL_KEY_LEN,
    EciesX25519BuildCryptography, LayerKeys, NoiseRequestState, OpenedShortRequest,
    RECORD_SLOT_NONCE_OFFSET, ValidatedRecordSlot,
};
use crate::identity::TunnelDirection;
use crate::short_record::{
    BuildOptions, HopRole, REQUEST_EXPIRATION_SECONDS, ShortReplyRecord, ShortRequestRecord,
    ShortResponseCode,
};

/// Wire size of a single short tunnel-build record (envelope or
/// hop-own reply). Re-exported here to keep multi-record code
/// self-contained.
pub const RECORD_BYTES: usize = SHORT_BUILD_RECORD_SIZE;

/// Plaintext request size for one short tunnel-build record.
pub const REQUEST_PLAINTEXT_BYTES: usize = SHORT_REQUEST_PLAINTEXT_SIZE;

/// Plaintext reply size for one short tunnel-build record.
pub const REPLY_PLAINTEXT_BYTES: usize = SHORT_REPLY_PLAINTEXT_SIZE;

/// Marker for the inbound creator-ephemeral public-key placement.
///
/// The current I2P Tunnel Creation Specification states that the
/// creator ECIES ephemeral public key is included in the inbound
/// short request plaintext because the IBGW layer has no build-record
/// DH, but does not pin the exact byte offset. Plan 111 §F preserves
/// the placeholder in the canonical 154-byte plaintext so a future
/// pinned implementation can land without rewriting the layout, and
/// disables inbound `prepare_short_build_message` until a current
/// reference-router source (Java I2P or i2pd) is available to pin
/// the interoperable location. The placeholder occupies the same
/// offset in every record so the test suite can detect drift.
pub const INBOUND_CREATOR_EPHEMERAL_PLACEHOLDER_LEN: usize = 0;

/// Plan 111 §F marker: inbound short-build construction is
/// `blocked-inbound-layout-ambiguity` until a current reference
/// implementation is available to pin the inbound creator-ephemeral
/// public-key placement. The constant exists so a future audit can
/// detect if the inbound path is silently re-enabled without the
/// corresponding placement decision.
pub const INBOUND_SHORT_BUILD_LAYOUT_AMBIGUITY: bool = true;

/// Hard ceiling on the wire record count.
pub const MAX_RECORD_COUNT: u8 = 8;

/// Minimum record count the local production builder emits. The
/// privacy-preserving policy is documented in
/// [`build_minimum_record_count`].
pub const MIN_PRODUCTION_RECORD_COUNT: u8 = 4;

/// Raw ChaCha20 32-byte key length.
pub const CHACHA20_KEY_LEN: usize = AEAD_KEY_LEN;

/// Record ownership category for a single wire slot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecordOwner {
    /// A real participating hop at the supplied canonical index.
    RealHop {
        /// Canonical hop index (zero is the first real hop).
        hop_index: u8,
    },
    /// The mandatory inbound originator fake record. The record
    /// carries the originator's truncated identity hash and a
    /// fresh X25519 ephemeral public key. Only present on inbound
    /// builds.
    OriginatorFake,
    /// A padding fake record. Not interpreted by any hop.
    PaddingFake,
}

impl RecordOwner {
    /// Returns the category label for diagnostics.
    pub const fn label(self) -> &'static str {
        match self {
            Self::RealHop { .. } => "real-hop",
            Self::OriginatorFake => "originator-fake",
            Self::PaddingFake => "padding-fake",
        }
    }

    /// Returns `true` when the owner is not a real participating hop.
    pub const fn is_fake(self) -> bool {
        matches!(self, Self::OriginatorFake | Self::PaddingFake)
    }
}

/// Typed record slot identifier. Valid domain is `0..=7`. The
/// `ValidatedRecordSlot` used by the build cryptography is a
/// re-export of this same domain; the type alias keeps the
/// multi-record API self-documenting.
pub type SlotIndex = ValidatedRecordSlot;

/// Bounded typed representation of one record-slot allocation inside
/// a [`ShortBuildRecordSet`]. Slot indices are unique by
/// construction; each canonical hop appears exactly once and no
/// real hop shares a slot with any fake record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RecordAssignment {
    /// Wire slot index in `[0, 7]`.
    pub slot: SlotIndex,
    /// Canonical hop or fake identity.
    pub owner: RecordOwner,
}

impl RecordAssignment {
    /// Constructs a record assignment with validation.
    pub const fn new(slot: SlotIndex, owner: RecordOwner) -> Self {
        Self { slot, owner }
    }
}

/// Bounded typed record set. The set owns at most eight slots and
/// requires at least one slot. Canonical hop order and wire slot
/// order are independent; the set stores the canonical order and a
/// fixed-size owner/slot table so iteration and lookup remain O(1)
/// over at most eight entries.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShortBuildRecordSet {
    /// Total wire slot count. Always in `1..=8`.
    count: u8,
    /// Owner of each wire slot indexed by slot number.
    slots: [Option<RecordOwner>; MAX_RECORD_COUNT as usize],
}

impl ShortBuildRecordSet {
    /// Returns the wire record count.
    pub const fn record_count(&self) -> u8 {
        self.count
    }

    /// Returns the canonical hop index for the supplied wire slot,
    /// when the slot belongs to a real hop.
    pub fn real_hop_at_slot(&self, slot: SlotIndex) -> Option<u8> {
        match self.slots[slot.get() as usize] {
            Some(RecordOwner::RealHop { hop_index }) => Some(hop_index),
            _ => None,
        }
    }

    /// Returns the wire slot assigned to the supplied canonical
    /// hop, when present.
    pub fn slot_for_real_hop(&self, hop_index: u8) -> Option<SlotIndex> {
        for index in 0..self.count {
            if self.slots[index as usize] == Some(RecordOwner::RealHop { hop_index }) {
                let slot = SlotIndex::new(index).expect("validated slot");
                return Some(slot);
            }
        }
        None
    }

    /// Returns the owner of the supplied wire slot.
    pub fn owner_at_slot(&self, slot: SlotIndex) -> Option<RecordOwner> {
        self.slots[slot.get() as usize]
    }

    /// Returns the canonical hop index of every real hop, in
    /// canonical hop order.
    pub fn real_hop_indices(&self) -> Vec<u8> {
        let mut out = Vec::new();
        for entry in &self.slots[..self.count as usize] {
            if let Some(RecordOwner::RealHop { hop_index }) = entry {
                out.push(*hop_index);
            }
        }
        out
    }

    /// Returns the wire slot and owner for every wire slot, in
    /// wire order.
    pub fn slot_assignments(&self) -> Vec<RecordAssignment> {
        let mut out = Vec::with_capacity(self.count as usize);
        for index in 0..self.count {
            let slot = SlotIndex::new(index).expect("validated slot");
            if let Some(owner) = self.slots[index as usize] {
                out.push(RecordAssignment::new(slot, owner));
            }
        }
        out
    }

    /// Returns whether the supplied wire slot is the originator
    /// fake slot.
    pub fn is_originator_fake_slot(&self, slot: SlotIndex) -> bool {
        matches!(
            self.slots[slot.get() as usize],
            Some(RecordOwner::OriginatorFake)
        )
    }

    /// Returns whether the supplied wire slot is a padding fake.
    pub fn is_padding_fake_slot(&self, slot: SlotIndex) -> bool {
        matches!(
            self.slots[slot.get() as usize],
            Some(RecordOwner::PaddingFake)
        )
    }

    /// Returns whether every real hop has been assigned a slot.
    pub fn is_complete(&self, real_hop_count: u8) -> bool {
        let mut assigned = 0_u8;
        for entry in &self.slots[..self.count as usize] {
            if matches!(entry, Some(RecordOwner::RealHop { .. })) {
                assigned = assigned.saturating_add(1);
            }
        }
        assigned == real_hop_count
    }

    /// Computes the canonical hop index for the supplied wire slot,
    /// even when the slot belongs to a fake record.
    pub fn slot_index(&self, slot: SlotIndex) -> usize {
        slot.get() as usize
    }
}

/// Computes the minimum wire record count the local production
/// builder emits for a path that declares `real_hop_count` real
/// hops and the supplied direction.
///
/// The privacy-preserving policy:
/// - outbound always emits at least four records;
/// - inbound always emits at least four records and reserves one
///   slot for the mandatory originator fake;
/// - paths with more than eight required records are rejected
///   before allocation.
pub fn build_minimum_record_count(
    real_hop_count: u8,
    direction: TunnelDirection,
) -> Result<u8, MultiRecordError> {
    if real_hop_count == 0 {
        return Err(MultiRecordError::EmptyPath);
    }
    if real_hop_count > MAX_RECORD_COUNT {
        return Err(MultiRecordError::HopCountExceedsMaximum {
            actual: real_hop_count,
            maximum: MAX_RECORD_COUNT,
        });
    }
    let reserved_for_originator = matches!(direction, TunnelDirection::Inbound);
    let min_for_real = real_hop_count;
    // Need at least four slots total and one slot reserved for the
    // originator fake on inbound builds.
    let candidate = match direction {
        TunnelDirection::Outbound => {
            if min_for_real >= MIN_PRODUCTION_RECORD_COUNT {
                min_for_real
            } else {
                MIN_PRODUCTION_RECORD_COUNT
            }
        }
        TunnelDirection::Inbound => {
            let with_originator = min_for_real.saturating_add(1);
            if with_originator >= MIN_PRODUCTION_RECORD_COUNT {
                with_originator
            } else {
                MIN_PRODUCTION_RECORD_COUNT
            }
        }
    };
    let candidate = candidate.max(MIN_PRODUCTION_RECORD_COUNT);
    if candidate > MAX_RECORD_COUNT {
        return Err(MultiRecordError::RecordCountExceedsMaximum {
            actual: candidate,
            maximum: MAX_RECORD_COUNT,
        });
    }
    let _ = reserved_for_originator;
    Ok(candidate)
}

/// Assigns slots to the supplied real hops plus the mandatory
/// fake records required by the policy. The permutation uses the
/// supplied RNG so deterministic seeds reproduce exact slot
/// mappings.
pub fn assign_record_slots<R: CryptoRng + RngCore>(
    real_hop_count: u8,
    direction: TunnelDirection,
    rng: &mut R,
) -> Result<ShortBuildRecordSet, MultiRecordError> {
    let total = build_minimum_record_count(real_hop_count, direction)?;
    let reserved_originator = matches!(direction, TunnelDirection::Inbound);
    // Reserve a dedicated slot for the originator fake on inbound
    // builds before applying the random permutation to the
    // remaining slots.
    let mut slot_owners: Vec<Option<RecordOwner>> = (0..total as usize).map(|_| None).collect();

    let mut remaining_slots: Vec<u8> = (0..total).collect();
    if reserved_originator {
        // Pick a uniformly random slot for the originator fake.
        let picked = pick_uniform_index(&mut remaining_slots, rng)?;
        slot_owners[picked as usize] = Some(RecordOwner::OriginatorFake);
    }
    // Pick slots for real hops in deterministic canonical order.
    let real_hop_iter: Vec<u8> = (0..real_hop_count).collect();
    for hop_index in real_hop_iter {
        let picked = pick_uniform_index(&mut remaining_slots, rng)?;
        slot_owners[picked as usize] = Some(RecordOwner::RealHop { hop_index });
    }
    // Any remaining slots become padding fakes.
    for owner in slot_owners.iter_mut().take(total as usize) {
        if owner.is_none() {
            *owner = Some(RecordOwner::PaddingFake);
        }
    }
    let mut slot_array: [Option<RecordOwner>; MAX_RECORD_COUNT as usize] = [None; 8];
    for (index, owner) in slot_owners
        .into_iter()
        .take(MAX_RECORD_COUNT as usize)
        .enumerate()
    {
        slot_array[index] = owner;
    }
    Ok(ShortBuildRecordSet {
        count: total,
        slots: slot_array,
    })
}

fn pick_uniform_index<R: CryptoRng + RngCore>(
    remaining: &mut Vec<u8>,
    rng: &mut R,
) -> Result<u8, MultiRecordError> {
    if remaining.is_empty() {
        return Err(MultiRecordError::SlotExhausted);
    }
    // Rejection-sample on `u32` to avoid modulo bias.
    let bound = remaining.len() as u32;
    let mask = next_power_of_two_mask(bound);
    loop {
        let r = rng.next_u32();
        let truncated = r & mask;
        if truncated < bound {
            let index = truncated as usize;
            return Ok(remaining.remove(index));
        }
    }
}

fn next_power_of_two_mask(value: u32) -> u32 {
    if value <= 1 {
        return 0;
    }
    let mut mask = 1_u32;
    while mask < value {
        mask = mask.wrapping_shl(1);
    }
    mask.wrapping_sub(1)
}

/// Typed inbound originator-fake material. The structure owns the
/// 16-byte truncated hash prefix, the 32-byte ephemeral X25519
/// public key, the 218-byte wire payload, and a creator-side
/// integrity hash the postprocessor compares against after
/// undoing the accumulated symmetric transforms.
pub struct OriginatorFake {
    /// Truncated identity-hash prefix placed at offset 0.
    pub hash_prefix: [u8; crate::build_crypto::HASH_PREFIX_LEN],
    /// Ephemeral X25519 public key placed at offset 16.
    pub ephemeral_pub: [u8; EPHEMERAL_KEY_LEN],
    /// Complete 218-byte wire payload.
    pub wire: [u8; RECORD_BYTES],
    /// Creator-side integrity hash recorded before dispatch.
    pub integrity_hash: [u8; 32],
}

impl fmt::Debug for OriginatorFake {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OriginatorFake")
            .field("hash_prefix", &self.hash_prefix)
            .field("ephemeral_pub", &self.ephemeral_pub)
            .field("wire", &"<redacted>")
            .field("integrity_hash", &"<redacted>")
            .finish()
    }
}

/// Builds the canonical 218-byte inbound originator-fake wire
/// record from the originator's identity hash and a freshly
/// generated ephemeral X25519 private key. The remaining 170 bytes
/// (after the 16-byte hash prefix and 32-byte ephemeral public
/// key) are filled with random bytes drawn from the supplied RNG.
/// The creator records an integrity hash over the complete wire
/// bytes so the postprocessor can detect modification after the
/// fake traverses the inbound tunnel.
pub fn build_originator_fake_record<R: CryptoRng + RngCore>(
    originator_hash: &Hash,
    rng: &mut R,
) -> Result<OriginatorFake, MultiRecordError> {
    let mut hash_prefix = [0_u8; crate::build_crypto::HASH_PREFIX_LEN];
    hash_prefix
        .copy_from_slice(&originator_hash.as_bytes()[..crate::build_crypto::HASH_PREFIX_LEN]);
    let mut ephemeral_priv = [0_u8; EPHEMERAL_KEY_LEN];
    rng.try_fill_bytes(&mut ephemeral_priv)
        .map_err(|_| MultiRecordError::RandomnessUnavailable)?;
    let ephemeral_pub = ephemeral_public(&ephemeral_priv);
    ephemeral_priv.zeroize();
    let mut wire = [0_u8; RECORD_BYTES];
    wire[..crate::build_crypto::HASH_PREFIX_LEN].copy_from_slice(&hash_prefix);
    wire[crate::build_crypto::HASH_PREFIX_LEN
        ..crate::build_crypto::HASH_PREFIX_LEN + EPHEMERAL_KEY_LEN]
        .copy_from_slice(&ephemeral_pub);
    let body_offset = crate::build_crypto::HASH_PREFIX_LEN + EPHEMERAL_KEY_LEN;
    let body_len = RECORD_BYTES - body_offset;
    let mut body = Zeroizing::new(vec![0_u8; body_len]);
    rng.try_fill_bytes(body.as_mut())
        .map_err(|_| MultiRecordError::RandomnessUnavailable)?;
    wire[body_offset..].copy_from_slice(body.as_ref());
    let integrity_hash = sha256_of(&wire);
    Ok(OriginatorFake {
        hash_prefix,
        ephemeral_pub,
        wire,
        integrity_hash,
    })
}

/// Recomputes the integrity hash of the supplied wire bytes and
/// compares against the recorded hash. The function returns
/// `Ok(())` when the hashes match and a typed
/// [`MultiRecordError::OriginatorFakeModified`] otherwise. The
/// caller must invoke this function on the postprocessed fake
/// record before the build is registered.
pub fn verify_originator_fake(
    fake: &OriginatorFake,
    wire_after_postprocess: &[u8],
) -> Result<(), MultiRecordError> {
    if wire_after_postprocess.len() != RECORD_BYTES {
        return Err(MultiRecordError::OriginatorFakeLengthMismatch {
            actual: wire_after_postprocess.len(),
            expected: RECORD_BYTES,
        });
    }
    let observed = sha256_of(wire_after_postprocess);
    if observed != fake.integrity_hash {
        return Err(MultiRecordError::OriginatorFakeModified);
    }
    Ok(())
}

/// Generates a generic 218-byte padding fake record. The bytes are
/// drawn entirely from the supplied RNG; the fake carries no
/// semantic content and is not interpreted by any hop.
pub fn build_padding_fake_record<R: CryptoRng + RngCore>(
    rng: &mut R,
) -> Result<[u8; RECORD_BYTES], MultiRecordError> {
    let mut wire = [0_u8; RECORD_BYTES];
    rng.try_fill_bytes(&mut wire)
        .map_err(|_| MultiRecordError::RandomnessUnavailable)?;
    Ok(wire)
}

/// Applies the canonical I2P raw-ChaCha20 transform to the
/// supplied 218-byte target record using the supplied hop's
/// derived `replyKey` and the target record slot. The IV is 12
/// bytes, zero in the first 4 bytes and at offset 5..11; the
/// record-slot byte lives at offset **4** of the IV
/// (the eight-byte little-endian nonce occupies bytes 4..11). The
/// transform is symmetric: applying the same call twice with the
/// same key and slot restores the original bytes.
pub fn chacha20_transform(
    reply_key: &[u8; CHACHA20_KEY_LEN],
    slot: SlotIndex,
    record: &mut [u8; RECORD_BYTES],
) -> Result<(), MultiRecordError> {
    chacha20_xor(reply_key, slot, record)
}

/// Lower-level helper that performs the raw ChaCha20 XOR against
/// the supplied in-place buffer using the supplied key and slot.
/// The function is exposed (instead of inlined into
/// [`chacha20_transform`]) so the postprocessor can reuse the same
/// primitive without re-importing the underlying cipher trait.
/// The IV follows the canonical record-slot encoding at offset 4.
pub fn chacha20_xor(
    key: &[u8; CHACHA20_KEY_LEN],
    slot: SlotIndex,
    record: &mut [u8; RECORD_BYTES],
) -> Result<(), MultiRecordError> {
    use chacha20::ChaCha20;
    let mut nonce = [0_u8; AEAD_NONCE_LEN];
    nonce[RECORD_SLOT_NONCE_OFFSET] = slot.get();
    let mut cipher = <ChaCha20 as KeyIvInit>::new(key.into(), &nonce.into());
    cipher.apply_keystream(record);
    Ok(())
}

/// Multi-record error taxonomy.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum MultiRecordError {
    /// The path declared no real hops.
    #[error("multi-record path declared no real hops")]
    EmptyPath,
    /// The hop count exceeded the documented maximum.
    #[error("multi-record hop count {actual} exceeds maximum {maximum}")]
    HopCountExceedsMaximum {
        /// Actual hop count.
        actual: u8,
        /// Maximum accepted hop count.
        maximum: u8,
    },
    /// The derived record count exceeded the documented maximum.
    #[error("multi-record count {actual} exceeds maximum {maximum}")]
    RecordCountExceedsMaximum {
        /// Actual record count.
        actual: u8,
        /// Maximum accepted record count.
        maximum: u8,
    },
    /// The slot allocator exhausted the available slots.
    #[error("multi-record slot allocator ran out of available slots")]
    SlotExhausted,
    /// The supplied RNG could not produce output.
    #[error("multi-record RNG unavailable")]
    RandomnessUnavailable,
    /// The originator fake record did not match its integrity hash.
    #[error("inbound originator fake record was modified after dispatch")]
    OriginatorFakeModified,
    /// The originator fake record carried the wrong number of bytes.
    #[error("originator fake record length {actual} does not match {expected}")]
    OriginatorFakeLengthMismatch {
        /// Actual length.
        actual: usize,
        /// Expected length.
        expected: usize,
    },
    /// The hop processor found zero slots whose 16-byte hash
    /// prefix matched the hop identity.
    #[error("hop did not find any record whose truncated hash prefix matched")]
    HopHashNotFound,
    /// The hop processor found multiple slots whose 16-byte hash
    /// prefix matched the hop identity.
    #[error("hop found multiple records whose truncated hash prefix matched")]
    DuplicateHopHash,
    /// The hop reply byte indicated a non-accepted response.
    #[error("hop rejected the build with response code {code}")]
    HopRejected {
        /// Rejected response code byte.
        code: u8,
    },
    /// Wrapped build cryptography error.
    #[error("multi-record cryptography rejected: {0}")]
    Cryptography(#[from] BuildCryptographyError),
    /// Wrapped short record error.
    #[error("multi-record short record rejected: {0}")]
    ShortRecord(#[from] crate::short_record::ShortBuildError),
}

fn sha256_of(input: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(input);
    let output = hasher.finalize();
    let mut out = [0_u8; 32];
    out.copy_from_slice(&output);
    out
}

fn ephemeral_public(priv_bytes: &[u8; EPHEMERAL_KEY_LEN]) -> [u8; EPHEMERAL_KEY_LEN] {
    let secret = x25519_dalek::StaticSecret::from(*priv_bytes);
    let public = x25519_dalek::PublicKey::from(&secret);
    public.to_bytes()
}

/// Per-hop crypto context the preprocessor retains so it can apply
/// the symmetric ChaCha20 transforms in the correct order before
/// the message is dispatched.
pub struct PreparedHopContext {
    /// Canonical hop index.
    pub hop_index: u8,
    /// The hop's 218-byte individually-sealed request record.
    pub own_record: [u8; RECORD_BYTES],
    /// The slot assigned to this hop in the record set.
    pub slot: SlotIndex,
    /// The post-request Noise state used to derive the layer keys.
    pub state: NoiseRequestState,
    /// The derived layer keys for this hop.
    pub layer_keys: LayerKeys,
    /// Hop role. Used by the postprocessor to surface the
    /// outbound-endpoint classification on the per-hop result.
    pub role: HopRole,
}

// Manual `Zeroize`/`ZeroizeOnDrop` implementation because `HopRole`
// is a public marker and does not implement `Zeroize` (it carries no
// secret material).
impl Zeroize for PreparedHopContext {
    fn zeroize(&mut self) {
        self.own_record.zeroize();
        self.state.zeroize();
        self.layer_keys.zeroize();
        // `slot` and `role` carry no secret material.
    }
}

impl ZeroizeOnDrop for PreparedHopContext {}

impl fmt::Debug for PreparedHopContext {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PreparedHopContext")
            .field("hop_index", &self.hop_index)
            .field("own_record", &"<redacted>")
            .field("slot", &self.slot)
            .field("state", &"<redacted>")
            .field("layer_keys", &"<redacted>")
            .field("role", &self.role)
            .finish()
    }
}

/// Prepared message ready for dispatch: per-hop contexts plus the
/// wire slots that hold padding and (for inbound) the originator
/// fake.
pub struct PreparedShortBuildMessage {
    /// Wire slot ownership map.
    pub record_set: ShortBuildRecordSet,
    /// Per-real-hop crypto contexts in canonical hop order.
    pub hop_contexts: Vec<PreparedHopContext>,
    /// Slot bytes that hold padding fakes (none if the path fills
    /// every slot with a real hop + inbound originator fake).
    pub padding_fakes: Vec<[u8; RECORD_BYTES]>,
    /// Inbound originator-fake material, when present.
    pub originator_fake: Option<OriginatorFake>,
    /// The total wire payload the message carries: `1 + count * 218`
    /// bytes starting with the one-byte count.
    pub payload: Zeroizing<Vec<u8>>,
    /// Direction the message was prepared for.
    pub direction: TunnelDirection,
    /// The first hop router hash a transport adapter should target.
    pub first_hop: Option<Hash>,
}

impl fmt::Debug for PreparedShortBuildMessage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PreparedShortBuildMessage")
            .field("record_set", &self.record_set)
            .field("hop_contexts", &"<redacted>")
            .field("padding_fakes", &self.padding_fakes.len())
            .field(
                "originator_fake",
                &self.originator_fake.as_ref().map(|_| "<redacted>"),
            )
            .field("payload", &format_args!("<{} bytes>", self.payload.len()))
            .field("direction", &self.direction)
            .field("first_hop", &self.first_hop)
            .finish()
    }
}

/// Prepares one multi-record short tunnel-build message from a
/// validated path and the supplied per-hop static encryption keys.
/// Each real hop is sealed individually through the Plan 109
/// EciesX25519 primitive; the per-hop records are then shuffled
/// into the assigned slots and the creator preprocessing applies
/// the iterative ChaCha20 transforms so each hop only sees its own
/// record at its stage.
#[allow(clippy::too_many_arguments)]
pub fn prepare_short_build_message<R: CryptoRng + RngCore>(
    cryptography: &EciesX25519BuildCryptography,
    path_hops: &[MultiRecordHopSpec<'_>],
    direction: TunnelDirection,
    creator_tunnel_id_bytes: [u8; 4],
    request_time_ms: u64,
    next_message_id: u32,
    first_hop: Option<Hash>,
    originator_hash: Option<&Hash>,
    rng: &mut R,
) -> Result<PreparedShortBuildMessage, MultiRecordError> {
    if path_hops.is_empty() {
        return Err(MultiRecordError::EmptyPath);
    }
    let record_set = assign_record_slots(path_hops.len() as u8, direction, rng)?;
    let mut hop_contexts: Vec<PreparedHopContext> = Vec::with_capacity(path_hops.len());
    for hop in path_hops {
        let slot = record_set
            .slot_for_real_hop(hop.canonical_index)
            .ok_or(MultiRecordError::SlotExhausted)?;
        let plaintext_array = build_hop_request_plaintext(
            hop,
            &creator_tunnel_id_bytes,
            request_time_ms,
            next_message_id,
        )?;
        let sealed = cryptography.seal_short_request(
            &plaintext_array,
            hop.static_encryption_key,
            hop.router_hash.as_bytes(),
            rng,
        )?;
        let state = sealed.state;
        let layer_keys = derive_layer_keys_for_hop(&state, hop.role)?;
        let mut own_record = [0_u8; RECORD_BYTES];
        own_record.copy_from_slice(sealed.record.as_ref());
        hop_contexts.push(PreparedHopContext {
            hop_index: hop.canonical_index,
            own_record,
            slot,
            state,
            layer_keys,
            role: hop.role,
        });
    }
    // Build the wire payload slot table.
    let mut slots: Vec<[u8; RECORD_BYTES]> =
        vec![[0_u8; RECORD_BYTES]; record_set.record_count() as usize];
    let mut padding_fakes: Vec<[u8; RECORD_BYTES]> = Vec::new();
    for context in &hop_contexts {
        slots[context.slot.get() as usize] = context.own_record;
    }
    let mut originator_fake: Option<OriginatorFake> = None;
    for index in 0..record_set.record_count() {
        let owner = record_set.owner_at_slot(SlotIndex::new(index).expect("validated"));
        match owner {
            Some(RecordOwner::OriginatorFake) => {
                let hash = originator_hash.ok_or(MultiRecordError::EmptyPath)?;
                let fake = build_originator_fake_record(hash, rng)?;
                slots[index as usize] = fake.wire;
                originator_fake = Some(fake);
            }
            Some(RecordOwner::PaddingFake) => {
                let fake = build_padding_fake_record(rng)?;
                slots[index as usize] = fake;
                padding_fakes.push(fake);
            }
            Some(RecordOwner::RealHop { .. }) => {}
            None => {
                return Err(MultiRecordError::SlotExhausted);
            }
        }
    }
    // Apply creator request preprocessing.
    preprocess_creator_request(&record_set, &hop_contexts, &mut slots)?;
    let payload = encode_short_tunnel_build_payload(&record_set, &slots)?;
    Ok(PreparedShortBuildMessage {
        record_set,
        hop_contexts,
        padding_fakes,
        originator_fake,
        payload: Zeroizing::new(payload),
        direction,
        first_hop,
    })
}

/// Hop specification consumed by [`prepare_short_build_message`].
pub struct MultiRecordHopSpec<'a> {
    /// Canonical hop index in the path.
    pub canonical_index: u8,
    /// Hop router hash.
    pub router_hash: &'a Hash,
    /// Hop X25519 static encryption key.
    pub static_encryption_key: &'a [u8; EPHEMERAL_KEY_LEN],
    /// Hop role.
    pub role: HopRole,
    /// Explicit per-hop receive tunnel identifier. Plan 111
    /// defect 6: each hop has its own nonzero receive tunnel id
    /// that is independent from any router hash and from any other
    /// hop's id.
    pub receive_tunnel: crate::identity::TunnelId,
    /// Explicit per-hop next tunnel identifier. The value is
    /// independent from the next router hash and from any other
    /// hop's id.
    pub next_tunnel: crate::identity::TunnelId,
    /// Next-hop router hash for the request plaintext. The
    /// terminal hop carries the upstream hop's router hash; the
    /// creator is responsible for supplying an explicit value
    /// rather than synthesising one from a prefix.
    pub next_router_hash: &'a Hash,
}

fn build_hop_request_plaintext(
    hop: &MultiRecordHopSpec<'_>,
    _creator_tunnel_id_bytes: &[u8; 4],
    request_time_ms: u64,
    next_message_id: u32,
) -> Result<[u8; SHORT_REQUEST_PLAINTEXT_SIZE], MultiRecordError> {
    let receive_tunnel = hop.receive_tunnel;
    let next_tunnel = hop.next_tunnel;
    let next_router = *hop.next_router_hash;
    let request_time = i2pr_proto::Date::from_millis(request_time_ms);
    let record = ShortRequestRecord::try_new(
        receive_tunnel,
        next_tunnel,
        next_router,
        hop.role,
        crate::short_record::LayerEncryptionType::Aes,
        request_time,
        REQUEST_EXPIRATION_SECONDS,
        next_message_id,
        BuildOptions::empty(),
    )?;
    let encoded = record.encode()?;
    let mut out = [0_u8; SHORT_REQUEST_PLAINTEXT_SIZE];
    out.copy_from_slice(encoded.as_ref());
    Ok(out)
}

fn derive_layer_keys_for_hop(
    state: &NoiseRequestState,
    role: HopRole,
) -> Result<LayerKeys, BuildCryptographyError> {
    let is_obep = matches!(role, HopRole::OutboundEndpoint);
    crate::build_crypto::derive_layer_keys(state, is_obep)
}

fn preprocess_creator_request(
    record_set: &ShortBuildRecordSet,
    contexts: &[PreparedHopContext],
    slots: &mut [[u8; RECORD_BYTES]],
) -> Result<(), MultiRecordError> {
    for context in contexts {
        let hop_index = context.hop_index;
        let target_slot = context.slot;
        for prior_context in contexts.iter().filter(|other| other.hop_index < hop_index) {
            chacha20_transform(
                prior_context.layer_keys.reply_key(),
                target_slot,
                &mut slots[target_slot.get() as usize],
            )?;
        }
    }
    let _ = record_set;
    Ok(())
}

/// Encodes the canonical `1 + count * 218`-byte STBM payload from
/// the supplied slot bytes. The function validates the count and
/// refuses unknown/trailing input.
pub fn encode_short_tunnel_build_payload(
    record_set: &ShortBuildRecordSet,
    slots: &[[u8; RECORD_BYTES]],
) -> Result<Vec<u8>, MultiRecordError> {
    let count = record_set.record_count();
    if count == 0 {
        return Err(MultiRecordError::RecordCountExceedsMaximum {
            actual: 0,
            maximum: MAX_RECORD_COUNT,
        });
    }
    if count > MAX_RECORD_COUNT {
        return Err(MultiRecordError::RecordCountExceedsMaximum {
            actual: count,
            maximum: MAX_RECORD_COUNT,
        });
    }
    if slots.len() != count as usize {
        return Err(MultiRecordError::SlotExhausted);
    }
    let mut out = Vec::with_capacity(1 + count as usize * RECORD_BYTES);
    out.push(count);
    for slot in slots {
        out.extend_from_slice(slot);
    }
    Ok(out)
}

/// Decodes the canonical STBM payload back into a count plus the
/// 218-byte records. The decoder refuses trailing bytes,
/// truncated input, count = 0, and count > 8.
pub fn decode_short_tunnel_build_payload(
    payload: &[u8],
) -> Result<(u8, Vec<[u8; RECORD_BYTES]>), MultiRecordError> {
    if payload.is_empty() {
        return Err(MultiRecordError::RecordCountExceedsMaximum {
            actual: 0,
            maximum: MAX_RECORD_COUNT,
        });
    }
    let count = payload[0];
    if count == 0 {
        return Err(MultiRecordError::RecordCountExceedsMaximum {
            actual: 0,
            maximum: MAX_RECORD_COUNT,
        });
    }
    if count > MAX_RECORD_COUNT {
        return Err(MultiRecordError::RecordCountExceedsMaximum {
            actual: count,
            maximum: MAX_RECORD_COUNT,
        });
    }
    let expected_len = 1 + count as usize * RECORD_BYTES;
    if payload.len() != expected_len {
        return Err(MultiRecordError::RecordCountExceedsMaximum {
            actual: count,
            maximum: MAX_RECORD_COUNT,
        });
    }
    let mut slots = Vec::with_capacity(count as usize);
    for index in 0..count as usize {
        let start = 1 + index * RECORD_BYTES;
        let end = start + RECORD_BYTES;
        let mut slot = [0_u8; RECORD_BYTES];
        slot.copy_from_slice(&payload[start..end]);
        slots.push(slot);
    }
    Ok((count, slots))
}

/// Encodes the canonical OTBRM payload from the supplied reply
/// records. The frame format is identical to the STBM frame:
/// one-byte count followed by `count * 218` reply bytes.
pub fn encode_outbound_tunnel_build_reply(
    count: u8,
    records: &[[u8; RECORD_BYTES]],
) -> Result<Vec<u8>, MultiRecordError> {
    if count == 0 {
        return Err(MultiRecordError::RecordCountExceedsMaximum {
            actual: 0,
            maximum: MAX_RECORD_COUNT,
        });
    }
    if count > MAX_RECORD_COUNT {
        return Err(MultiRecordError::RecordCountExceedsMaximum {
            actual: count,
            maximum: MAX_RECORD_COUNT,
        });
    }
    if records.len() != count as usize {
        return Err(MultiRecordError::SlotExhausted);
    }
    let mut out = Vec::with_capacity(1 + count as usize * RECORD_BYTES);
    out.push(count);
    for record in records {
        out.extend_from_slice(record);
    }
    Ok(out)
}

/// Decodes the canonical OTBRM payload back into the count and
/// the 218-byte reply records. The decoder enforces the same
/// rules as the STBM decoder.
pub fn decode_outbound_tunnel_build_reply(
    payload: &[u8],
) -> Result<(u8, Vec<[u8; RECORD_BYTES]>), MultiRecordError> {
    decode_short_tunnel_build_payload(payload)
}

/// Per-hop processing result. The struct is consumed by the
/// [`CreatorReplyPostprocessor`] once every real hop has
/// processed its stage.
#[derive(Debug)]
pub struct ProcessedHopResult {
    /// Canonical hop index.
    pub hop_index: u8,
    /// The slot the hop was assigned.
    pub slot: SlotIndex,
    /// The decrypted 202-byte reply plaintext.
    pub plaintext: Zeroizing<Vec<u8>>,
    /// The response code byte the hop returned.
    pub response_code: ShortResponseCode,
    /// Whether the authenticated hop plaintext declared the
    /// outbound-endpoint role. Used by the registry/registrar to
    /// route through the OBEP continuation path.
    pub is_obep: bool,
}

/// Standalone message-level hop processor. The processor is the
/// Plan 110 replacement for the Plan 109 single-record
/// `DeterministicResponder`; it consumes one decoded STBM payload,
/// finds the slot whose 16-byte hash prefix matches the hop
/// identity, opens the hop's own request, derives layer keys, seals
/// a reply, and transforms every other record with the hop's
/// derived `replyKey` and the target slot's IV. The processor
/// returns the postprocessed payload and the hop's own
/// [`ProcessedHopResult`].
pub struct MessageHopProcessor;

impl MessageHopProcessor {
    /// Processes one hop's stage against the supplied payload. The
    /// caller supplies the static private X25519 key for the hop
    /// and the hop's identity hash.
    #[allow(clippy::too_many_arguments)]
    pub fn process_hop(
        cryptography: &EciesX25519BuildCryptography,
        payload: &[u8],
        hop_static_priv: &[u8; EPHEMERAL_KEY_LEN],
        hop_identity: &Hash,
        response_code: ShortResponseCode,
    ) -> Result<(Vec<u8>, ProcessedHopResult), MultiRecordError> {
        let (count, mut slots) = decode_short_tunnel_build_payload(payload)?;
        // Find the unique slot whose 16-byte hash prefix matches
        // the hop's identity.
        let mut matches: Vec<u8> = Vec::new();
        for (index, slot) in slots.iter().enumerate() {
            if slot[..crate::build_crypto::HASH_PREFIX_LEN]
                == hop_identity.as_bytes()[..crate::build_crypto::HASH_PREFIX_LEN]
            {
                matches.push(index as u8);
            }
        }
        if matches.is_empty() {
            return Err(MultiRecordError::HopHashNotFound);
        }
        if matches.len() > 1 {
            return Err(MultiRecordError::DuplicateHopHash);
        }
        let slot_index = matches[0];
        let slot = SlotIndex::new(slot_index).expect("validated");
        let opened: OpenedShortRequest = cryptography.open_short_request(
            &slots[slot_index as usize],
            hop_static_priv,
            hop_identity.as_bytes(),
        )?;
        // Plan 111 defect 7: the authenticated hop role is
        // decoded from the request plaintext rather than flattened
        // to participant.
        let decoded_record = ShortRequestRecord::decode(opened.plaintext.as_ref())
            .map_err(MultiRecordError::ShortRecord)?;
        let is_obep = matches!(decoded_record.role(), HopRole::OutboundEndpoint);
        let layer_keys = derive_layer_keys_for_hop(&opened.state, decoded_record.role())?;
        let reply = ShortReplyRecord::new(BuildOptions::empty(), response_code);
        let plaintext_vec = reply.encode();
        let mut plaintext = [0_u8; SHORT_REPLY_PLAINTEXT_SIZE];
        plaintext.copy_from_slice(plaintext_vec.as_ref());
        let sealed_reply = cryptography.seal_short_reply(
            &plaintext,
            &layer_keys,
            &opened.state.transcript_hash(),
            slot,
        )?;
        slots[slot_index as usize] = sealed_reply;
        // Transform every other record.
        for (index, other_slot) in slots.iter_mut().enumerate() {
            if index as u8 == slot_index {
                continue;
            }
            let target_slot = SlotIndex::new(index as u8).expect("validated");
            chacha20_transform(layer_keys.reply_key(), target_slot, other_slot)?;
        }
        let new_payload = encode_short_tunnel_build_payload_from_count(count, &slots)?;
        Ok((
            new_payload,
            ProcessedHopResult {
                hop_index: slot_index,
                slot,
                plaintext: Zeroizing::new(plaintext.to_vec()),
                response_code,
                is_obep,
            },
        ))
    }
}

fn encode_short_tunnel_build_payload_from_count(
    count: u8,
    slots: &[[u8; RECORD_BYTES]],
) -> Result<Vec<u8>, MultiRecordError> {
    if count == 0 || count > MAX_RECORD_COUNT {
        return Err(MultiRecordError::RecordCountExceedsMaximum {
            actual: count,
            maximum: MAX_RECORD_COUNT,
        });
    }
    if slots.len() != count as usize {
        return Err(MultiRecordError::SlotExhausted);
    }
    let mut out = Vec::with_capacity(1 + count as usize * RECORD_BYTES);
    out.push(count);
    for slot in slots {
        out.extend_from_slice(slot);
    }
    Ok(out)
}

/// Postprocessor that undoes the accumulated later-hop ChaCha20
/// transforms and authenticates every real-hop reply. The
/// postprocessor uses the canonical reply key/slot/saved-h triple
/// the build cryptography expects; a single reply whose slot, key,
/// or saved `h` does not match fails the entire build.
pub struct CreatorReplyPostprocessor;

impl CreatorReplyPostprocessor {
    /// Authenticates every real-hop reply, undoing the accumulated
    /// symmetric transforms first. The function returns the
    /// per-hop [`ProcessedHopResult`] list in canonical hop order
    /// and verifies the inbound originator fake when the build was
    /// inbound.
    #[allow(clippy::too_many_arguments)]
    pub fn process_reply(
        cryptography: &EciesX25519BuildCryptography,
        contexts: &[PreparedHopContext],
        record_set: &ShortBuildRecordSet,
        reply_payload: &[u8],
        originator_fake: Option<&OriginatorFake>,
    ) -> Result<Vec<ProcessedHopResult>, MultiRecordError> {
        let (count, mut slots) = decode_short_tunnel_build_payload(reply_payload)?;
        let expected_count = record_set.record_count();
        if count != expected_count {
            return Err(MultiRecordError::SlotExhausted);
        }
        // First, undo the symmetric transforms for each real hop
        // using every later real hop's reply key and the target
        // hop's slot.
        let mut ordered: Vec<&PreparedHopContext> = contexts.iter().collect();
        ordered.sort_by_key(|context| context.hop_index);
        for (i, target) in ordered.iter().enumerate() {
            for later in ordered.iter().skip(i + 1) {
                chacha20_transform(
                    later.layer_keys.reply_key(),
                    target.slot,
                    &mut slots[target.slot.get() as usize],
                )?;
            }
        }
        // Also undo the symmetric transforms applied to the
        // inbound originator fake slot by every real hop. The
        // postprocessor uses the slot byte of the fake itself as
        // the IV slot for each undo step.
        if let Some(fake) = originator_fake {
            let fake_slot = (0..record_set.record_count())
                .find(|index| {
                    record_set.is_originator_fake_slot(SlotIndex::new(*index).expect("ok"))
                })
                .ok_or(MultiRecordError::SlotExhausted)?;
            let fake_slot_index = SlotIndex::new(fake_slot).expect("validated");
            for hop in &ordered {
                chacha20_transform(
                    hop.layer_keys.reply_key(),
                    fake_slot_index,
                    &mut slots[fake_slot as usize],
                )?;
            }
            let wire = &slots[fake_slot as usize];
            verify_originator_fake(fake, wire)?;
        }
        // Then open every real-hop reply in canonical hop order.
        let mut results = Vec::with_capacity(ordered.len());
        for context in &ordered {
            let slot = context.slot;
            let slot_bytes = &slots[slot.get() as usize];
            let plaintext = cryptography.open_short_reply(
                slot_bytes,
                &context.layer_keys,
                &context.state.transcript_hash(),
                slot,
            )?;
            let reply = ShortReplyRecord::decode(plaintext.as_ref())?;
            results.push(ProcessedHopResult {
                hop_index: context.hop_index,
                slot,
                plaintext: Zeroizing::new(plaintext.to_vec()),
                response_code: reply.response(),
                is_obep: context.role == HopRole::OutboundEndpoint,
            });
        }
        Ok(results)
    }
}

/// Reference fixture for the multi-hop conformance test. The
/// fixture is built by a small specification-shaped reference
/// implementation that does not call the production
/// [`EciesX25519BuildCryptography`] primitives; it derives the
/// Noise transcript, the ChaCha20-Poly1305 AEAD, the post-`MixKey`
/// KDF chain, the raw ChaCha20 transform, and the ChaCha20-Poly1305
/// reply AEAD independently. The fixture proves byte-for-byte
/// parity against the production primitive.
pub struct MultiHopReferenceFixture;

impl MultiHopReferenceFixture {
    /// Generates a complete reference trajectory for a three-real-hop
    /// outbound path with one padding fake. The function returns
    /// the wire payload the creator would emit, the expected
    /// postprocessed payload after every hop, and the expected
    /// reply plaintext per hop.
    #[allow(clippy::too_many_arguments)]
    pub fn three_hop_one_fake(
        seed: u64,
        originator_hash: &Hash,
    ) -> Result<MultiHopFixture, MultiRecordError> {
        let mut rng = crate::conformance_fixtures::DeterministicRng::new([0x42_u8; 32]);
        // Per-hop static private keys.
        let hop0_priv = fixture_privkey(seed.wrapping_add(1));
        let hop1_priv = fixture_privkey(seed.wrapping_add(2));
        let hop2_priv = fixture_privkey(seed.wrapping_add(3));
        let hop0_pub = ephemeral_public(&hop0_priv);
        let hop1_pub = ephemeral_public(&hop1_priv);
        let hop2_pub = ephemeral_public(&hop2_priv);
        let hop0_hash = fixture_hash(seed.wrapping_add(1));
        let hop1_hash = fixture_hash(seed.wrapping_add(2));
        let hop2_hash = fixture_hash(seed.wrapping_add(3));
        let record_set = assign_record_slots(3, TunnelDirection::Outbound, &mut rng)?;
        // Seal each hop individually with a fixed ephemeral key so
        // the fixture is deterministic without depending on the
        // production primitive's RNG path.
        let eph0 = fixture_privkey(seed.wrapping_add(101));
        let eph1 = fixture_privkey(seed.wrapping_add(102));
        let eph2 = fixture_privkey(seed.wrapping_add(103));
        let cryptography = EciesX25519BuildCryptography::new();
        let plaintext0 = fixture_request_plaintext(
            [0x10, 0x00, 0x00, 0x01],
            HopRole::InboundGateway,
            &hop1_hash,
        )?;
        let plaintext1 =
            fixture_request_plaintext([0x10, 0x00, 0x00, 0x02], HopRole::Participant, &hop2_hash)?;
        let plaintext2 = fixture_request_plaintext(
            [0x10, 0x00, 0x00, 0x03],
            HopRole::OutboundEndpoint,
            &hop1_hash,
        )?;
        let sealed0 = cryptography.seal_short_request_with_ephemeral(
            &plaintext0,
            &hop0_pub,
            hop0_hash.as_bytes(),
            &eph0,
        )?;
        let sealed1 = cryptography.seal_short_request_with_ephemeral(
            &plaintext1,
            &hop1_pub,
            hop1_hash.as_bytes(),
            &eph1,
        )?;
        let sealed2 = cryptography.seal_short_request_with_ephemeral(
            &plaintext2,
            &hop2_pub,
            hop2_hash.as_bytes(),
            &eph2,
        )?;
        let layer0 = derive_layer_keys_for_hop(&sealed0.state, HopRole::InboundGateway)?;
        let layer1 = derive_layer_keys_for_hop(&sealed1.state, HopRole::Participant)?;
        let layer2 = derive_layer_keys_for_hop(&sealed2.state, HopRole::OutboundEndpoint)?;
        let mut record0 = [0_u8; RECORD_BYTES];
        record0.copy_from_slice(sealed0.record.as_ref());
        let mut record1 = [0_u8; RECORD_BYTES];
        record1.copy_from_slice(sealed1.record.as_ref());
        let mut record2 = [0_u8; RECORD_BYTES];
        record2.copy_from_slice(sealed2.record.as_ref());
        let context0 = PreparedHopContext {
            hop_index: 0,
            own_record: record0,
            slot: record_set.slot_for_real_hop(0).expect("slot"),
            state: sealed0.state,
            layer_keys: layer0,
            role: HopRole::InboundGateway,
        };
        let context1 = PreparedHopContext {
            hop_index: 1,
            own_record: record1,
            slot: record_set.slot_for_real_hop(1).expect("slot"),
            state: sealed1.state,
            layer_keys: layer1,
            role: HopRole::Participant,
        };
        let context2 = PreparedHopContext {
            hop_index: 2,
            own_record: record2,
            slot: record_set.slot_for_real_hop(2).expect("slot"),
            state: sealed2.state,
            layer_keys: layer2,
            role: HopRole::OutboundEndpoint,
        };
        let contexts = vec![context0, context1, context2];
        let mut slots = vec![[0_u8; RECORD_BYTES]; record_set.record_count() as usize];
        for context in &contexts {
            slots[context.slot.get() as usize] = context.own_record;
        }
        // Fill remaining slots with padding fakes.
        for index in 0..record_set.record_count() {
            let owner = record_set.owner_at_slot(SlotIndex::new(index).expect("ok"));
            if matches!(owner, Some(RecordOwner::PaddingFake)) {
                slots[index as usize] = build_padding_fake_record(&mut rng)?;
            }
        }
        let initial_payload = encode_short_tunnel_build_payload(&record_set, &slots)?;
        // Apply creator request preprocessing.
        preprocess_creator_request(&record_set, &contexts, &mut slots)?;
        let preprocessed_payload = encode_short_tunnel_build_payload(&record_set, &slots)?;
        // Process each hop.
        let mut current_payload = preprocessed_payload.clone();
        for context in &contexts {
            let (hop_priv, hop_hash) = match context.hop_index {
                0 => (hop0_priv, hop0_hash),
                1 => (hop1_priv, hop1_hash),
                2 => (hop2_priv, hop2_hash),
                _ => unreachable!(),
            };
            let (new_payload, _result) = MessageHopProcessor::process_hop(
                &cryptography,
                &current_payload,
                &hop_priv,
                &hop_hash,
                ShortResponseCode::Accepted,
            )?;
            current_payload = new_payload;
        }
        // Postprocess the replies.
        let post = CreatorReplyPostprocessor::process_reply(
            &cryptography,
            &contexts,
            &record_set,
            &current_payload,
            None,
        )?;
        let _ = originator_hash;
        Ok(MultiHopFixture {
            record_set,
            contexts,
            initial_payload,
            preprocessed_payload,
            post_hop_payload: current_payload,
            results: post,
        })
    }
}

/// Output of [`MultiHopReferenceFixture::three_hop_one_fake`].
#[derive(Debug)]
pub struct MultiHopFixture {
    /// Wire slot ownership map.
    pub record_set: ShortBuildRecordSet,
    /// Per-real-hop contexts in canonical hop order.
    pub contexts: Vec<PreparedHopContext>,
    /// The wire payload the creator emitted before preprocessing.
    pub initial_payload: Vec<u8>,
    /// The wire payload the creator emitted after preprocessing.
    pub preprocessed_payload: Vec<u8>,
    /// The wire payload the simulated hops produced.
    pub post_hop_payload: Vec<u8>,
    /// The processed per-hop reply results.
    pub results: Vec<ProcessedHopResult>,
}

fn fixture_privkey(seed: u64) -> [u8; EPHEMERAL_KEY_LEN] {
    let mut bytes = [0_u8; EPHEMERAL_KEY_LEN];
    let mut cursor = seed;
    for byte in bytes.iter_mut() {
        cursor = (cursor
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407))
            & 0xFFFF;
        *byte = cursor as u8;
    }
    bytes
}

fn fixture_hash(seed: u64) -> Hash {
    let mut bytes = [0_u8; 32];
    let mut cursor = seed;
    for byte in bytes.iter_mut() {
        cursor = (cursor
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407))
            & 0xFFFF;
        *byte = cursor as u8;
    }
    Hash::from_bytes(bytes)
}

fn fixture_request_plaintext(
    creator_tunnel_id_bytes: [u8; 4],
    role: HopRole,
    next_router: &Hash,
) -> Result<[u8; SHORT_REQUEST_PLAINTEXT_SIZE], MultiRecordError> {
    let receive_tunnel = u32::from_be_bytes(creator_tunnel_id_bytes);
    let next_tunnel = u32::from_be_bytes([
        next_router.as_bytes()[0],
        next_router.as_bytes()[1],
        next_router.as_bytes()[2],
        next_router.as_bytes()[3],
    ]);
    let record = ShortRequestRecord::try_new(
        crate::identity::TunnelId::new(receive_tunnel).expect("id"),
        crate::identity::TunnelId::new(next_tunnel).expect("id"),
        *next_router,
        role,
        crate::short_record::LayerEncryptionType::Aes,
        i2pr_proto::Date::from_millis(60_000),
        REQUEST_EXPIRATION_SECONDS,
        0xABCD_1234,
        BuildOptions::empty(),
    )?;
    let encoded = record.encode()?;
    let mut out = [0_u8; SHORT_REQUEST_PLAINTEXT_SIZE];
    out.copy_from_slice(encoded.as_ref());
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::TunnelId;
    use crate::short_record::LayerEncryptionType;
    use rand_chacha::ChaCha8Rng;
    use rand_core::SeedableRng;

    #[test]
    fn min_record_count_policy() {
        assert_eq!(
            build_minimum_record_count(1, TunnelDirection::Outbound).expect("ok"),
            MIN_PRODUCTION_RECORD_COUNT
        );
        assert_eq!(
            build_minimum_record_count(3, TunnelDirection::Outbound).expect("ok"),
            4
        );
        assert_eq!(
            build_minimum_record_count(4, TunnelDirection::Outbound).expect("ok"),
            4
        );
        assert_eq!(
            build_minimum_record_count(1, TunnelDirection::Inbound).expect("ok"),
            4
        );
        assert_eq!(
            build_minimum_record_count(3, TunnelDirection::Inbound).expect("ok"),
            4
        );
        assert!(build_minimum_record_count(9, TunnelDirection::Outbound).is_err());
    }

    #[test]
    fn slot_assignment_is_unique_and_complete() {
        let mut rng = ChaCha8Rng::seed_from_u64(1);
        let set = assign_record_slots(3, TunnelDirection::Outbound, &mut rng).expect("set");
        assert_eq!(set.record_count(), 4);
        assert!(set.is_complete(3));
        let mut seen_slots = Vec::new();
        for index in 0..set.record_count() {
            assert!(seen_slots.iter().all(|value| *value != index));
            seen_slots.push(index);
        }
    }

    #[test]
    fn inbound_set_includes_originator_fake() {
        let mut rng = ChaCha8Rng::seed_from_u64(2);
        let set = assign_record_slots(2, TunnelDirection::Inbound, &mut rng).expect("set");
        assert_eq!(set.record_count(), 4);
        let originator_count = (0..set.record_count())
            .filter(|index| set.is_originator_fake_slot(SlotIndex::new(*index).expect("ok")))
            .count();
        assert_eq!(originator_count, 1);
        assert!(set.is_complete(2));
    }

    #[test]
    fn slot_assignment_is_deterministic_per_seed() {
        let mut rng_a = ChaCha8Rng::seed_from_u64(3);
        let mut rng_b = ChaCha8Rng::seed_from_u64(3);
        let a = assign_record_slots(3, TunnelDirection::Outbound, &mut rng_a).expect("a");
        let b = assign_record_slots(3, TunnelDirection::Outbound, &mut rng_b).expect("b");
        for index in 0..a.record_count() {
            assert_eq!(
                a.owner_at_slot(SlotIndex::new(index).expect("ok")),
                b.owner_at_slot(SlotIndex::new(index).expect("ok"))
            );
        }
    }

    #[test]
    fn chacha20_transform_is_symmetric() {
        let key = [0x42_u8; 32];
        let slot = SlotIndex::new(3).expect("ok");
        let original = [0x99_u8; RECORD_BYTES];
        let mut copy = original;
        chacha20_transform(&key, slot, &mut copy).expect("forward");
        assert_ne!(copy, original);
        chacha20_transform(&key, slot, &mut copy).expect("reverse");
        assert_eq!(copy, original);
    }

    #[test]
    fn chacha20_transform_with_different_slots_yields_different_output() {
        let key = [0x55_u8; 32];
        let slot_zero = SlotIndex::new(0).expect("ok");
        let slot_one = SlotIndex::new(1).expect("ok");
        let mut a = [0x66_u8; RECORD_BYTES];
        let mut b = a;
        chacha20_transform(&key, slot_zero, &mut a).expect("a");
        chacha20_transform(&key, slot_one, &mut b).expect("b");
        assert_ne!(a, b);
    }

    #[test]
    fn chacha20_transform_nonzero_slot_changes_iv() {
        // Plan 111 defect 3: a nonzero slot byte at offset 4 must
        // produce a different first keystream block from a zero
        // slot byte; the transform must not place the slot byte at
        // offset 11. The first keystream block difference is
        // observable as a different output for two otherwise equal
        // inputs.
        use chacha20::ChaCha20;
        use chacha20::cipher::{KeyIvInit, StreamCipher};

        let key = [0x77_u8; 32];
        let slot_zero = SlotIndex::new(0).expect("ok");
        let slot_four = SlotIndex::new(4).expect("ok");
        let mut buf_zero = [0x66_u8; 64];
        let mut buf_four = buf_zero;

        let mut nonce_zero = [0_u8; 12];
        nonce_zero[RECORD_SLOT_NONCE_OFFSET] = slot_zero.get();
        let mut cipher_zero = <ChaCha20 as KeyIvInit>::new((&key).into(), (&nonce_zero).into());
        cipher_zero.apply_keystream(&mut buf_zero);

        let mut nonce_four = [0_u8; 12];
        nonce_four[RECORD_SLOT_NONCE_OFFSET] = slot_four.get();
        let mut cipher_four = <ChaCha20 as KeyIvInit>::new((&key).into(), (&nonce_four).into());
        cipher_four.apply_keystream(&mut buf_four);

        assert_ne!(buf_zero, buf_four);

        // And the bad placement at offset 11 must produce a
        // strictly different keystream that does NOT match the
        // production chacha20_xor result.
        let mut bad_nonce = [0_u8; 12];
        bad_nonce[11] = slot_four.get();
        let mut buf_bad = [0x66_u8; 64];
        let mut cipher_bad = <ChaCha20 as KeyIvInit>::new((&key).into(), (&bad_nonce).into());
        cipher_bad.apply_keystream(&mut buf_bad);
        assert_ne!(buf_bad, buf_four);
    }

    #[test]
    fn role_aware_processor_decodes_authenticated_role() {
        // Plan 111 defect 7: the per-hop processor must decode
        // the role from the authenticated request plaintext
        // rather than flatten to participant. An OBEP role in the
        // authenticated plaintext must surface as
        // `is_obep = true` on the per-hop result.
        let cryptography = EciesX25519BuildCryptography::new();
        let hop0_priv = {
            let mut bytes = [0x55u8; 32];
            bytes[0..8].copy_from_slice(&1_u64.to_le_bytes());
            bytes
        };
        let hop1_priv = {
            let mut bytes = [0x77u8; 32];
            bytes[0..8].copy_from_slice(&2_u64.to_le_bytes());
            bytes
        };
        let hop0_pub = ephemeral_public(&hop0_priv);
        let hop1_pub = ephemeral_public(&hop1_priv);
        let hop0_hash = Hash::from_bytes([0xAA; 32]);
        let hop1_hash = Hash::from_bytes([0xBB; 32]);
        let hop0_eph = {
            let mut bytes = [0x33u8; 32];
            bytes[0..8].copy_from_slice(&101_u64.to_le_bytes());
            bytes
        };
        let hop1_eph = {
            let mut bytes = [0x44u8; 32];
            bytes[0..8].copy_from_slice(&102_u64.to_le_bytes());
            bytes
        };

        let plaintext0: [u8; SHORT_REQUEST_PLAINTEXT_SIZE] = {
            let mut bytes = [0u8; SHORT_REQUEST_PLAINTEXT_SIZE];
            // First 4 bytes: receive tunnel id.
            bytes[0..4].copy_from_slice(&0x1000_u32.to_be_bytes());
            // Next 4 bytes: next tunnel id.
            bytes[4..8].copy_from_slice(&0x2000_u32.to_be_bytes());
            // Next 32 bytes: next router hash.
            bytes[8..40].copy_from_slice(hop1_hash.as_bytes());
            // Role flag at offset 40: OBEP = 0x40.
            bytes[40] = HopRole::OutboundEndpoint.flag();
            // Bytes 41..43: zero.
            // Layer encryption type at offset 43: AES = 0x00.
            bytes[43] = LayerEncryptionType::Aes.byte();
            // Request time minutes at offset 44..48: 1 minute.
            bytes[44..48].copy_from_slice(&1_u32.to_be_bytes());
            // Expiration at offset 48..52: 600 seconds.
            bytes[48..52].copy_from_slice(&REQUEST_EXPIRATION_SECONDS.to_be_bytes());
            // Message id at offset 52..56: nonzero.
            bytes[52..56].copy_from_slice(&0xABCD_1234_u32.to_be_bytes());
            // Mapping at offset 56: two-byte empty.
            bytes
        };
        let sealed0 = cryptography
            .seal_short_request_with_ephemeral(
                &plaintext0,
                &hop0_pub,
                hop0_hash.as_bytes(),
                &hop0_eph,
            )
            .expect("seal obep");

        let plaintext1: [u8; SHORT_REQUEST_PLAINTEXT_SIZE] = {
            let mut bytes = plaintext0;
            bytes[40] = HopRole::Participant.flag();
            bytes
        };
        let sealed1 = cryptography
            .seal_short_request_with_ephemeral(
                &plaintext1,
                &hop1_pub,
                hop1_hash.as_bytes(),
                &hop1_eph,
            )
            .expect("seal participant");

        let mut record0 = [0u8; RECORD_BYTES];
        record0.copy_from_slice(sealed0.record.as_ref());
        let mut record1 = [0u8; RECORD_BYTES];
        record1.copy_from_slice(sealed1.record.as_ref());

        // Build a 2-record payload manually.
        let mut slots = vec![[0u8; RECORD_BYTES]; 2];
        slots[0] = record0;
        slots[1] = record1;
        let payload = encode_short_tunnel_build_payload_from_count(2, &slots).expect("payload");

        // Process hop0 (OBEP) using only its static key.
        let (_new_payload, result_obep) = MessageHopProcessor::process_hop(
            &cryptography,
            &payload,
            &hop0_priv,
            &hop0_hash,
            ShortResponseCode::Accepted,
        )
        .expect("process hop0");
        assert!(
            result_obep.is_obep,
            "OBEP role must surface as is_obep=true"
        );

        // Process hop1 (participant) using only its static key.
        let (_new_payload2, result_part) = MessageHopProcessor::process_hop(
            &cryptography,
            &payload,
            &hop1_priv,
            &hop1_hash,
            ShortResponseCode::Accepted,
        )
        .expect("process hop1");
        assert!(
            !result_part.is_obep,
            "participant role must surface as is_obep=false"
        );
    }

    #[test]
    fn inbound_layout_ambiguity_marker_is_committed() {
        // Plan 111 §F: the inbound creator-ephemeral layout is
        // pinned by reference-router source inspection. Until
        // that source is locked, the inbound short-build
        // construction must remain explicitly disabled and the
        // marker must read `true`. A future re-enabler must
        // also flip this marker, which an audit can detect.
        // The placeholder occupies zero bytes until a pinned
        // reference-router source is available.
        const { assert!(INBOUND_SHORT_BUILD_LAYOUT_AMBIGUITY) };
        const { assert!(INBOUND_CREATOR_EPHEMERAL_PLACEHOLDER_LEN == 0) };
    }

    #[test]
    fn stbm_payload_round_trip() {
        let mut rng = ChaCha8Rng::seed_from_u64(4);
        let set = assign_record_slots(3, TunnelDirection::Outbound, &mut rng).expect("set");
        let mut slots = Vec::new();
        for _ in 0..set.record_count() {
            slots.push(build_padding_fake_record(&mut rng).expect("fake"));
        }
        let payload = encode_short_tunnel_build_payload(&set, &slots).expect("encode");
        assert_eq!(
            payload.len(),
            1 + set.record_count() as usize * RECORD_BYTES
        );
        let (count, decoded) = decode_short_tunnel_build_payload(&payload).expect("decode");
        assert_eq!(count, set.record_count());
        assert_eq!(decoded.len(), slots.len());
        for (a, b) in decoded.iter().zip(slots.iter()) {
            assert_eq!(a, b);
        }
    }

    #[test]
    fn stbm_payload_rejects_count_zero() {
        let outcome = decode_short_tunnel_build_payload(&[0_u8; 10]);
        assert!(matches!(
            outcome,
            Err(MultiRecordError::RecordCountExceedsMaximum { .. })
        ));
    }

    #[test]
    fn stbm_payload_rejects_count_over_max() {
        let mut bad = vec![9_u8];
        bad.extend_from_slice(&vec![0_u8; 9 * RECORD_BYTES]);
        let outcome = decode_short_tunnel_build_payload(&bad);
        assert!(matches!(
            outcome,
            Err(MultiRecordError::RecordCountExceedsMaximum { .. })
        ));
    }

    #[test]
    fn stbm_payload_rejects_trailing_bytes() {
        let mut bad = vec![1_u8];
        bad.extend_from_slice(&vec![0_u8; RECORD_BYTES + 1]);
        let outcome = decode_short_tunnel_build_payload(&bad);
        assert!(matches!(
            outcome,
            Err(MultiRecordError::RecordCountExceedsMaximum { .. })
        ));
    }

    #[test]
    fn otbrm_payload_matches_stbm_format() {
        let records = vec![[0x77_u8; RECORD_BYTES]; 3];
        let payload = encode_outbound_tunnel_build_reply(3, &records).expect("encode");
        let (count, decoded) = decode_outbound_tunnel_build_reply(&payload).expect("decode");
        assert_eq!(count, 3);
        assert_eq!(decoded.len(), 3);
        for (a, b) in decoded.iter().zip(records.iter()) {
            assert_eq!(a, b);
        }
    }

    #[test]
    fn originator_fake_rejects_tamper() {
        let hash = Hash::from_bytes([0x42_u8; 32]);
        let mut rng = ChaCha8Rng::seed_from_u64(5);
        let fake = build_originator_fake_record(&hash, &mut rng).expect("fake");
        let mut wire = fake.wire;
        wire[0] ^= 0x01;
        let outcome = verify_originator_fake(&fake, &wire);
        assert!(matches!(
            outcome,
            Err(MultiRecordError::OriginatorFakeModified)
        ));
    }

    #[test]
    fn multi_hop_reference_fixture_is_deterministic() {
        let hash = Hash::from_bytes([0x33_u8; 32]);
        let a = MultiHopReferenceFixture::three_hop_one_fake(7, &hash).expect("a");
        let b = MultiHopReferenceFixture::three_hop_one_fake(7, &hash).expect("b");
        assert_eq!(a.initial_payload, b.initial_payload);
        assert_eq!(a.preprocessed_payload, b.preprocessed_payload);
        assert_eq!(a.post_hop_payload, b.post_hop_payload);
        assert_eq!(a.results.len(), 3);
        for result in &a.results {
            assert_eq!(result.response_code, ShortResponseCode::Accepted);
        }
    }

    #[test]
    fn multi_hop_postprocessor_round_trip() {
        let hash = Hash::from_bytes([0x44_u8; 32]);
        let fixture = MultiHopReferenceFixture::three_hop_one_fake(11, &hash).expect("fixture");
        let cryptography = EciesX25519BuildCryptography::new();
        let post = CreatorReplyPostprocessor::process_reply(
            &cryptography,
            &fixture.contexts,
            &fixture.record_set,
            &fixture.post_hop_payload,
            None,
        )
        .expect("postprocess");
        assert_eq!(post.len(), fixture.contexts.len());
        for result in &post {
            assert_eq!(result.response_code, ShortResponseCode::Accepted);
        }
    }

    #[test]
    fn slot_assignment_permutation_differs_per_seed() {
        let mut rng_a = ChaCha8Rng::seed_from_u64(21);
        let mut rng_b = ChaCha8Rng::seed_from_u64(22);
        let a = assign_record_slots(3, TunnelDirection::Outbound, &mut rng_a).expect("a");
        let b = assign_record_slots(3, TunnelDirection::Outbound, &mut rng_b).expect("b");
        let mut differs = false;
        for index in 0..a.record_count() {
            if a.owner_at_slot(SlotIndex::new(index).expect("ok"))
                != b.owner_at_slot(SlotIndex::new(index).expect("ok"))
            {
                differs = true;
                break;
            }
        }
        assert!(
            differs,
            "different seeds should produce different slot owners"
        );
    }

    #[test]
    fn prepare_short_build_message_produces_canonical_stbm() {
        let cryptography = EciesX25519BuildCryptography::new();
        let hop0 = MultiRecordHopSpec {
            canonical_index: 0,
            router_hash: &Hash::from_bytes([0x10_u8; 32]),
            static_encryption_key: &[0xAA_u8; 32],
            role: HopRole::InboundGateway,
            receive_tunnel: TunnelId::new(0x1000).expect("id"),
            next_tunnel: TunnelId::new(0x2000).expect("id"),
            next_router_hash: &Hash::from_bytes([0x11_u8; 32]),
        };
        let hop1 = MultiRecordHopSpec {
            canonical_index: 1,
            router_hash: &Hash::from_bytes([0x11_u8; 32]),
            static_encryption_key: &[0xBB_u8; 32],
            role: HopRole::Participant,
            receive_tunnel: TunnelId::new(0x3000).expect("id"),
            next_tunnel: TunnelId::new(0x4000).expect("id"),
            next_router_hash: &Hash::from_bytes([0x12_u8; 32]),
        };
        let hop2 = MultiRecordHopSpec {
            canonical_index: 2,
            router_hash: &Hash::from_bytes([0x12_u8; 32]),
            static_encryption_key: &[0xCC_u8; 32],
            role: HopRole::OutboundEndpoint,
            receive_tunnel: TunnelId::new(0x5000).expect("id"),
            next_tunnel: TunnelId::new(0x6000).expect("id"),
            next_router_hash: &Hash::from_bytes([0x13_u8; 32]),
        };
        let hops = vec![hop0, hop1, hop2];
        let mut rng = ChaCha8Rng::seed_from_u64(31);
        let prepared = prepare_short_build_message(
            &cryptography,
            &hops,
            TunnelDirection::Outbound,
            [0x10, 0x00, 0x00, 0x01],
            60_000,
            0x1234_5678,
            Some(Hash::from_bytes([0x10_u8; 32])),
            None,
            &mut rng,
        )
        .expect("prepare");
        assert_eq!(prepared.record_set.record_count(), 4);
        assert_eq!(prepared.payload.len(), 1 + 4 * RECORD_BYTES);
        assert_eq!(prepared.hop_contexts.len(), 3);
        assert!(prepared.originator_fake.is_none());
    }

    #[test]
    fn prepare_short_build_inbound_includes_originator_fake() {
        let cryptography = EciesX25519BuildCryptography::new();
        let originator_hash = Hash::from_bytes([0x77_u8; 32]);
        let hop0 = MultiRecordHopSpec {
            canonical_index: 0,
            router_hash: &Hash::from_bytes([0x10_u8; 32]),
            static_encryption_key: &[0xAA_u8; 32],
            role: HopRole::InboundGateway,
            receive_tunnel: TunnelId::new(0x1000).expect("id"),
            next_tunnel: TunnelId::new(0x2000).expect("id"),
            next_router_hash: &Hash::from_bytes([0x11_u8; 32]),
        };
        let hop1 = MultiRecordHopSpec {
            canonical_index: 1,
            router_hash: &Hash::from_bytes([0x11_u8; 32]),
            static_encryption_key: &[0xBB_u8; 32],
            role: HopRole::Participant,
            receive_tunnel: TunnelId::new(0x3000).expect("id"),
            next_tunnel: TunnelId::new(0x4000).expect("id"),
            // Terminal hop hands its own router hash as the next router.
            next_router_hash: &Hash::from_bytes([0x11_u8; 32]),
        };
        let hops = vec![hop0, hop1];
        let mut rng = ChaCha8Rng::seed_from_u64(41);
        let prepared = prepare_short_build_message(
            &cryptography,
            &hops,
            TunnelDirection::Inbound,
            [0x10, 0x00, 0x00, 0x01],
            60_000,
            0x1234_5678,
            Some(Hash::from_bytes([0x10_u8; 32])),
            Some(&originator_hash),
            &mut rng,
        )
        .expect("prepare");
        assert_eq!(prepared.record_set.record_count(), 4);
        assert!(prepared.originator_fake.is_some());
        assert_eq!(
            prepared.originator_fake.as_ref().unwrap().hash_prefix,
            originator_hash.as_bytes()[..crate::build_crypto::HASH_PREFIX_LEN]
        );
    }

    #[test]
    fn process_hop_rejects_zero_matches() {
        let cryptography = EciesX25519BuildCryptography::new();
        // Build a payload whose slots do not include the supplied
        // hop's hash prefix.
        let mut slots = vec![[0x77_u8; RECORD_BYTES]; 4];
        for index in 0..4_u8 {
            let fake_slot = SlotIndex::new(index).expect("ok");
            chacha20_xor(&[0x33_u8; 32], fake_slot, &mut slots[index as usize]).expect("transform");
        }
        let payload = encode_short_tunnel_build_payload_from_count(4, &slots).expect("payload");
        let hop_identity = Hash::from_bytes([0x99_u8; 32]);
        let outcome = MessageHopProcessor::process_hop(
            &cryptography,
            &payload,
            &[0x44_u8; 32],
            &hop_identity,
            ShortResponseCode::Accepted,
        );
        assert!(matches!(outcome, Err(MultiRecordError::HopHashNotFound)));
    }

    #[test]
    fn postprocessor_rejects_tampered_originator_fake() {
        // Run an inbound fixture through the full pipeline and
        // tamper with the originator fake before postprocessing.
        let cryptography = EciesX25519BuildCryptography::new();
        let originator_hash = Hash::from_bytes([0x55_u8; 32]);
        let hop0 = MultiRecordHopSpec {
            canonical_index: 0,
            router_hash: &Hash::from_bytes([0x10_u8; 32]),
            static_encryption_key: &[0xAA_u8; 32],
            role: HopRole::InboundGateway,
            receive_tunnel: TunnelId::new(0x3000).expect("id"),
            next_tunnel: TunnelId::new(0x4000).expect("id"),
            next_router_hash: &Hash::from_bytes([0x11_u8; 32]),
        };
        let hop1 = MultiRecordHopSpec {
            canonical_index: 1,
            router_hash: &Hash::from_bytes([0x11_u8; 32]),
            static_encryption_key: &[0xBB_u8; 32],
            role: HopRole::Participant,
            receive_tunnel: TunnelId::new(0x5000).expect("id"),
            next_tunnel: TunnelId::new(0x6000).expect("id"),
            next_router_hash: &Hash::from_bytes([0x11_u8; 32]),
        };
        let hops = vec![hop0, hop1];
        let mut rng = ChaCha8Rng::seed_from_u64(51);
        let prepared = prepare_short_build_message(
            &cryptography,
            &hops,
            TunnelDirection::Inbound,
            [0x10, 0x00, 0x00, 0x01],
            60_000,
            0x1234_5678,
            Some(Hash::from_bytes([0x10_u8; 32])),
            Some(&originator_hash),
            &mut rng,
        )
        .expect("prepare");
        let fake = prepared.originator_fake.as_ref().unwrap();
        // Find the originator fake slot and tamper it.
        let mut tampered = prepared.payload.to_vec();
        let (count, mut slots) = decode_short_tunnel_build_payload(&tampered).expect("decode");
        let fake_slot = (0..count)
            .find(|index| {
                prepared
                    .record_set
                    .is_originator_fake_slot(SlotIndex::new(*index).expect("ok"))
            })
            .expect("fake slot");
        slots[fake_slot as usize][0] ^= 0x01;
        tampered = encode_short_tunnel_build_payload_from_count(count, &slots).expect("encode");
        let post = CreatorReplyPostprocessor::process_reply(
            &cryptography,
            &prepared.hop_contexts,
            &prepared.record_set,
            &tampered,
            Some(fake),
        );
        assert!(matches!(
            post,
            Err(MultiRecordError::OriginatorFakeModified)
        ));
    }
}
