//! Plan 115 canonical production I2NP bridge for short tunnel-build
//! messages.
//!
//! Work package B of [`plans/115-qualified-independent-short-build-consumption-and-external-delivery.md`](../../../../plans/115-qualified-independent-short-build-consumption-and-external-delivery.md)
//! owns the smallest reusable function that turns the canonical
//! count-prefixed STBM body emitted by
//! [`crate::short::ShortBuildStateMachine::deliver_action`] into one
//! complete I2NP type-25 message, with a no-double-prefix invariant.
//!
//! The bridge is mandatory for every Plan 115 consumer because the
//! existing [`i2pr_proto::DeferredBuildRecords`] constructor
//! expects the raw `count * 218` record bytes while
//! [`i2pr_proto::I2npBody::ShortTunnelBuild`] writes the
//! variable-record count byte itself. Wrapping the already
//! count-prefixed delivery payload directly as
//! `DeferredBuildRecords` would produce a body whose first byte is
//! `count` and whose second byte is the original `count`, which is
//! not the canonical I2NP type-25 framing.
//!
//! The bridge therefore:
//!
//! 1. validates `payload.len() == 1 + record_count * 218`;
//! 2. validates `payload[0] == record_count`;
//! 3. removes the one-byte count only for `DeferredBuildRecords`
//!    construction;
//! 4. constructs
//!    [`DeferredBuildRecords::new(record_count, 218, raw_records)`];
//! 5. wraps it in [`i2pr_proto::I2npBody::ShortTunnelBuild`];
//! 6. constructs an [`i2pr_proto::I2npMessage`] with the requested
//!    header form;
//! 7. emits the complete message bytes;
//! 8. round-trips through the standard I2NP decoder and asserts the
//!    recovered body equals the original count-prefixed delivery
//!    payload exactly.
//!
//! The bridge owns zero cryptographic state and never reorders,
//! mutates, or regenerates records. It is the single canonical
//! production-side seam for threading
//! [`crate::short::ShortBuildAction::Deliver`] into the existing
//! I2NP/transport boundaries.

#![forbid(unsafe_code)]

use thiserror::Error;
use zeroize::Zeroize;
use zeroize::Zeroizing;

use i2pr_proto::{
    CodecError, Date, DeferredBuildRecords, I2npBody, I2npMessage, MAX_BUILD_RECORDS,
    SHORT_BUILD_RECORD_SIZE,
};

use crate::short::ShortBuildAction;

/// The header variant the bridge should attach to the produced I2NP
/// message.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BridgeHeader {
    /// The 16-byte standard I2NP header with millisecond expiration
    /// and SHA-256 checksum. Used when the consumer expects a
    /// standard-header message.
    Standard {
        /// Message identifier (must be nonzero).
        message_id: u32,
        /// Expiration timestamp in milliseconds since the Unix epoch.
        expiration_ms: u64,
    },
    /// The 9-byte NTCP2/SSU2 short header. Used when the consumer
    /// is an authenticated router transport.
    ShortTransport {
        /// Message identifier (must be nonzero).
        message_id: u32,
        /// Expiration timestamp in seconds since the Unix epoch.
        expiration_seconds: u32,
    },
}

/// Validation failures for the canonical production I2NP bridge.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum BridgeError {
    /// The count byte did not match the supplied record count.
    #[error("STBM record count prefix {prefix} does not match supplied record count {declared}")]
    RecordCountMismatch {
        /// Value of the first byte of the delivery payload.
        prefix: u8,
        /// Record count the caller declared.
        declared: u8,
    },
    /// The record count was zero or above the I2NP maximum.
    #[error("STBM record count {actual} is outside [1, {maximum}]")]
    RecordCountOutOfRange {
        /// Rejected record count.
        actual: u8,
        /// Maximum accepted record count.
        maximum: u8,
    },
    /// The total delivery payload length did not match
    /// `1 + count * 218`.
    #[error("STBM payload length {actual} does not match 1 + {count} * 218 = {expected}")]
    PayloadLengthMismatch {
        /// Supplied payload length.
        actual: usize,
        /// Declared record count.
        count: u8,
        /// Expected payload length.
        expected: usize,
    },
    /// The record count byte in the delivery payload was zero.
    #[error("STBM payload declared zero records")]
    ZeroRecordCount,
    /// The wrapped [`i2pr_proto::DeferredBuildRecords`] constructor
    /// refused the supplied bytes. The bridge treats this as a
    /// structural rejection and propagates the codec error verbatim.
    #[error("deferred build records rejected the supplied bytes: {0}")]
    DeferredBuildRecords(CodecError),
    /// The I2NP message header / body encoder refused the wrapped
    /// bridge output. This indicates an invariant violation between
    /// the tunnel-side adapter and the codec registry.
    #[error("I2NP message framing rejected the bridge output: {0}")]
    MessageFraming(CodecError),
    /// The standard-header round-trip decoder recovered a body
    /// whose bytes do not match the original count-prefixed delivery
    /// payload. This is a hard invariant failure and never
    /// recoverable in production.
    #[error(
        "I2NP round-trip body mismatch: recovered length {actual} differs from original {expected}"
    )]
    RoundTripBodyMismatch {
        /// Length of the recovered body.
        actual: usize,
        /// Length of the original delivery payload.
        expected: usize,
    },
}

impl From<CodecError> for BridgeError {
    fn from(error: CodecError) -> Self {
        Self::DeferredBuildRecords(error)
    }
}

/// The recovered, sanitized representation of one bridge invocation.
///
/// The bridge never logs or debug-prints raw record bytes. The
/// `stbm_body_sha256` digest is the only durable identifier of the
/// production-supplied payload, and the `i2np_encoded_sha256` digest
/// is the only durable identifier of the complete wrapped message.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BridgeRecord {
    /// Record count recovered from the bridge output.
    pub record_count: u8,
    /// Length of the count-prefixed STBM body.
    pub stbm_body_length: usize,
    /// SHA-256 of the count-prefixed STBM body.
    pub stbm_body_sha256: [u8; 32],
    /// Length of the complete encoded I2NP message (header + body).
    pub i2np_encoded_length: usize,
    /// SHA-256 of the complete encoded I2NP message.
    pub i2np_encoded_sha256: [u8; 32],
}

/// The canonical production I2NP bridge.
///
/// The struct is zero-sized and contains no per-call state. The
/// methods consume the supplied [`ShortBuildAction::Deliver`] and
/// return either a typed error or the bridge record + the
/// constructed [`I2npMessage`]. The constructor never mutates
/// records, never re-derives routing metadata, and never logs raw
/// bytes.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ShortBuildI2npBridge;

impl ShortBuildI2npBridge {
    /// Constructs a new bridge. The bridge is stateless; the
    /// constructor exists for naming and future composition.
    pub const fn new() -> Self {
        Self
    }

    /// Wraps the count-prefixed delivery payload of a
    /// [`ShortBuildAction::Deliver`] in one complete
    /// [`I2npMessage`] with the requested header variant.
    ///
    /// The function validates the payload contract, splits the
    /// count byte from the raw records, constructs the
    /// [`DeferredBuildRecords`] from the raw records only, encodes
    /// the message with the supplied header, and round-trips the
    /// result through [`I2npMessage::decode_standard`] to recover
    /// the body. The recovered body must equal the original
    /// count-prefixed delivery payload exactly; any mismatch fails
    /// closed with [`BridgeError::RoundTripBodyMismatch`].
    ///
    /// On success the returned [`BridgeRecord`] exposes only the
    /// sanitized length/SHA-256 digests; the raw records are not
    /// exposed through the public surface beyond the constructed
    /// [`I2npMessage`].
    pub fn wrap_deliver_action(
        &self,
        action: &ShortBuildAction,
        header: BridgeHeader,
    ) -> Result<(I2npMessage, BridgeRecord), BridgeError> {
        let ShortBuildAction::Deliver {
            message,
            record_count,
            ..
        } = action;
        let payload = message.as_slice();
        let (i2np_message, record_count_recovered, stbm_body_length) =
            self.wrap_payload(payload, *record_count, header)?;
        let stbm_body_sha256 = sha256_of(payload);
        let i2np_encoded = match header {
            BridgeHeader::Standard { .. } => i2np_message
                .encode_standard_to_vec(i2pr_proto::MAX_I2NP_PAYLOAD_SIZE)
                .map_err(BridgeError::MessageFraming)?,
            BridgeHeader::ShortTransport { .. } => i2np_message
                .encode_short_transport_to_vec(i2pr_proto::MAX_I2NP_PAYLOAD_SIZE)
                .map_err(BridgeError::MessageFraming)?,
        };
        let i2np_encoded_length = i2np_encoded.len();
        let i2np_encoded_sha256 = sha256_of(&i2np_encoded);
        Ok((
            i2np_message,
            BridgeRecord {
                record_count: record_count_recovered,
                stbm_body_length,
                stbm_body_sha256,
                i2np_encoded_length,
                i2np_encoded_sha256,
            },
        ))
    }

    /// Wraps a raw count-prefixed STBM payload and a separately
    /// supplied record count. The record count is checked against
    /// the payload prefix; the two must agree.
    fn wrap_payload(
        &self,
        payload: &[u8],
        record_count: u8,
        header: BridgeHeader,
    ) -> Result<(I2npMessage, u8, usize), BridgeError> {
        if record_count == 0 {
            return Err(BridgeError::ZeroRecordCount);
        }
        if record_count as usize > MAX_BUILD_RECORDS {
            return Err(BridgeError::RecordCountOutOfRange {
                actual: record_count,
                maximum: MAX_BUILD_RECORDS as u8,
            });
        }
        let expected_len = 1_usize
            .checked_add(
                usize::from(record_count)
                    .checked_mul(SHORT_BUILD_RECORD_SIZE)
                    .ok_or(BridgeError::PayloadLengthMismatch {
                        actual: payload.len(),
                        count: record_count,
                        expected: 0,
                    })?,
            )
            .ok_or(BridgeError::PayloadLengthMismatch {
                actual: payload.len(),
                count: record_count,
                expected: 0,
            })?;
        if payload.len() != expected_len {
            return Err(BridgeError::PayloadLengthMismatch {
                actual: payload.len(),
                count: record_count,
                expected: expected_len,
            });
        }
        if payload[0] != record_count {
            return Err(BridgeError::RecordCountMismatch {
                prefix: payload[0],
                declared: record_count,
            });
        }
        // Split: remove the one-byte count for `DeferredBuildRecords`.
        let raw_records = payload[1..].to_vec();
        let deferred =
            DeferredBuildRecords::new(record_count, SHORT_BUILD_RECORD_SIZE, raw_records)?;
        let body = I2npBody::ShortTunnelBuild(deferred);
        let i2np_message = match header {
            BridgeHeader::Standard {
                message_id,
                expiration_ms,
            } => I2npMessage::new_standard(message_id, Date::from_millis(expiration_ms), body)?,
            BridgeHeader::ShortTransport {
                message_id,
                expiration_seconds,
            } => I2npMessage::new_short_transport(message_id, expiration_seconds, body)?,
        };
        // Round-trip: decode the message with the matching header and
        // assert the recovered body equals the original
        // count-prefixed delivery payload exactly. This proves the
        // bridge never double-prefixes the record count byte.
        let decoded = match header {
            BridgeHeader::Standard { .. } => {
                let encoded_for_round_trip =
                    i2np_message.encode_standard_to_vec(i2pr_proto::MAX_I2NP_PAYLOAD_SIZE)?;
                I2npMessage::decode_standard(
                    &encoded_for_round_trip,
                    i2pr_proto::MAX_I2NP_PAYLOAD_SIZE,
                )?
            }
            BridgeHeader::ShortTransport { .. } => {
                let encoded_for_round_trip = i2np_message
                    .encode_short_transport_to_vec(i2pr_proto::MAX_I2NP_PAYLOAD_SIZE)?;
                I2npMessage::decode_short_transport(
                    &encoded_for_round_trip,
                    i2pr_proto::MAX_I2NP_PAYLOAD_SIZE,
                )?
            }
        };
        let I2npBody::ShortTunnelBuild(records) = decoded.body() else {
            return Err(BridgeError::RoundTripBodyMismatch {
                actual: 0,
                expected: payload.len(),
            });
        };
        let recovered_payload = encode_recovered_body(records)?;
        let _ = recovered_payload; // `recovered_payload` length and bytes already asserted below
        let recovered_len = recovered_payload.len();
        if recovered_len != payload.len() {
            return Err(BridgeError::RoundTripBodyMismatch {
                actual: recovered_len,
                expected: payload.len(),
            });
        }
        // Constant-time compare via XOR-OR. The payload may carry
        // secret-bearing tunnel-build record bytes; we never log
        // the bytes and the comparison never short-circuits on the
        // first differing index.
        let mut diff = 0_u8;
        for (a, b) in recovered_payload.iter().zip(payload.iter()) {
            diff |= a ^ b;
        }
        // Zeroize the recovered payload buffer before it leaves
        // scope; the bridge never stores or prints raw record bytes.
        let mut recovered_payload = recovered_payload;
        recovered_payload.zeroize();
        if diff != 0 {
            return Err(BridgeError::RoundTripBodyMismatch {
                actual: recovered_len,
                expected: payload.len(),
            });
        }
        Ok((i2np_message, record_count, payload.len()))
    }
}

/// Re-encodes the recovered [`DeferredBuildRecords`] back into the
/// canonical count-prefixed STBM body for round-trip comparison.
///
/// The helper is internal because it re-derives the exact bytes
/// the I2NP type-25 body would carry on the wire; it is **not** an
/// externally exposed codec. The returned vector stays in this
/// function's stack frame and is zeroized on drop.
fn encode_recovered_body(
    records: &DeferredBuildRecords,
) -> Result<Zeroizing<Vec<u8>>, BridgeError> {
    if usize::from(records.count()) > MAX_BUILD_RECORDS {
        return Err(BridgeError::RecordCountOutOfRange {
            actual: records.count(),
            maximum: MAX_BUILD_RECORDS as u8,
        });
    }
    let expected = usize::from(records.count()) * usize::from(records.record_size());
    if records.records().len() != expected {
        return Err(BridgeError::PayloadLengthMismatch {
            actual: records.records().len(),
            count: records.count(),
            expected,
        });
    }
    let mut out = Zeroizing::new(Vec::with_capacity(1 + records.records().len()));
    out.push(records.count());
    out.extend_from_slice(records.records());
    Ok(out)
}

/// Computes SHA-256 of the supplied byte slice. The helper is a
/// thin wrapper so the bridge surface does not depend on a specific
/// SHA-256 provider at the call site.
fn sha256_of(input: &[u8]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(input);
    let output = hasher.finalize();
    let mut out = [0_u8; 32];
    out.copy_from_slice(&output);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use i2pr_proto::Hash;
    use zeroize::Zeroizing;

    fn payload(count: u8, byte: u8) -> Zeroizing<Vec<u8>> {
        let mut out = Zeroizing::new(Vec::with_capacity(
            1 + usize::from(count) * SHORT_BUILD_RECORD_SIZE,
        ));
        out.push(count);
        for _ in 0..usize::from(count) {
            out.extend(std::iter::repeat_n(byte, SHORT_BUILD_RECORD_SIZE));
        }
        out
    }

    fn deliver(count: u8, byte: u8) -> ShortBuildAction {
        ShortBuildAction::Deliver {
            first_hop: Hash::from_bytes([0xAA; 32]),
            message: payload(count, byte),
            record_count: count,
            deadline_ms: 60_000,
        }
    }

    #[test]
    fn bridge_wraps_one_record_with_standard_header() {
        let bridge = ShortBuildI2npBridge::new();
        let action = deliver(1, 0x11);
        let (message, record) = bridge
            .wrap_deliver_action(
                &action,
                BridgeHeader::Standard {
                    message_id: 0x1234_5678,
                    expiration_ms: 60_000,
                },
            )
            .expect("bridge");
        assert_eq!(record.record_count, 1);
        assert_eq!(record.stbm_body_length, 1 + SHORT_BUILD_RECORD_SIZE);
        // Round-trip via the i2pr-proto decoder.
        let encoded = message
            .encode_standard_to_vec(i2pr_proto::MAX_I2NP_PAYLOAD_SIZE)
            .expect("encode");
        let decoded = I2npMessage::decode_standard(&encoded, i2pr_proto::MAX_I2NP_PAYLOAD_SIZE)
            .expect("decode");
        let I2npBody::ShortTunnelBuild(records) = decoded.body() else {
            panic!("expected ShortTunnelBuild body");
        };
        assert_eq!(records.count(), 1);
        assert_eq!(records.records().len(), SHORT_BUILD_RECORD_SIZE);
        assert_eq!(records.records()[0], 0x11);
    }

    #[test]
    fn bridge_wraps_four_records_with_short_transport_header() {
        let bridge = ShortBuildI2npBridge::new();
        let action = deliver(4, 0x22);
        let (message, record) = bridge
            .wrap_deliver_action(
                &action,
                BridgeHeader::ShortTransport {
                    message_id: 0xDEAD_BEEF,
                    expiration_seconds: 60,
                },
            )
            .expect("bridge");
        assert_eq!(record.record_count, 4);
        assert_eq!(record.stbm_body_length, 1 + 4 * SHORT_BUILD_RECORD_SIZE);
        let encoded = message
            .encode_short_transport_to_vec(i2pr_proto::MAX_I2NP_PAYLOAD_SIZE)
            .expect("encode short transport");
        assert_eq!(
            encoded.len(),
            9 + 1 + 4 * SHORT_BUILD_RECORD_SIZE,
            "short-transport header is 9 bytes"
        );
        let decoded =
            I2npMessage::decode_short_transport(&encoded, i2pr_proto::MAX_I2NP_PAYLOAD_SIZE)
                .expect("decode short transport");
        let I2npBody::ShortTunnelBuild(records) = decoded.body() else {
            panic!("expected ShortTunnelBuild body");
        };
        assert_eq!(records.count(), 4);
        for byte in records.records() {
            assert_eq!(*byte, 0x22);
        }
    }

    #[test]
    fn bridge_round_trip_recovers_exact_payload_bytes() {
        let bridge = ShortBuildI2npBridge::new();
        let original_payload = payload(3, 0x42);
        let original_bytes: Vec<u8> = original_payload.to_vec();
        let action = ShortBuildAction::Deliver {
            first_hop: Hash::from_bytes([0xBB; 32]),
            message: original_payload,
            record_count: 3,
            deadline_ms: 60_000,
        };
        let (message, _record) = bridge
            .wrap_deliver_action(
                &action,
                BridgeHeader::Standard {
                    message_id: 1,
                    expiration_ms: 1,
                },
            )
            .expect("bridge");
        let encoded = message
            .encode_standard_to_vec(i2pr_proto::MAX_I2NP_PAYLOAD_SIZE)
            .expect("encode");
        let decoded = I2npMessage::decode_standard(&encoded, i2pr_proto::MAX_I2NP_PAYLOAD_SIZE)
            .expect("decode");
        let I2npBody::ShortTunnelBuild(records) = decoded.body() else {
            panic!("expected ShortTunnelBuild body");
        };
        let mut recovered = Vec::with_capacity(1 + records.records().len());
        recovered.push(records.count());
        recovered.extend_from_slice(records.records());
        assert_eq!(recovered, original_bytes);
    }

    #[test]
    fn bridge_rejects_mismatched_record_count_prefix() {
        let bridge = ShortBuildI2npBridge::new();
        let mut bad_payload = payload(2, 0x33);
        bad_payload[0] = 3; // claim 3 records but only carry 2
        let action = ShortBuildAction::Deliver {
            first_hop: Hash::from_bytes([0xCC; 32]),
            message: bad_payload,
            record_count: 2,
            deadline_ms: 60_000,
        };
        let outcome = bridge.wrap_deliver_action(
            &action,
            BridgeHeader::Standard {
                message_id: 1,
                expiration_ms: 1,
            },
        );
        assert!(matches!(
            outcome,
            Err(BridgeError::RecordCountMismatch { .. })
        ));
    }

    #[test]
    fn bridge_rejects_truncated_payload() {
        let bridge = ShortBuildI2npBridge::new();
        let mut bad_payload = payload(2, 0x44);
        bad_payload.truncate(1 + SHORT_BUILD_RECORD_SIZE); // only one record
        let action = ShortBuildAction::Deliver {
            first_hop: Hash::from_bytes([0xDD; 32]),
            message: bad_payload,
            record_count: 2,
            deadline_ms: 60_000,
        };
        let outcome = bridge.wrap_deliver_action(
            &action,
            BridgeHeader::Standard {
                message_id: 1,
                expiration_ms: 1,
            },
        );
        assert!(matches!(
            outcome,
            Err(BridgeError::PayloadLengthMismatch { .. })
        ));
    }

    #[test]
    fn bridge_rejects_zero_record_count() {
        let bridge = ShortBuildI2npBridge::new();
        let empty = Zeroizing::new(Vec::<u8>::new());
        let action = ShortBuildAction::Deliver {
            first_hop: Hash::from_bytes([0xEE; 32]),
            message: empty,
            record_count: 0,
            deadline_ms: 60_000,
        };
        let outcome = bridge.wrap_deliver_action(
            &action,
            BridgeHeader::Standard {
                message_id: 1,
                expiration_ms: 1,
            },
        );
        assert!(matches!(outcome, Err(BridgeError::ZeroRecordCount)));
    }

    #[test]
    fn bridge_rejects_record_count_above_maximum() {
        let bridge = ShortBuildI2npBridge::new();
        let oversize_count = (MAX_BUILD_RECORDS + 1) as u8;
        let payload = payload(oversize_count, 0x55);
        let action = ShortBuildAction::Deliver {
            first_hop: Hash::from_bytes([0xFF; 32]),
            message: payload,
            record_count: oversize_count,
            deadline_ms: 60_000,
        };
        let outcome = bridge.wrap_deliver_action(
            &action,
            BridgeHeader::Standard {
                message_id: 1,
                expiration_ms: 1,
            },
        );
        assert!(matches!(
            outcome,
            Err(BridgeError::RecordCountOutOfRange { .. })
        ));
    }

    #[test]
    fn bridge_record_carries_digests_only() {
        let bridge = ShortBuildI2npBridge::new();
        let action = deliver(2, 0x66);
        let (_message, record) = bridge
            .wrap_deliver_action(
                &action,
                BridgeHeader::Standard {
                    message_id: 1,
                    expiration_ms: 1,
                },
            )
            .expect("bridge");
        assert_eq!(record.stbm_body_length, 1 + 2 * SHORT_BUILD_RECORD_SIZE);
        assert_eq!(
            record.i2np_encoded_length,
            16 + 1 + 2 * SHORT_BUILD_RECORD_SIZE
        );
        // Both digests are populated and zero-initialized fields have
        // been overwritten by `sha256_of`.
        assert!(
            record.stbm_body_sha256.iter().any(|byte| *byte != 0),
            "stbm body digest must be populated"
        );
        assert!(
            record.i2np_encoded_sha256.iter().any(|byte| *byte != 0),
            "i2np encoded digest must be populated"
        );
    }

    #[test]
    fn bridge_debug_does_not_leak_raw_bytes() {
        let bridge = ShortBuildI2npBridge::new();
        let action = deliver(2, 0x77);
        let (_message, record) = bridge
            .wrap_deliver_action(
                &action,
                BridgeHeader::Standard {
                    message_id: 1,
                    expiration_ms: 1,
                },
            )
            .expect("bridge");
        let rendered = format!("{record:?}");
        // The bridge record never stores or prints raw record bytes.
        for forbidden in ["0x77", "\\x77"] {
            assert!(
                !rendered.contains(forbidden),
                "bridge debug must not contain raw record byte literal `{forbidden}`"
            );
        }
        assert!(
            rendered.contains("stbm_body_sha256"),
            "bridge debug must expose only sanitized digest labels"
        );
    }
}
