//! Plan 291 runtime-neutral repliable-datagram substrate.
//!
//! Java I2PTunnel Streamr authenticates subscribers with signed
//! repliable datagrams (Datagram1, I2CP protocol 17) and fans media
//! out as unauthenticated raw datagrams (protocol 18). This module
//! owns the bounded wire framing, Ed25519 sender authentication,
//! and the per-destination send/receive queues. It owns no I/O, no
//! sockets, no timers, and no destination identity store: sends
//! borrow the caller's [`DestinationIdentity`], receives carry only
//! public authenticated sender material.
//!
//! Wire (see `specs/protocols/12-repliable-datagrams-streamr.md`):
//!
//! ```text
//! Datagram1 (17): from || signature || payload
//!   from      :: full Destination bytes (bounded decode)
//!   signature :: 64-byte Ed25519 signature over the payload by the
//!                from destination's signing key
//!   payload   :: application bytes, at most 1200
//! Raw (18):       payload only, at most 1200 bytes, no sender proof
//! ```
//!
//! One wire datagram is exactly one queue event: no fragmentation,
//! no reassembly, no multi-packet handshake. Oversized,
//! malformed, and unverifiable inputs are typed rejections.

#![forbid(unsafe_code)]

use std::collections::VecDeque;

use i2pr_crypto::{ROUTER_SIGNING_KEY_TYPE, SIGNATURE_LENGTH, verify_signature};
use i2pr_proto::streaming::{ClientPayload, encode_client_payload};
use i2pr_proto::{PROTOCOL_TYPE_DATAGRAM, PROTOCOL_TYPE_RAW, SignatureValue};

use crate::identity::DestinationIdentity;
use crate::streaming::transport::TransportSendRequest;

/// I2CP protocol number selected for signed repliable datagrams.
pub const DATAGRAM1_PROTOCOL: u8 = PROTOCOL_TYPE_DATAGRAM;
/// I2CP protocol number selected for raw datagrams.
pub const RAW_DATAGRAM_PROTOCOL: u8 = PROTOCOL_TYPE_RAW;
/// Hard ceiling on the application payload of one datagram in
/// either direction. Larger local payloads are rejected, never
/// fragmented; larger wire payloads are rejected, never
/// reassembled.
pub const MAX_DATAGRAM_APPLICATION_PAYLOAD: usize = 1200;
/// Hard ceiling on the decoded `from` Destination of a Datagram1.
/// Production destinations are 387 bytes; the bound leaves room
/// for future key types without opening an allocation tap.
pub const MAX_DATAGRAM_FROM_BYTES: usize = 2048;
/// Bounded receive queue per destination. Full queues reject with
/// [`DatagramError::ReceiveQueueFull`]; events are never dropped
/// silently and never reordered.
pub const MAX_DATAGRAM_RECEIVE_QUEUE: usize = 64;
/// Bounded outbound queue per destination. Full queues reject the
/// send with [`DatagramError::OutboundQueueFull`]; realtime media
/// is never buffered without bound.
pub const MAX_DATAGRAM_OUTBOUND_QUEUE: usize = 64;

/// Typed repliable-datagram failure.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum DatagramError {
    /// The protocol number is not a supported datagram type.
    #[error("unsupported datagram protocol {protocol}")]
    UnsupportedProtocol {
        /// Observed I2CP protocol number.
        protocol: u8,
    },
    /// The application payload exceeds the one-datagram ceiling.
    #[error("datagram payload {actual} exceeds the {maximum} byte ceiling")]
    PayloadTooLarge {
        /// Observed payload length.
        actual: usize,
        /// Enforced ceiling.
        maximum: usize,
    },
    /// The Datagram1 envelope is structurally invalid.
    #[error("malformed datagram1 envelope: {reason}")]
    MalformedEnvelope {
        /// Static rejection reason (no input bytes echoed).
        reason: &'static str,
    },
    /// The `from` Destination does not decode or has an
    /// unsupported signing key type.
    #[error("unverifiable datagram sender: {reason}")]
    UnverifiableSender {
        /// Static rejection reason (no key material echoed).
        reason: &'static str,
    },
    /// The Ed25519 signature does not verify over the payload.
    #[error("datagram signature invalid")]
    InvalidSignature,
    /// The local identity cannot sign (unsupported key type).
    #[error("datagram signing unavailable: {reason}")]
    SigningUnavailable {
        /// Static reason.
        reason: &'static str,
    },
    /// The client-payload envelope failed to encode.
    #[error("datagram envelope encode failed")]
    EnvelopeEncode,
    /// The bounded receive queue is full.
    #[error("datagram receive queue full")]
    ReceiveQueueFull,
    /// The bounded outbound queue is full (delivery driver is not
    /// draining; realtime payload is dropped by the caller, never
    /// buffered without bound).
    #[error("datagram outbound queue full")]
    OutboundQueueFull,
}

/// One outbound datagram request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DatagramSendRequest {
    /// SHA-256 hash of the recipient destination.
    pub destination_hash: [u8; 32],
    /// Source I2P port on the local destination.
    pub source_port: u16,
    /// Destination I2P port on the remote destination.
    pub destination_port: u16,
    /// I2CP protocol number: 17 (repliable) or 18 (raw).
    pub protocol: u8,
    /// Application payload (at most 1200 bytes).
    pub payload: Vec<u8>,
}

/// One authenticated inbound datagram event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DatagramReceiveEvent {
    /// Authenticated sender hash. For Datagram1 this is the
    /// signature-verified `from` destination; for raw datagrams
    /// (which carry no sender proof) it is the garlic-session
    /// authenticated transport peer that delivered the payload.
    pub from_hash: [u8; 32],
    /// Sender's I2P source port.
    pub source_port: u16,
    /// Local I2P destination port.
    pub destination_port: u16,
    /// I2CP protocol number (17 or 18).
    pub protocol: u8,
    /// Application payload.
    pub payload: Vec<u8>,
    /// Monotonic receipt time supplied by the caller.
    pub received_at_ms: u64,
}

/// Bounded per-destination counters (non-secret diagnostics).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DatagramCounters {
    /// Datagram1 events accepted after verification.
    pub accepted: u64,
    /// Raw events accepted.
    pub accepted_raw: u64,
    /// Inputs rejected (malformed, unverifiable, oversize,
    /// queue-full).
    pub rejected: u64,
}

/// Per-destination repliable-datagram manager.
#[derive(Debug)]
pub struct DatagramManager {
    queue: VecDeque<DatagramReceiveEvent>,
    outbound: VecDeque<crate::streaming::transport::TransportSendRequest>,
    counters: DatagramCounters,
}

impl DatagramManager {
    /// Creates an empty manager.
    pub fn new() -> Self {
        Self {
            queue: VecDeque::new(),
            outbound: VecDeque::new(),
            counters: DatagramCounters::default(),
        }
    }

    /// Returns the current counters.
    pub fn counters(&self) -> DatagramCounters {
        self.counters
    }

    /// Returns the queued event count.
    pub fn queue_len(&self) -> usize {
        self.queue.len()
    }

    /// Returns the queued outbound request count.
    pub fn outbound_queue_len(&self) -> usize {
        self.outbound.len()
    }

    /// Builds one outbound datagram [`TransportSendRequest`]
    /// and queues it for the delivery driver. Protocol 17 wraps
    /// the payload in a signed Datagram1 envelope borrowing the
    /// caller's identity; protocol 18 sends the payload raw.
    /// Anything else is rejected before any state mutates.
    pub fn send(
        &mut self,
        identity: &DestinationIdentity,
        request: &DatagramSendRequest,
    ) -> Result<TransportSendRequest, DatagramError> {
        if request.payload.len() > MAX_DATAGRAM_APPLICATION_PAYLOAD {
            return Err(DatagramError::PayloadTooLarge {
                actual: request.payload.len(),
                maximum: MAX_DATAGRAM_APPLICATION_PAYLOAD,
            });
        }
        if self.outbound.len() >= MAX_DATAGRAM_OUTBOUND_QUEUE {
            return Err(DatagramError::OutboundQueueFull);
        }
        let datagram_bytes = match request.protocol {
            DATAGRAM1_PROTOCOL => {
                let from_bytes = identity
                    .destination()
                    .encode_to_vec(MAX_DATAGRAM_FROM_BYTES)
                    .map_err(|_| DatagramError::SigningUnavailable {
                        reason: "sender destination does not encode",
                    })?;
                let signature = identity.sign(&request.payload).map_err(|_| {
                    DatagramError::SigningUnavailable {
                        reason: "sender signing key unavailable",
                    }
                })?;
                if signature.key_type() != ROUTER_SIGNING_KEY_TYPE {
                    return Err(DatagramError::SigningUnavailable {
                        reason: "sender signing key is not Ed25519",
                    });
                }
                let signature_bytes = signature.as_bytes();
                if signature_bytes.len() != SIGNATURE_LENGTH {
                    return Err(DatagramError::SigningUnavailable {
                        reason: "sender signature has unexpected length",
                    });
                }
                let mut bytes =
                    Vec::with_capacity(from_bytes.len() + SIGNATURE_LENGTH + request.payload.len());
                bytes.extend_from_slice(&from_bytes);
                bytes.extend_from_slice(signature_bytes);
                bytes.extend_from_slice(&request.payload);
                bytes
            }
            RAW_DATAGRAM_PROTOCOL => request.payload.clone(),
            protocol => return Err(DatagramError::UnsupportedProtocol { protocol }),
        };
        let envelope = ClientPayload {
            protocol: request.protocol,
            source_port: request.source_port,
            destination_port: request.destination_port,
            payload: datagram_bytes,
        };
        let application_bytes =
            encode_client_payload(&envelope).map_err(|_| DatagramError::EnvelopeEncode)?;
        let transport = TransportSendRequest {
            destination_hash: request.destination_hash,
            source_port: request.source_port,
            destination_port: request.destination_port,
            application_payload: application_bytes,
            sequence: 0,
            send_stream_id: 0,
            receive_stream_id: 0,
        };
        self.outbound.push_back(transport.clone());
        Ok(transport)
    }

    /// Processes one inbound client payload for protocol 17 or 18.
    /// Datagram1 inputs are parsed and signature-verified before
    /// queueing; raw inputs are bounded and queued stamped with
    /// the transport-authenticated sender. Accepted events are
    /// queued in arrival order.
    pub fn process_inbound(
        &mut self,
        protocol: u8,
        source_port: u16,
        destination_port: u16,
        payload_bytes: &[u8],
        transport_sender: [u8; 32],
        now_ms: u64,
    ) -> Result<(), DatagramError> {
        let event = match protocol {
            DATAGRAM1_PROTOCOL => {
                self.accept_repliable(source_port, destination_port, payload_bytes, now_ms)?
            }
            RAW_DATAGRAM_PROTOCOL => {
                if payload_bytes.len() > MAX_DATAGRAM_APPLICATION_PAYLOAD {
                    self.counters.rejected = self.counters.rejected.saturating_add(1);
                    return Err(DatagramError::PayloadTooLarge {
                        actual: payload_bytes.len(),
                        maximum: MAX_DATAGRAM_APPLICATION_PAYLOAD,
                    });
                }
                self.counters.accepted_raw = self.counters.accepted_raw.saturating_add(1);
                DatagramReceiveEvent {
                    from_hash: transport_sender,
                    source_port,
                    destination_port,
                    protocol,
                    payload: payload_bytes.to_vec(),
                    received_at_ms: now_ms,
                }
            }
            protocol => {
                self.counters.rejected = self.counters.rejected.saturating_add(1);
                return Err(DatagramError::UnsupportedProtocol { protocol });
            }
        };
        if self.queue.len() >= MAX_DATAGRAM_RECEIVE_QUEUE {
            self.counters.rejected = self.counters.rejected.saturating_add(1);
            return Err(DatagramError::ReceiveQueueFull);
        }
        self.queue.push_back(event);
        Ok(())
    }

    /// Parses and authenticates one Datagram1 envelope.
    fn accept_repliable(
        &mut self,
        source_port: u16,
        destination_port: u16,
        payload_bytes: &[u8],
        now_ms: u64,
    ) -> Result<DatagramReceiveEvent, DatagramError> {
        // The `from` Destination is self-delimiting through its
        // canonical codec: decode the prefix under the bound,
        // re-encode, and require the encoding to be an exact
        // prefix of the envelope. Non-canonical encodings fail
        // closed here.
        let from =
            i2pr_proto::Destination::decode_from_cursor(payload_bytes, MAX_DATAGRAM_FROM_BYTES)
                .map_err(|_| self.malformed("from destination does not decode"))?;
        let from_bytes = from
            .encode_to_vec(MAX_DATAGRAM_FROM_BYTES)
            .map_err(|_| self.malformed("from destination does not re-encode"))?;
        if from_bytes.is_empty() || !payload_bytes.starts_with(&from_bytes) {
            return Err(self.malformed("envelope from is not canonical"));
        }
        let rest = &payload_bytes[from_bytes.len()..];
        if rest.len() < SIGNATURE_LENGTH {
            return Err(self.malformed("envelope missing the signature"));
        }
        let (signature_bytes, application) = rest.split_at(SIGNATURE_LENGTH);
        if application.len() > MAX_DATAGRAM_APPLICATION_PAYLOAD {
            self.counters.rejected = self.counters.rejected.saturating_add(1);
            return Err(DatagramError::PayloadTooLarge {
                actual: application.len(),
                maximum: MAX_DATAGRAM_APPLICATION_PAYLOAD,
            });
        }
        let signing_key = from.signing_key();
        if signing_key.key_type() != ROUTER_SIGNING_KEY_TYPE {
            self.counters.rejected = self.counters.rejected.saturating_add(1);
            return Err(DatagramError::UnverifiableSender {
                reason: "from signing key is not Ed25519",
            });
        }
        let signature = SignatureValue::new(ROUTER_SIGNING_KEY_TYPE, signature_bytes.to_vec())
            .map_err(|_| {
                self.counters.rejected = self.counters.rejected.saturating_add(1);
                DatagramError::UnverifiableSender {
                    reason: "signature does not decode",
                }
            })?;
        verify_signature(signing_key, application, &signature).map_err(|_| {
            self.counters.rejected = self.counters.rejected.saturating_add(1);
            DatagramError::InvalidSignature
        })?;
        let from_hash = from.hash().map_err(|_| {
            self.counters.rejected = self.counters.rejected.saturating_add(1);
            DatagramError::UnverifiableSender {
                reason: "from destination does not hash",
            }
        })?;
        self.counters.accepted = self.counters.accepted.saturating_add(1);
        Ok(DatagramReceiveEvent {
            from_hash: *from_hash.as_bytes(),
            source_port,
            destination_port,
            protocol: DATAGRAM1_PROTOCOL,
            payload: application.to_vec(),
            received_at_ms: now_ms,
        })
    }

    /// Drains queued events in arrival order.
    pub fn drain_received(&mut self) -> Vec<DatagramReceiveEvent> {
        self.queue.drain(..).collect()
    }

    /// Drains queued outbound transport requests in admission
    /// order for the delivery driver.
    pub fn drain_outbound(&mut self) -> Vec<crate::streaming::transport::TransportSendRequest> {
        self.outbound.drain(..).collect()
    }

    /// Records one malformed-envelope rejection.
    fn malformed(&mut self, reason: &'static str) -> DatagramError {
        self.counters.rejected = self.counters.rejected.saturating_add(1);
        DatagramError::MalformedEnvelope { reason }
    }
}

impl Default for DatagramManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand_chacha::ChaCha8Rng;
    use rand_core::SeedableRng;

    fn test_identity(seed: u64) -> DestinationIdentity {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        DestinationIdentity::generate(&mut rng).expect("identity")
    }

    fn send_request(
        identity: &DestinationIdentity,
        protocol: u8,
        payload: &[u8],
    ) -> TransportSendRequest {
        let mut manager = DatagramManager::new();
        manager
            .send(
                identity,
                &DatagramSendRequest {
                    destination_hash: [0xA5_u8; 32],
                    source_port: 5001,
                    destination_port: 0,
                    protocol,
                    payload: payload.to_vec(),
                },
            )
            .expect("send builds")
    }

    #[test]
    fn repliable_round_trip_verifies_sender() {
        let sender = test_identity(7);
        let request = send_request(&sender, DATAGRAM1_PROTOCOL, &[0x00]);
        // The envelope carries protocol 17 with the sender ports.
        let envelope =
            i2pr_proto::streaming::decode_client_payload(&request.application_payload, 65_536)
                .expect("envelope decodes");
        assert_eq!(envelope.protocol, DATAGRAM1_PROTOCOL);
        let mut receiver = DatagramManager::new();
        receiver
            .process_inbound(
                envelope.protocol,
                envelope.source_port,
                envelope.destination_port,
                &envelope.payload,
                [0xCC_u8; 32],
                1_000,
            )
            .expect("verified receive accepts");
        let events = receiver.drain_received();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].payload, vec![0x00]);
        assert_eq!(events[0].source_port, 5001);
        assert_ne!(events[0].from_hash, [0_u8; 32]);
        assert_eq!(receiver.counters().accepted, 1);
    }

    #[test]
    fn raw_round_trip_carries_no_sender() {
        let sender = test_identity(9);
        let request = send_request(&sender, RAW_DATAGRAM_PROTOCOL, b"media");
        let envelope =
            i2pr_proto::streaming::decode_client_payload(&request.application_payload, 65_536)
                .expect("envelope decodes");
        assert_eq!(envelope.protocol, RAW_DATAGRAM_PROTOCOL);
        let mut receiver = DatagramManager::new();
        receiver
            .process_inbound(
                envelope.protocol,
                envelope.source_port,
                envelope.destination_port,
                &envelope.payload,
                [0xCC_u8; 32],
                2_000,
            )
            .expect("raw receive accepts");
        let events = receiver.drain_received();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].from_hash, [0xCC_u8; 32]);
        assert_eq!(events[0].payload, b"media");
    }

    #[test]
    fn tampered_payload_fails_verification() {
        let sender = test_identity(11);
        let request = send_request(&sender, DATAGRAM1_PROTOCOL, &[0x00]);
        let envelope =
            i2pr_proto::streaming::decode_client_payload(&request.application_payload, 65_536)
                .expect("envelope decodes");
        let mut tampered = envelope.payload.clone();
        let last = tampered.len() - 1;
        tampered[last] ^= 0xFF;
        let mut receiver = DatagramManager::new();
        let outcome = receiver.process_inbound(
            envelope.protocol,
            envelope.source_port,
            envelope.destination_port,
            &tampered,
            [0xCC_u8; 32],
            3_000,
        );
        assert_eq!(outcome, Err(DatagramError::InvalidSignature));
        assert!(receiver.drain_received().is_empty());
    }

    #[test]
    fn wrong_sender_key_fails_verification() {
        let sender = test_identity(13);
        let other = test_identity(14);
        let request = send_request(&sender, DATAGRAM1_PROTOCOL, &[0x00]);
        let envelope =
            i2pr_proto::streaming::decode_client_payload(&request.application_payload, 65_536)
                .expect("envelope decodes");
        // Re-sign nothing: swap the `from` block for another
        // destination while keeping the signature (cross-protocol
        // confusion must fail closed).
        let other_from = other
            .destination()
            .encode_to_vec(MAX_DATAGRAM_FROM_BYTES)
            .expect("from encodes");
        let from_len = sender
            .destination()
            .encode_to_vec(MAX_DATAGRAM_FROM_BYTES)
            .expect("from encodes")
            .len();
        let mut swapped = other_from;
        swapped.extend_from_slice(&envelope.payload[from_len..]);
        let mut receiver = DatagramManager::new();
        let outcome = receiver.process_inbound(
            envelope.protocol,
            envelope.source_port,
            envelope.destination_port,
            &swapped,
            [0xCC_u8; 32],
            4_000,
        );
        assert_eq!(outcome, Err(DatagramError::InvalidSignature));
    }

    #[test]
    fn oversize_payloads_reject_both_directions() {
        let sender = test_identity(15);
        let mut manager = DatagramManager::new();
        let outcome = manager.send(
            &sender,
            &DatagramSendRequest {
                destination_hash: [0xA5_u8; 32],
                source_port: 0,
                destination_port: 0,
                protocol: RAW_DATAGRAM_PROTOCOL,
                payload: vec![0x55_u8; MAX_DATAGRAM_APPLICATION_PAYLOAD + 1],
            },
        );
        assert!(matches!(
            outcome,
            Err(DatagramError::PayloadTooLarge { .. })
        ));
        let mut receiver = DatagramManager::new();
        let outcome = receiver.process_inbound(
            RAW_DATAGRAM_PROTOCOL,
            0,
            0,
            &vec![0x55_u8; MAX_DATAGRAM_APPLICATION_PAYLOAD + 1],
            [0xCC_u8; 32],
            5_000,
        );
        assert!(matches!(
            outcome,
            Err(DatagramError::PayloadTooLarge { .. })
        ));
    }

    #[test]
    fn unsupported_protocol_rejects_before_state() {
        let sender = test_identity(17);
        let mut manager = DatagramManager::new();
        let outcome = manager.send(
            &sender,
            &DatagramSendRequest {
                destination_hash: [0xA5_u8; 32],
                source_port: 0,
                destination_port: 0,
                protocol: 6,
                payload: vec![0x00],
            },
        );
        assert_eq!(
            outcome,
            Err(DatagramError::UnsupportedProtocol { protocol: 6 })
        );
        let mut receiver = DatagramManager::new();
        let outcome = receiver.process_inbound(19, 0, 0, &[0x00], [0xCC_u8; 32], 6_000);
        assert_eq!(
            outcome,
            Err(DatagramError::UnsupportedProtocol { protocol: 19 })
        );
        assert_eq!(receiver.counters().rejected, 1);
    }

    #[test]
    fn full_queue_rejects_without_silent_drop() {
        let sender = test_identity(19);
        let request = send_request(&sender, RAW_DATAGRAM_PROTOCOL, b"x");
        let envelope =
            i2pr_proto::streaming::decode_client_payload(&request.application_payload, 65_536)
                .expect("envelope decodes");
        let mut receiver = DatagramManager::new();
        for _ in 0..MAX_DATAGRAM_RECEIVE_QUEUE {
            receiver
                .process_inbound(
                    envelope.protocol,
                    envelope.source_port,
                    envelope.destination_port,
                    &envelope.payload,
                    [0xCC_u8; 32],
                    7_000,
                )
                .expect("queue accepts to capacity");
        }
        let outcome = receiver.process_inbound(
            envelope.protocol,
            envelope.source_port,
            envelope.destination_port,
            &envelope.payload,
            [0xCC_u8; 32],
            7_000,
        );
        assert_eq!(outcome, Err(DatagramError::ReceiveQueueFull));
        assert_eq!(receiver.drain_received().len(), MAX_DATAGRAM_RECEIVE_QUEUE);
    }

    #[test]
    fn truncated_envelope_is_malformed() {
        let mut receiver = DatagramManager::new();
        assert!(
            receiver
                .process_inbound(DATAGRAM1_PROTOCOL, 0, 0, &[], [0xCC_u8; 32], 8_000)
                .is_err()
        );
        assert!(
            receiver
                .process_inbound(DATAGRAM1_PROTOCOL, 0, 0, &[0x01; 64], [0xCC_u8; 32], 8_000)
                .is_err()
        );
        assert_eq!(receiver.counters().rejected, 2);
    }
}
