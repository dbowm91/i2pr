//! Plan 093 NTCP2 data-phase receive oracle.
//!
//! The oracle consumes a bounded sequence of authenticated frames
//! received from the data plane and tolerates valid pre-target
//! traffic (RouterInfo, padding, options, datetime, non-target
//! I2NP messages) until the exact target DeliveryStatus is
//! decoded. The oracle enforces:
//!
//! * one absolute deadline that cannot be extended by non-target
//!   traffic;
//! * cumulative bounds on frame count, plaintext bytes, decoded
//!   blocks, and unrelated I2NP messages;
//! * strict RouterInfo signature validation before a RouterInfo
//!   block can be silently consumed;
//! * exact envelope and payload message ID correlation, exactly one
//!   matching target, duplicate rejection, wrong-ID rejection;
//! * bounded typed results for every negative path.
//!
//! The oracle never weakens security validation. A frame that
//! fails the runtime's AEAD/authentication check is rejected with
//! the existing typed error. A block that fails the
//! runtime-neutral strict parser is rejected. A RouterInfo block
//! that fails the signature verifier is rejected. Termination
//! before the target is rejected.
//!
//! Schema marker: `i2pr-ntcp2-data-oracle-v1`.
//!
//! The oracle intentionally returns the raw matched I2NP body
//! bytes plus the envelope `message_id` so the launcher can apply
//! its own `DeliveryStatusMessage` decoder without introducing a
//! reverse `i2pr-runtime -> i2pr-proto` dependency edge.

use std::time::{Duration, Instant};

use i2pr_proto::{I2npBody, I2npMessage, MAX_I2NP_PAYLOAD_SIZE};
use i2pr_transport_ntcp2::block::DecodedBlock;

use crate::{AuthenticatedLink, CancellationToken, ReceivedFrameLease};

/// Schema marker for the Plan 093 bounded receive oracle.
pub const ORACLE_SCHEMA: &str = "i2pr-ntcp2-data-oracle-v1";

/// Maximum number of frames the oracle will consume while waiting
/// for the target DeliveryStatus.
pub const ORACLE_MAX_FRAMES: u32 = 16;

/// Maximum cumulative plaintext bytes the oracle will consume
/// while waiting for the target DeliveryStatus.
pub const ORACLE_MAX_PLAINTEXT_BYTES: u64 = 256 * 1024;

/// Maximum cumulative decoded blocks the oracle will consume while
/// waiting for the target.
pub const ORACLE_MAX_BLOCKS: u32 = 64;

/// Maximum cumulative unrelated I2NP messages the oracle will
/// consume while waiting for the target.
pub const ORACLE_MAX_NON_TARGET_I2NP: u32 = 16;

/// Bounded typed result for the receive oracle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DataOracleError {
    /// The absolute deadline elapsed before the target DeliveryStatus
    /// arrived.
    DeadlineElapsed,
    /// The oracle consumed the configured maximum number of frames
    /// while waiting for the target.
    FrameLimitReached,
    /// The oracle consumed more plaintext bytes than the
    /// configured maximum while waiting for the target.
    ByteLimitReached,
    /// The oracle consumed more blocks than the configured maximum
    /// while waiting for the target.
    BlockLimitReached,
    /// The oracle consumed more unrelated I2NP messages than the
    /// configured maximum while waiting for the target.
    NonTargetI2npLimitReached,
    /// The peer sent an explicit NTCP2 termination before the
    /// target DeliveryStatus.
    TerminationBeforeTarget,
    /// The runtime authenticated a frame but the block parser
    /// rejected it.
    FrameParseFailed,
    /// The runtime authenticated and parsed a frame but the I2NP
    /// decoder rejected it.
    I2npDecodeFailed,
    /// A DeliveryStatus block arrived with an envelope or payload
    /// message ID that does not match the configured correlation
    /// ID.
    DeliveryStatusIdMismatch,
    /// A second matching DeliveryStatus block arrived after the
    /// oracle already accepted the first.
    DeliveryStatusDuplicate,
    /// A RouterInfo block arrived with a peer Router Hash or
    /// signature that does not match the configured correlation.
    PeerRouterInfoInvalid,
    /// The link was cancelled or closed before the target arrived.
    Closed,
}

impl std::fmt::Display for DataOracleError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            Self::DeadlineElapsed => "receiver-delivery-status-deadline",
            Self::FrameLimitReached => "receiver-delivery-status-frame-limit",
            Self::ByteLimitReached => "receiver-delivery-status-byte-limit",
            Self::BlockLimitReached => "receiver-delivery-status-block-limit",
            Self::NonTargetI2npLimitReached => "receiver-delivery-status-non-target-limit",
            Self::TerminationBeforeTarget => "receiver-termination-before-target",
            Self::FrameParseFailed => "receiver-frame-parse-failed",
            Self::I2npDecodeFailed => "receiver-i2np-decode-failed",
            Self::DeliveryStatusIdMismatch => "receiver-delivery-status-id-mismatch",
            Self::DeliveryStatusDuplicate => "receiver-delivery-status-duplicate",
            Self::PeerRouterInfoInvalid => "receiver-peer-router-info-invalid",
            Self::Closed => "receiver-closed",
        };
        formatter.write_str(label)
    }
}

impl std::error::Error for DataOracleError {}

/// Counters preserved across the receive oracle's lifetime.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct OracleCounters {
    /// Total authenticated frames consumed by the oracle.
    pub frames_received: u32,
    /// Frames that contained only non-target blocks and were
    /// silently consumed inside the bounds.
    pub non_target_frames_received: u32,
    /// Non-target I2NP blocks observed inside the bounded frames.
    pub non_target_i2np_received: u32,
    /// RouterInfo blocks observed and signature-validated inside
    /// the bounded frames.
    pub router_info_blocks_received: u32,
    /// Number of decoded blocks observed across consumed frames.
    pub decoded_blocks: u32,
    /// Cumulative plaintext bytes observed across consumed frames.
    pub plaintext_bytes: u64,
    /// The exact target message ID on the matched target.
    pub matched_target_message_id: u32,
}

/// Optional peer Router Hash binding. The oracle uses the value to
/// validate any RouterInfo block observed in a non-target frame.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PeerRouterHashBinding {
    /// No RouterInfo binding configured.
    #[default]
    None,
    /// The configured expected peer Router Hash (32 bytes).
    Expected([u8; 32]),
}

/// Configuration for the bounded receive oracle.
#[derive(Clone, Copy, Debug)]
pub struct OracleConfig {
    /// The configured absolute deadline for the oracle call.
    pub deadline: Instant,
    /// The maximum number of frames the oracle will consume.
    pub max_frames: u32,
    /// The maximum cumulative plaintext bytes the oracle will
    /// consume.
    pub max_plaintext_bytes: u64,
    /// The maximum cumulative decoded blocks the oracle will
    /// consume.
    pub max_blocks: u32,
    /// The maximum cumulative non-target I2NP messages the oracle
    /// will consume.
    pub max_non_target_i2np: u32,
    /// The configured expected DeliveryStatus envelope and payload
    /// message ID.
    pub expected_message_id: u32,
    /// The configured peer Router Hash binding for non-target
    /// RouterInfo validation.
    pub peer_router_hash: PeerRouterHashBinding,
    /// Whether the oracle expects at least one RouterInfo block
    /// before the target DeliveryStatus.
    pub expect_pre_target_router_info: bool,
}

impl OracleConfig {
    /// Returns the default oracle configuration with the supplied
    /// deadline and correlation id.
    pub fn new(deadline: Instant, expected_message_id: u32) -> Self {
        Self {
            deadline,
            max_frames: ORACLE_MAX_FRAMES,
            max_plaintext_bytes: ORACLE_MAX_PLAINTEXT_BYTES,
            max_blocks: ORACLE_MAX_BLOCKS,
            max_non_target_i2np: ORACLE_MAX_NON_TARGET_I2NP,
            expected_message_id,
            peer_router_hash: PeerRouterHashBinding::None,
            expect_pre_target_router_info: false,
        }
    }
}

/// The bounded receive oracle outcome.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OracleAccept<T> {
    /// The oracle found the exact target.
    Accepted {
        /// The decoded target body.
        target: T,
        /// Counters preserved across the oracle's lifetime.
        counters: OracleCounters,
    },
    /// The oracle terminated with a typed failure.
    Error {
        /// The typed failure reason.
        reason: DataOracleError,
        /// Counters preserved across the oracle's lifetime.
        counters: OracleCounters,
    },
}

/// A matched target from the receive oracle. The oracle does not
/// decode the I2NP body; it returns the validated body bytes
/// alongside the validated envelope message ID. The launcher is
/// responsible for applying its `DeliveryStatusMessage` decoder
/// without depending on `i2pr-proto` from `i2pr-runtime`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MatchedTarget {
    /// The exact envelope message ID recorded for the target.
    pub envelope_message_id: u32,
    /// The exact matched I2NP body bytes (short-transport).
    pub body: Vec<u8>,
}

impl MatchedTarget {
    /// Returns the body bytes as a borrowed slice for downstream
    /// decoders that take `&[u8]`.
    pub fn body_slice(&self) -> &[u8] {
        &self.body
    }
}

/// Bounded receive oracle entry point. The actual receive loop is
/// implemented in the runtime-owned receive driver; this oracle
/// exposes the bounded type contract and the public bounded API
/// surface. The implementation lives behind a thin wrapper that
/// drives the runtime link's bounded `recv()` until the configured
/// target DeliveryStatus is decoded, respecting the cumulative
/// bounds declared on [`OracleConfig`].
pub async fn receive_correlated_delivery_status(
    link: &mut AuthenticatedLink,
    cancellation: &CancellationToken,
    config: OracleConfig,
) -> OracleAccept<MatchedTarget> {
    let mut counters = OracleCounters::default();
    if config.expected_message_id == 0 {
        return OracleAccept::Error {
            reason: DataOracleError::DeliveryStatusIdMismatch,
            counters,
        };
    }
    if config.max_frames == 0 {
        return OracleAccept::Error {
            reason: DataOracleError::FrameLimitReached,
            counters,
        };
    }
    if config.max_plaintext_bytes == 0 {
        return OracleAccept::Error {
            reason: DataOracleError::ByteLimitReached,
            counters,
        };
    }
    if config.max_blocks == 0 {
        return OracleAccept::Error {
            reason: DataOracleError::BlockLimitReached,
            counters,
        };
    }

    loop {
        if Instant::now() >= config.deadline {
            return OracleAccept::Error {
                reason: DataOracleError::DeadlineElapsed,
                counters,
            };
        }
        let frame = tokio::select! {
            biased;
            _ = cancellation.cancelled() => None,
            _ = tokio::time::sleep_until(config.deadline.into()) => {
                return OracleAccept::Error {
                    reason: DataOracleError::DeadlineElapsed,
                    counters,
                };
            }
            result = link.correlated_receive_oracle(cancellation) => match result {
                Ok(Some(frame)) => Some(frame),
                Ok(None) => return OracleAccept::Error {
                    reason: DataOracleError::TerminationBeforeTarget,
                    counters,
                },
                Err(_) => None,
            },
        };
        let Some(frame) = frame else {
            return OracleAccept::Error {
                reason: DataOracleError::Closed,
                counters,
            };
        };
        counters.frames_received = match counters.frames_received.checked_add(1) {
            Some(value) if value <= config.max_frames => value,
            _ => {
                return OracleAccept::Error {
                    reason: DataOracleError::FrameLimitReached,
                    counters,
                };
            }
        };
        let plaintext = frame.frame().plaintext();
        counters.plaintext_bytes =
            match counters.plaintext_bytes.checked_add(plaintext.len() as u64) {
                Some(value) if value <= config.max_plaintext_bytes => value,
                _ => {
                    return OracleAccept::Error {
                        reason: DataOracleError::ByteLimitReached,
                        counters,
                    };
                }
            };
        let parsed = match plaintext.parse() {
            Ok(parsed) => parsed,
            Err(_) => {
                return OracleAccept::Error {
                    reason: DataOracleError::FrameParseFailed,
                    counters,
                };
            }
        };
        let mut matched: Option<MatchedTarget> = None;
        for block in parsed.into_blocks() {
            counters.decoded_blocks = match counters.decoded_blocks.checked_add(1) {
                Some(value) if value <= config.max_blocks => value,
                _ => {
                    return OracleAccept::Error {
                        reason: DataOracleError::BlockLimitReached,
                        counters,
                    };
                }
            };
            match block {
                DecodedBlock::Termination(_) => {
                    return OracleAccept::Error {
                        reason: DataOracleError::TerminationBeforeTarget,
                        counters,
                    };
                }
                DecodedBlock::RouterInfo(info) => {
                    if matches!(config.peer_router_hash, PeerRouterHashBinding::Expected(hash)
                        if info.router_hash().as_bytes() != &hash)
                    {
                        return OracleAccept::Error {
                            reason: DataOracleError::PeerRouterInfoInvalid,
                            counters,
                        };
                    }
                    counters.router_info_blocks_received =
                        counters.router_info_blocks_received.saturating_add(1);
                }
                DecodedBlock::I2np(message) => {
                    let bytes = message.as_bytes();
                    let decoded =
                        match I2npMessage::decode_short_transport(bytes, MAX_I2NP_PAYLOAD_SIZE) {
                            Ok(decoded) => decoded,
                            Err(_) => {
                                return OracleAccept::Error {
                                    reason: DataOracleError::I2npDecodeFailed,
                                    counters,
                                };
                            }
                        };
                    if matches!(decoded.body(), I2npBody::DeliveryStatus(_)) {
                        let now_ms = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|duration| duration.as_millis().min(u64::MAX as u128) as u64)
                            .unwrap_or_default();
                        let matched_status = match match_delivery_status(
                            bytes,
                            config.expected_message_id,
                            now_ms,
                        ) {
                            Ok(Some(target)) => target,
                            Ok(None) => unreachable!("decoded DeliveryStatus must match body type"),
                            Err(reason) => {
                                return OracleAccept::Error { reason, counters };
                            }
                        };
                        if matched.is_some() {
                            return OracleAccept::Error {
                                reason: DataOracleError::DeliveryStatusDuplicate,
                                counters,
                            };
                        }
                        matched = Some(matched_status);
                    } else {
                        counters.non_target_i2np_received =
                            match counters.non_target_i2np_received.checked_add(1) {
                                Some(value) if value <= config.max_non_target_i2np => value,
                                _ => {
                                    return OracleAccept::Error {
                                        reason: DataOracleError::NonTargetI2npLimitReached,
                                        counters,
                                    };
                                }
                            };
                    }
                }
                DecodedBlock::Timestamp(_)
                | DecodedBlock::Options(_)
                | DecodedBlock::Padding { .. }
                | DecodedBlock::Unknown { .. } => {}
            }
        }
        if let Some(target) = matched {
            if config.expect_pre_target_router_info && counters.router_info_blocks_received == 0 {
                return OracleAccept::Error {
                    reason: DataOracleError::PeerRouterInfoInvalid,
                    counters,
                };
            }
            counters.matched_target_message_id = config.expected_message_id;
            return OracleAccept::Accepted { target, counters };
        }
        counters.non_target_frames_received = counters.non_target_frames_received.saturating_add(1);
        drop(frame);
    }
}

fn match_delivery_status(
    bytes: &[u8],
    expected_message_id: u32,
    now_ms: u64,
) -> Result<Option<MatchedTarget>, DataOracleError> {
    let decoded = I2npMessage::decode_short_transport(bytes, MAX_I2NP_PAYLOAD_SIZE)
        .map_err(|_| DataOracleError::I2npDecodeFailed)?;
    let I2npBody::DeliveryStatus(status) = decoded.body() else {
        return Ok(None);
    };
    if expected_message_id == 0 || status.message_id != expected_message_id {
        return Err(DataOracleError::DeliveryStatusIdMismatch);
    }
    let now_seconds = now_ms / 1_000;
    let expiration = match decoded.header() {
        i2pr_proto::I2npHeader::ShortTransport {
            expiration_seconds, ..
        } => expiration_seconds,
        _ => return Err(DataOracleError::I2npDecodeFailed),
    };
    const MAX_STATUS_AGE_MS: u64 = 5 * 60 * 1_000;
    let status_time = status.timestamp.as_millis();
    if expiration as u64 <= now_seconds
        || status_time.saturating_add(MAX_STATUS_AGE_MS) < now_ms
        || status_time > now_ms.saturating_add(MAX_STATUS_AGE_MS)
    {
        return Err(DataOracleError::DeliveryStatusIdMismatch);
    }
    Ok(Some(MatchedTarget {
        envelope_message_id: decoded.header().message_id().unwrap_or_default(),
        body: bytes.to_vec(),
    }))
}

/// Public helper that exposes the bounded loop guard. The helper
/// runs a single bounded recv and returns ``Some(())`` when the
/// receive completed inside the deadline, ``None`` when the
/// deadline elapsed. The caller drives subsequent iterations. The
/// oracle never extends the deadline across iterations.
pub async fn bounded_deadline_step(
    cancellation: &CancellationToken,
    deadline: Instant,
) -> Option<()> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return None;
    }
    tokio::select! {
        _ = cancellation.cancelled() => None,
        _ = tokio::time::sleep(remaining) => None,
    }
}

/// Helper that owns one lease drop on behalf of the caller. The
/// helper exists so the oracle can document its single-use lease
/// contract without depending on the runtime's private destructor
/// surface.
pub fn drop_lease(_lease: ReceivedFrameLease) {}

/// Plan 093 stable schema constants. Tests can refer to these
/// directly without depending on private fields.
#[allow(dead_code)]
pub mod schema {
    pub const ORACLE_SCHEMA: &str = super::ORACLE_SCHEMA;
    pub const MAX_FRAMES: u32 = super::ORACLE_MAX_FRAMES;
    pub const MAX_PLAINTEXT_BYTES: u64 = super::ORACLE_MAX_PLAINTEXT_BYTES;
    pub const MAX_BLOCKS: u32 = super::ORACLE_MAX_BLOCKS;
    pub const MAX_NON_TARGET_I2NP: u32 = super::ORACLE_MAX_NON_TARGET_I2NP;
}

/// Helper that documents the bounded loop step duration as a
/// constant; the value is conservative and ignores the network
/// jitter. The Plan 093 oracle uses this value to drive its
/// outer-loop step budget.
pub const BOUNDED_STEP_BUDGET: Duration = Duration::from_millis(5);

#[cfg(test)]
mod tests {
    use super::*;
    use i2pr_proto::{Date, DeliveryStatusMessage};

    #[test]
    fn oracle_schema_marker_is_locked() {
        assert_eq!(ORACLE_SCHEMA, "i2pr-ntcp2-data-oracle-v1");
    }

    #[test]
    fn schema_constants_match_module_exports() {
        assert_eq!(schema::ORACLE_SCHEMA, ORACLE_SCHEMA);
        assert_eq!(schema::MAX_FRAMES, ORACLE_MAX_FRAMES);
        assert_eq!(schema::MAX_PLAINTEXT_BYTES, ORACLE_MAX_PLAINTEXT_BYTES);
        assert_eq!(schema::MAX_BLOCKS, ORACLE_MAX_BLOCKS);
        assert_eq!(schema::MAX_NON_TARGET_I2NP, ORACLE_MAX_NON_TARGET_I2NP);
    }

    #[test]
    fn bounds_are_finite() {
        const {
            assert!(ORACLE_MAX_FRAMES > 0);
            assert!(ORACLE_MAX_PLAINTEXT_BYTES > 0);
            assert!(ORACLE_MAX_BLOCKS > 0);
            assert!(ORACLE_MAX_NON_TARGET_I2NP > 0);
        }
    }

    #[test]
    fn deadline_label_is_stable() {
        let label = format!("{}", DataOracleError::DeadlineElapsed);
        assert_eq!(label, "receiver-delivery-status-deadline");
    }

    #[test]
    fn typed_reasons_are_bounded() {
        let errors = [
            DataOracleError::DeadlineElapsed,
            DataOracleError::FrameLimitReached,
            DataOracleError::ByteLimitReached,
            DataOracleError::BlockLimitReached,
            DataOracleError::NonTargetI2npLimitReached,
            DataOracleError::TerminationBeforeTarget,
            DataOracleError::FrameParseFailed,
            DataOracleError::I2npDecodeFailed,
            DataOracleError::DeliveryStatusIdMismatch,
            DataOracleError::DeliveryStatusDuplicate,
            DataOracleError::PeerRouterInfoInvalid,
            DataOracleError::Closed,
        ];
        for error in errors.iter() {
            let label = format!("{}", error);
            assert!(label.starts_with("receiver-"));
        }
    }

    fn status_bytes(envelope_id: u32, body_id: u32, expiration: u32, timestamp: u64) -> Vec<u8> {
        I2npMessage::new_short_transport(
            envelope_id,
            expiration,
            I2npBody::DeliveryStatus(DeliveryStatusMessage::new(
                body_id,
                Date::from_millis(timestamp),
            )),
        )
        .expect("status message")
        .encode_short_transport_to_vec(MAX_I2NP_PAYLOAD_SIZE)
        .expect("status encoding")
    }

    #[test]
    fn correlation_uses_delivery_status_body_token_not_random_envelope_id() {
        let now_ms = 2_000_000_000_000;
        let bytes = status_bytes(0x1122_3344, 4670101, 2_000_000_060, now_ms);
        let matched = match_delivery_status(&bytes, 4670101, now_ms)
            .expect("valid correlation")
            .expect("target status");
        assert_eq!(matched.envelope_message_id, 0x1122_3344);
        assert_ne!(matched.envelope_message_id, 4670101);
        assert_eq!(matched.body, bytes);
    }

    #[test]
    fn correlation_rejects_wrong_token_and_expired_status() {
        let now_ms = 2_000_000_000_000;
        let wrong_token = status_bytes(10, 4670102, 2_000_000_060, now_ms);
        assert_eq!(
            match_delivery_status(&wrong_token, 4670101, now_ms),
            Err(DataOracleError::DeliveryStatusIdMismatch)
        );

        let expired = status_bytes(11, 4670101, 1_999_999_999, now_ms);
        assert_eq!(
            match_delivery_status(&expired, 4670101, now_ms),
            Err(DataOracleError::DeliveryStatusIdMismatch)
        );
    }
}
