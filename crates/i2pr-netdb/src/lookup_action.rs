//! Plan 105 §5 action vocabulary and bounded decompression.
//!
//! The state machine produces typed actions rather than effects. A
//! later runtime adapter (Plan 106 / Milestone 5) translates each
//! action into a real transport or tunnel operation. The vocabulary is
//! intentionally small:
//!
//! - `SendDatabaselookup`
//! - `NeedExploratoryReplyPath`
//! - `Complete`

use std::fmt;

use i2pr_proto::CodecError;
use thiserror::Error;

use crate::lookup_id::{LookupId, LookupKind, ReplyPath};
use crate::router_info::RouterHash;

/// Maximum size of a single gzip-compressed `RouterInfo` payload the
/// decoder will accept. The bound matches the per-record stash cap in
/// the I2P Java NetDB and prevents an adversarial floodfill from
/// forcing huge decompressions.
pub const MAX_COMPRESSED_ROUTER_INFO_BYTES: usize = 16 * 1024;
/// Maximum decompressed `RouterInfo` payload. Placed above the
/// compressed ceiling so the decoder never has to allocate gigabytes
/// to dissolve a small payload.
pub const MAX_DECOMPRESSED_ROUTER_INFO_BYTES: usize = 32 * 1024;

/// Action emitted by the lookup state machine. The runtime adapter is
/// expected to translate each variant into one concrete step.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LookupAction {
    /// Send a `DatabaseLookup` to the specified peer. The block
    /// carries the complete outbound I2NP payload (already encoded by
    /// the state machine) so the runtime cannot inspect the protocol
    /// body.
    SendDatabaselookup {
        /// Lookup identity.
        lookup_id: LookupId,
        /// Candidate floodfill RouterHash to query.
        peer: RouterHash,
        /// Complete outbound `DatabaseLookup` body length.
        encoded_len: usize,
    },
    /// The state machine requires an exploratory reply path before
    /// it can emit any `DatabaseLookup`. The runtime must either
    /// supply a `ReplyPath` or treat the lookup as blocked.
    NeedExploratoryReplyPath {
        /// Lookup identity awaiting a reply path.
        lookup_id: LookupId,
    },
    /// The state machine has terminated. The runtime must dispatch
    /// the outcome to the embedded waiters.
    Complete {
        /// Lookup identity.
        lookup_id: LookupId,
        /// Final outcome summary.
        outcome: LookupOutcome,
    },
}

/// Final lookup outcome summary. The shape is bounded so the runtime
/// can serialise it without inspecting the protocol state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LookupOutcome {
    kind: LookupKind,
    target: RouterHash,
    final_state: LookupFinalState,
    attempts: usize,
    suggestions_merged: usize,
}

impl LookupOutcome {
    /// Constructs a new outcome summary.
    pub fn new(
        kind: LookupKind,
        target: RouterHash,
        final_state: LookupFinalState,
        attempts: usize,
        suggestions_merged: usize,
    ) -> Self {
        Self {
            kind,
            target,
            final_state,
            attempts,
            suggestions_merged,
        }
    }

    /// Returns the lookup kind.
    pub const fn kind(&self) -> LookupKind {
        self.kind
    }

    /// Returns the target RouterHash.
    pub const fn target(&self) -> RouterHash {
        self.target
    }

    /// Returns the final state.
    pub const fn final_state(&self) -> LookupFinalState {
        self.final_state
    }

    /// Returns the number of attempts that were issued.
    pub const fn attempts(&self) -> usize {
        self.attempts
    }

    /// Returns the number of `DatabaseSearchReply` suggestions that
    /// were merged in.
    pub const fn suggestions_merged(&self) -> usize {
        self.suggestions_merged
    }
}

/// Categorical final state of a lookup.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LookupFinalState {
    /// The target `RouterInfo` was accepted from a `DatabaseStore`
    /// response.
    Success,
    /// The lookup exhausted the peer budget without a usable
    /// `DatabaseStore`.
    PeerExhausted,
    /// The lookup deadline elapsed before completion.
    Timeout,
    /// The runtime cancelled the lookup.
    Cancelled,
    /// The lookup was blocked because no exploratory reply path was
    /// supplied.
    PathUnavailable,
    /// The lookup found no eligible floodfill candidates.
    NoEligibleCandidates,
}

impl fmt::Display for LookupFinalState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Success => formatter.write_str("success"),
            Self::PeerExhausted => formatter.write_str("peer-exhausted"),
            Self::Timeout => formatter.write_str("timeout"),
            Self::Cancelled => formatter.write_str("cancelled"),
            Self::PathUnavailable => formatter.write_str("path-unavailable"),
            Self::NoEligibleCandidates => formatter.write_str("no-eligible-candidates"),
        }
    }
}

/// Bounded gzip decompression for `DatabaseStore` `RouterInfo`
/// payloads.
///
/// The decoder enforces both the compressed and decompressed length
/// ceilings before allocating any buffer. Returning a typed
/// `DecompressionError` is the contract — the caller may then route
/// the typed failure into the lookup state machine.
pub fn decompress_router_info(compressed: &[u8]) -> Result<Vec<u8>, DecompressionError> {
    use std::io::Read as _;

    if compressed.is_empty() {
        return Err(DecompressionError::Empty);
    }
    if compressed.len() > MAX_COMPRESSED_ROUTER_INFO_BYTES {
        return Err(DecompressionError::CompressedTooLarge {
            actual: compressed.len(),
            maximum: MAX_COMPRESSED_ROUTER_INFO_BYTES,
        });
    }
    let mut decoder = flate2::read::GzDecoder::new(compressed);
    let mut buf = Vec::with_capacity(compressed.len() * 2);
    let mut chunk = [0u8; 4096];
    loop {
        match decoder.read(&mut chunk) {
            Ok(0) => return Ok(buf),
            Ok(read) => {
                if buf
                    .len()
                    .checked_add(read)
                    .is_none_or(|total| total > MAX_DECOMPRESSED_ROUTER_INFO_BYTES)
                {
                    return Err(DecompressionError::DecompressedTooLarge {
                        actual: buf.len() + read,
                        maximum: MAX_DECOMPRESSED_ROUTER_INFO_BYTES,
                    });
                }
                buf.extend_from_slice(&chunk[..read]);
            }
            Err(error) => {
                return Err(DecompressionError::Io {
                    kind: error.kind(),
                    context: "decompress-input",
                });
            }
        }
    }
}

/// Typed failure category for `DatabaseStore` `RouterInfo`
/// decompression.
#[derive(Clone, Debug, Error)]
pub enum DecompressionError {
    /// The compressed payload was empty.
    Empty,
    /// The compressed payload exceeded the per-record ceiling.
    CompressedTooLarge {
        /// Actual compressed length.
        actual: usize,
        /// Maximum compressed length accepted.
        maximum: usize,
    },
    /// The decompressed payload exceeded the post-decompression
    /// ceiling.
    DecompressedTooLarge {
        /// Actual decompressed length seen so far.
        actual: usize,
        /// Maximum decompressed length accepted.
        maximum: usize,
    },
    /// The underlying decoder reported an I/O error.
    Io {
        /// Categorical I/O failure.
        kind: std::io::ErrorKind,
        /// Static error context.
        context: &'static str,
    },
    /// The decompressed payload could not be decoded as a router
    /// info.
    RouterInfoDecode(#[from] CodecError),
}

impl PartialEq for DecompressionError {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Empty, Self::Empty) => true,
            (
                Self::CompressedTooLarge {
                    actual: a,
                    maximum: ma,
                },
                Self::CompressedTooLarge {
                    actual: b,
                    maximum: mb,
                },
            ) => a == b && ma == mb,
            (
                Self::DecompressedTooLarge {
                    actual: a,
                    maximum: ma,
                },
                Self::DecompressedTooLarge {
                    actual: b,
                    maximum: mb,
                },
            ) => a == b && ma == mb,
            (
                Self::Io {
                    kind: ka,
                    context: ca,
                },
                Self::Io {
                    kind: kb,
                    context: cb,
                },
            ) => ka == kb && ca == cb,
            (Self::RouterInfoDecode(a), Self::RouterInfoDecode(b)) => a == b,
            _ => false,
        }
    }
}

impl Eq for DecompressionError {}

impl fmt::Display for DecompressionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("compressed RouterInfo payload is empty"),
            Self::CompressedTooLarge { actual, maximum } => write!(
                formatter,
                "compressed RouterInfo exceeds {maximum}-byte ceiling (actual {actual})"
            ),
            Self::DecompressedTooLarge { actual, maximum } => write!(
                formatter,
                "decompressed RouterInfo exceeds {maximum}-byte ceiling (actual {actual})"
            ),
            Self::Io { kind, context } => {
                write!(formatter, "decompression failed at {context}: {kind:?}")
            }
            Self::RouterInfoDecode(error) => {
                write!(formatter, "router info decode failed: {error}")
            }
        }
    }
}

/// Bound on the number of excluded peers the lookup may attach to a
/// `DatabaseLookup`. The codec enforces
/// `MAX_DATABASE_LOOKUP_EXCLUDED_PEERS` (512); we keep the lookup
/// state machine smaller to avoid stressing the registry.
pub const LOOKUP_EXCLUDED_PEER_BUDGET: usize = 256;

impl LookupAction {
    /// Returns the lookup identifier, if any.
    pub const fn lookup_id(&self) -> LookupId {
        match self {
            Self::SendDatabaselookup { lookup_id, .. }
            | Self::NeedExploratoryReplyPath { lookup_id, .. }
            | Self::Complete { lookup_id, .. } => *lookup_id,
        }
    }

    /// Returns `true` when the action is the terminal `Complete`
    /// action.
    pub const fn is_complete(&self) -> bool {
        matches!(self, Self::Complete { .. })
    }
}

/// Marker trait for any state-machine adapter that accepts a reply
/// path. The state machine calls the adapter at most once per lookup
/// to convert the supplied `ReplyPath` into a transaction-level
/// `SendDatabaselookup` action.
pub trait ReplyPathSink {
    /// Accepts a reply path for the lookup. Returning `false` tells
    /// the state machine to keep blocking on the path.
    fn accept_reply_path(&mut self, lookup_id: LookupId, path: ReplyPath) -> bool;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_lookup_id_round_trips() {
        let target = RouterHash::from_bytes([0x77u8; 32]);
        let id = LookupId::new(7, LookupKind::RouterInfo, target);
        let action = LookupAction::Complete {
            lookup_id: id,
            outcome: LookupOutcome::new(
                LookupKind::RouterInfo,
                target,
                LookupFinalState::Success,
                1,
                0,
            ),
        };
        assert_eq!(action.lookup_id(), id);
        assert!(action.is_complete());
    }

    #[test]
    fn action_lookup_id_round_trips_for_non_complete() {
        let target = RouterHash::from_bytes([0x12u8; 32]);
        let id = LookupId::new(99, LookupKind::RouterInfo, target);
        let action = LookupAction::NeedExploratoryReplyPath { lookup_id: id };
        assert_eq!(action.lookup_id(), id);
        assert!(!action.is_complete());
    }

    #[test]
    fn empty_payload_is_rejected() {
        let result = decompress_router_info(&[]).unwrap_err();
        assert_eq!(result, DecompressionError::Empty);
    }

    #[test]
    fn malicious_oversized_compressed_payload_is_rejected() {
        let payload = vec![0u8; MAX_COMPRESSED_ROUTER_INFO_BYTES + 1];
        let result = decompress_router_info(&payload).unwrap_err();
        assert!(matches!(
            result,
            DecompressionError::CompressedTooLarge { .. }
        ));
    }

    #[test]
    fn malformed_gzip_input_is_rejected() {
        let payload = vec![0x66, 0x66, 0x66, 0x66];
        let result = decompress_router_info(&payload);
        assert!(matches!(result, Err(DecompressionError::Io { .. })));
    }

    #[test]
    fn truncated_gzip_input_is_rejected() {
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        use std::io::Write as _;
        encoder.write_all(b"abc").unwrap();
        let mut compressed = encoder.finish().unwrap();
        compressed.truncate(compressed.len() / 2);
        let result = decompress_router_info(&compressed);
        assert!(matches!(result, Err(DecompressionError::Io { .. })));
    }

    #[test]
    fn round_trip_compress_then_decompress_returns_input() {
        let original = b"i2pr-routerinfo-payload";
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        use std::io::Write as _;
        encoder.write_all(original).unwrap();
        let compressed = encoder.finish().unwrap();
        assert!(compressed.len() < original.len() + 64);
        let decompressed = decompress_router_info(&compressed).expect("decompress");
        assert_eq!(decompressed, original);
    }

    #[test]
    fn decompressed_ceiling_is_enforced() {
        // Build a payload that decompresses past the ceiling.
        let big_payload = vec![0u8; MAX_DECOMPRESSED_ROUTER_INFO_BYTES + 1];
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        use std::io::Write as _;
        encoder.write_all(&big_payload).unwrap();
        let compressed = encoder.finish().unwrap();
        let result = decompress_router_info(&compressed).unwrap_err();
        assert!(matches!(
            result,
            DecompressionError::DecompressedTooLarge { .. }
        ));
    }
}
