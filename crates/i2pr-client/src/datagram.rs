//! Runtime-neutral repliable-datagram substrate (Plan 291, extended by Plan 368).
//!
//! Java I2PTunnel Streamr authenticates subscribers with signed
//! repliable datagrams (Datagram1, I2CP protocol 17) and fans media
//! out as unauthenticated raw datagrams (typically protocol 18). Plan 368 adds
//! Proposal 163 Datagram2/Datagram3 (protocols 19/20). This module
//! owns bounded wire framing, Ed25519 sender authentication,
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
//!   payload   :: application bytes
//! Raw (18):       payload only, no sender proof
//! Datagram2 (19): from || flags || options? || delegation? || payload || signature
//! Datagram3 (20): fromhash || flags || options? || payload (unauthenticated)
//! ```
//!
//! One wire datagram is exactly one queue event: no fragmentation,
//! no reassembly, no multi-packet handshake. Oversized,
//! malformed, and unverifiable inputs are typed rejections.

#![forbid(unsafe_code)]

use std::collections::VecDeque;

use i2pr_crypto::{ROUTER_SIGNING_KEY_TYPE, SIGNATURE_LENGTH, verify_signature};
use i2pr_proto::streaming::{ClientPayload, encode_client_payload};
use i2pr_proto::{
    Destination, Mapping, PROTOCOL_TYPE_DATAGRAM, PROTOCOL_TYPE_DATAGRAM2, PROTOCOL_TYPE_DATAGRAM3,
    PROTOCOL_TYPE_RAW, SignatureValue, SigningKeyType, SigningPublicKey,
};

use crate::identity::DestinationIdentity;
use crate::streaming::transport::TransportSendRequest;

/// I2CP protocol number selected for signed repliable datagrams.
pub const DATAGRAM1_PROTOCOL: u8 = PROTOCOL_TYPE_DATAGRAM;
/// I2CP protocol number selected for raw datagrams.
pub const RAW_DATAGRAM_PROTOCOL: u8 = PROTOCOL_TYPE_RAW;
/// I2CP protocol number for Proposal 163 authenticated Datagram2.
pub const DATAGRAM2_PROTOCOL: u8 = PROTOCOL_TYPE_DATAGRAM2;
/// I2CP protocol number for Proposal 163 unauthenticated Datagram3.
pub const DATAGRAM3_PROTOCOL: u8 = PROTOCOL_TYPE_DATAGRAM3;
/// Hard ceiling on the application payload of one datagram in
/// either direction. Larger local payloads are rejected, never
/// fragmented; larger wire payloads are rejected, never
/// reassembled.
pub const MAX_DATAGRAM_APPLICATION_PAYLOAD: usize =
    i2pr_proto::streaming::MAX_APPLICATION_PAYLOAD_BYTES - 256;
/// Maximum encoded canonical Mapping options accepted by Datagram2/3.
pub const MAX_DATAGRAM_OPTIONS_BYTES: usize = 1024;
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
/// Bound on authenticated Datagram2 replay entries per destination.
pub const MAX_DATAGRAM2_REPLAY_ENTRIES: usize = 1024;
/// Datagram2 replay entries expire after this monotonic interval.
pub const DATAGRAM2_REPLAY_TTL_MS: u64 = 10 * 60 * 1000;
const DATAGRAM2_FLAG_OPTIONS: u16 = 1 << 4;
const DATAGRAM2_FLAG_OFFLINE: u16 = 1 << 5;
const DATAGRAM2_FLAGS_VERSION: u16 = 2;
const DATAGRAM3_FLAG_OPTIONS: u16 = 1 << 4;
const DATAGRAM3_FLAGS_VERSION: u16 = 3;

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
    /// The Datagram2 recipient hash does not match this destination.
    #[error("datagram2 recipient binding invalid")]
    InvalidRecipient,
    /// A Datagram2 replay was rejected.
    #[error("datagram2 replay rejected")]
    Replay,
    /// The bounded Datagram2 replay cache contains only unexpired entries.
    /// New messages are rejected rather than evicting an entry and reopening
    /// its replay window.
    #[error("datagram2 replay cache is full")]
    ReplayCacheFull,
    /// A Datagram2 delegated signing key is invalid or expired.
    #[error("datagram2 offline signing delegation invalid")]
    InvalidOfflineSignature,
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
    /// I2CP protocol number: 17–20.
    pub protocol: u8,
    /// Bounded application payload.
    pub payload: Vec<u8>,
    /// Canonical I2P mapping options used by protocols 19 and 20.
    pub options: Option<Mapping>,
}

/// Borrowed inbound datagram context, including the local recipient hash
/// required to authenticate Proposal 163 Datagram2 messages.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DatagramInboundRequest<'a> {
    /// I2CP protocol number.
    pub protocol: u8,
    /// Sender's I2P source port.
    pub source_port: u16,
    /// Local I2P destination port.
    pub destination_port: u16,
    /// Encoded I2CP application payload.
    pub payload: &'a [u8],
    /// Authenticated transport peer hash for unauthenticated envelopes.
    pub transport_sender: [u8; 32],
    /// Local destination hash used by recipient-bound Datagram2 signatures.
    pub recipient_hash: [u8; 32],
    /// Monotonic receipt time in milliseconds.
    pub now_ms: u64,
    /// Wall-clock receipt time in seconds.
    pub now_seconds: u32,
}

/// One authenticated inbound datagram event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DatagramReceiveEvent {
    /// Authenticated sender hash. For Datagram1 this is the
    /// signature-verified `from` destination; for raw datagrams
    /// (which carry no sender proof) it is the garlic-session
    /// authenticated transport peer that delivered the payload.
    pub from_hash: [u8; 32],
    /// Canonical complete sender Destination for authenticated Datagram1/2.
    /// Datagram3 and Raw expose only a hash or transport peer and leave this
    /// unset.
    pub from_destination: Option<Vec<u8>>,
    /// Sender's I2P source port.
    pub source_port: u16,
    /// Local I2P destination port.
    pub destination_port: u16,
    /// I2CP protocol number (17–20).
    pub protocol: u8,
    /// Application payload.
    pub payload: Vec<u8>,
    /// Original I2CP application bytes before Datagram1/2/3 decoding. RAW
    /// subsessions use these bytes when their `LISTEN_PROTOCOL` selects a
    /// reserved datagram protocol.
    pub raw_payload: Vec<u8>,
    /// Authenticated sender metadata. False for Datagram3 and Raw.
    pub sender_authenticated: bool,
    /// Bounded protocol mapping, when present.
    pub options: Option<Mapping>,
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
    /// Authenticated Datagram2 events accepted after signature and replay checks.
    pub accepted_datagram2: u64,
    /// Unauthenticated Datagram3 events accepted.
    pub accepted_datagram3: u64,
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
    replay: VecDeque<([u8; 32], u64)>,
}

impl DatagramManager {
    /// Creates an empty manager.
    pub fn new() -> Self {
        Self {
            queue: VecDeque::new(),
            outbound: VecDeque::new(),
            counters: DatagramCounters::default(),
            replay: VecDeque::new(),
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
    /// and queues it for the delivery driver. Protocols 17/19 sign
    /// using the caller's identity; non-reserved RAW protocols send
    /// the payload directly and protocol 20 includes an unauthenticated
    /// source hash.
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
            protocol if is_raw_application_protocol(protocol) => request.payload.clone(),
            DATAGRAM2_PROTOCOL => self.encode_datagram2(identity, request)?,
            DATAGRAM3_PROTOCOL => self.encode_datagram3(identity, request)?,
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

    /// Processes one inbound client payload. Use [`Self::process_inbound_at`]
    /// for Datagram2 because recipient binding and offline-key expiry require
    /// the local destination hash and wall-clock seconds.
    pub fn process_inbound(
        &mut self,
        protocol: u8,
        source_port: u16,
        destination_port: u16,
        payload_bytes: &[u8],
        transport_sender: [u8; 32],
        now_ms: u64,
    ) -> Result<(), DatagramError> {
        self.process_inbound_at(DatagramInboundRequest {
            protocol,
            source_port,
            destination_port,
            payload: payload_bytes,
            transport_sender,
            recipient_hash: [0_u8; 32],
            now_ms,
            now_seconds: (now_ms / 1000).min(u64::from(u32::MAX)) as u32,
        })
    }

    /// Processes a payload with the local destination hash and wall clock needed
    /// for Proposal 163 recipient binding and offline delegation expiry.
    pub fn process_inbound_at(
        &mut self,
        request: DatagramInboundRequest<'_>,
    ) -> Result<(), DatagramError> {
        let DatagramInboundRequest {
            protocol,
            source_port,
            destination_port,
            payload: payload_bytes,
            transport_sender,
            recipient_hash,
            now_ms,
            now_seconds,
        } = request;
        let event = match protocol {
            DATAGRAM1_PROTOCOL => {
                self.accept_repliable(source_port, destination_port, payload_bytes, now_ms)?
            }
            protocol if is_raw_application_protocol(protocol) => {
                if payload_bytes.len() > MAX_DATAGRAM_APPLICATION_PAYLOAD {
                    self.counters.rejected = self.counters.rejected.saturating_add(1);
                    return Err(DatagramError::PayloadTooLarge {
                        actual: payload_bytes.len(),
                        maximum: MAX_DATAGRAM_APPLICATION_PAYLOAD,
                    });
                }
                DatagramReceiveEvent {
                    from_hash: transport_sender,
                    from_destination: None,
                    source_port,
                    destination_port,
                    protocol,
                    payload: payload_bytes.to_vec(),
                    raw_payload: payload_bytes.to_vec(),
                    sender_authenticated: false,
                    options: None,
                    received_at_ms: now_ms,
                }
            }
            DATAGRAM2_PROTOCOL => self.accept_datagram2(
                source_port,
                destination_port,
                payload_bytes,
                recipient_hash,
                now_ms,
                now_seconds,
            )?,
            DATAGRAM3_PROTOCOL => {
                self.accept_datagram3(source_port, destination_port, payload_bytes, now_ms)?
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
        match protocol {
            DATAGRAM1_PROTOCOL => {
                self.counters.accepted = self.counters.accepted.saturating_add(1);
            }
            protocol if is_raw_application_protocol(protocol) => {
                self.counters.accepted_raw = self.counters.accepted_raw.saturating_add(1);
            }
            DATAGRAM2_PROTOCOL => {
                self.counters.accepted_datagram2 =
                    self.counters.accepted_datagram2.saturating_add(1);
            }
            DATAGRAM3_PROTOCOL => {
                self.counters.accepted_datagram3 =
                    self.counters.accepted_datagram3.saturating_add(1);
            }
            _ => {}
        }
        Ok(())
    }

    fn encode_datagram2(
        &self,
        identity: &DestinationIdentity,
        request: &DatagramSendRequest,
    ) -> Result<Vec<u8>, DatagramError> {
        let options = request.options.as_ref();
        let options_bytes = options
            .map(|mapping| mapping.encode_to_vec(MAX_DATAGRAM_OPTIONS_BYTES))
            .transpose()
            .map_err(|_| DatagramError::MalformedEnvelope {
                reason: "options mapping does not encode",
            })?;
        let mut flags = DATAGRAM2_FLAGS_VERSION;
        if options_bytes.is_some() {
            flags |= DATAGRAM2_FLAG_OPTIONS;
        }
        let flags_bytes = flags.to_be_bytes();
        let mut preimage = Vec::with_capacity(
            32 + 2 + options_bytes.as_ref().map_or(0, Vec::len) + request.payload.len(),
        );
        preimage.extend_from_slice(&request.destination_hash);
        preimage.extend_from_slice(&flags_bytes);
        if let Some(options) = &options_bytes {
            preimage.extend_from_slice(options);
        }
        preimage.extend_from_slice(&request.payload);
        let signature =
            identity
                .sign(&preimage)
                .map_err(|_| DatagramError::SigningUnavailable {
                    reason: "sender signing key unavailable",
                })?;
        if signature.key_type() != ROUTER_SIGNING_KEY_TYPE
            || signature.as_bytes().len() != SIGNATURE_LENGTH
        {
            return Err(DatagramError::SigningUnavailable {
                reason: "sender signing key is not Ed25519",
            });
        }
        let from = identity
            .destination()
            .encode_to_vec(MAX_DATAGRAM_FROM_BYTES)
            .map_err(|_| DatagramError::SigningUnavailable {
                reason: "sender destination does not encode",
            })?;
        let mut envelope = Vec::with_capacity(
            from.len()
                + 2
                + options_bytes.as_ref().map_or(0, Vec::len)
                + request.payload.len()
                + SIGNATURE_LENGTH,
        );
        envelope.extend_from_slice(&from);
        envelope.extend_from_slice(&flags_bytes);
        if let Some(options) = options_bytes {
            envelope.extend_from_slice(&options);
        }
        envelope.extend_from_slice(&request.payload);
        envelope.extend_from_slice(signature.as_bytes());
        Ok(envelope)
    }

    fn encode_datagram3(
        &self,
        identity: &DestinationIdentity,
        request: &DatagramSendRequest,
    ) -> Result<Vec<u8>, DatagramError> {
        let from_hash =
            identity
                .destination()
                .hash()
                .map_err(|_| DatagramError::SigningUnavailable {
                    reason: "sender destination does not hash",
                })?;
        let options_bytes = request
            .options
            .as_ref()
            .map(|mapping| mapping.encode_to_vec(MAX_DATAGRAM_OPTIONS_BYTES))
            .transpose()
            .map_err(|_| DatagramError::MalformedEnvelope {
                reason: "options mapping does not encode",
            })?;
        let mut flags = DATAGRAM3_FLAGS_VERSION;
        if options_bytes.is_some() {
            flags |= DATAGRAM3_FLAG_OPTIONS;
        }
        let mut envelope = Vec::with_capacity(
            34 + options_bytes.as_ref().map_or(0, Vec::len) + request.payload.len(),
        );
        envelope.extend_from_slice(from_hash.as_bytes());
        envelope.extend_from_slice(&flags.to_be_bytes());
        if let Some(options) = options_bytes {
            envelope.extend_from_slice(&options);
        }
        envelope.extend_from_slice(&request.payload);
        Ok(envelope)
    }

    fn accept_datagram3(
        &mut self,
        source_port: u16,
        destination_port: u16,
        bytes: &[u8],
        now_ms: u64,
    ) -> Result<DatagramReceiveEvent, DatagramError> {
        if bytes.len() < 34 {
            return Err(self.malformed("datagram3 header is truncated"));
        }
        let from_hash: [u8; 32] = bytes[..32].try_into().expect("fixed slice");
        let flags = u16::from_be_bytes([bytes[32], bytes[33]]);
        if flags & !(DATAGRAM3_FLAG_OPTIONS | 0x000f) != 0
            || flags & 0x000f != DATAGRAM3_FLAGS_VERSION
        {
            return Err(self.malformed("datagram3 flags are invalid"));
        }
        let (options, payload_start) = if flags & DATAGRAM3_FLAG_OPTIONS != 0 {
            let (mapping, consumed) = decode_mapping_prefix(&bytes[34..])
                .map_err(|_| self.malformed("datagram3 options are invalid"))?;
            (mapping, 34 + consumed)
        } else {
            (None, 34)
        };
        let payload = &bytes[payload_start..];
        if payload.len() > MAX_DATAGRAM_APPLICATION_PAYLOAD {
            self.counters.rejected = self.counters.rejected.saturating_add(1);
            return Err(DatagramError::PayloadTooLarge {
                actual: payload.len(),
                maximum: MAX_DATAGRAM_APPLICATION_PAYLOAD,
            });
        }
        Ok(DatagramReceiveEvent {
            from_hash,
            from_destination: None,
            source_port,
            destination_port,
            protocol: DATAGRAM3_PROTOCOL,
            payload: payload.to_vec(),
            raw_payload: bytes.to_vec(),
            sender_authenticated: false,
            options,
            received_at_ms: now_ms,
        })
    }

    fn accept_datagram2(
        &mut self,
        source_port: u16,
        destination_port: u16,
        bytes: &[u8],
        recipient_hash: [u8; 32],
        now_ms: u64,
        now_seconds: u32,
    ) -> Result<DatagramReceiveEvent, DatagramError> {
        let from = Destination::decode_from_cursor(bytes, MAX_DATAGRAM_FROM_BYTES)
            .map_err(|_| self.malformed("datagram2 sender destination does not decode"))?;
        let from_bytes = from
            .encode_to_vec(MAX_DATAGRAM_FROM_BYTES)
            .map_err(|_| self.malformed("datagram2 sender destination does not encode"))?;
        if from_bytes.is_empty()
            || !bytes.starts_with(&from_bytes)
            || bytes.len() < from_bytes.len() + 2 + SIGNATURE_LENGTH
        {
            return Err(self.malformed("datagram2 envelope is truncated or noncanonical"));
        }
        let flags = u16::from_be_bytes([bytes[from_bytes.len()], bytes[from_bytes.len() + 1]]);
        if flags & !(DATAGRAM2_FLAG_OPTIONS | DATAGRAM2_FLAG_OFFLINE | 0x000f) != 0
            || flags & 0x000f != DATAGRAM2_FLAGS_VERSION
        {
            return Err(self.malformed("datagram2 flags are invalid"));
        }
        let mut cursor = from_bytes.len() + 2;
        let options = if flags & DATAGRAM2_FLAG_OPTIONS != 0 {
            let (mapping, next) = decode_mapping_prefix(&bytes[cursor..])
                .map_err(|_| self.malformed("datagram2 options are invalid"))?;
            cursor += next;
            mapping
        } else {
            None
        };
        let mut signing_key = from.signing_key().clone();
        if flags & DATAGRAM2_FLAG_OFFLINE != 0 {
            let (key, next) =
                decode_offline_delegation(&bytes[cursor..], from.signing_key(), now_seconds)
                    .map_err(|_| {
                        self.counters.rejected = self.counters.rejected.saturating_add(1);
                        DatagramError::InvalidOfflineSignature
                    })?;
            signing_key = key;
            cursor += next;
        }
        let signature_len = signing_key
            .key_type()
            .signature_len()
            .ok_or(DatagramError::InvalidOfflineSignature)?;
        if bytes.len() < cursor + signature_len {
            return Err(self.malformed("datagram2 signature is truncated"));
        }
        let payload_end = bytes.len() - signature_len;
        if payload_end < cursor {
            return Err(self.malformed("datagram2 payload bounds are invalid"));
        }
        let payload = &bytes[cursor..payload_end];
        if payload.len() > MAX_DATAGRAM_APPLICATION_PAYLOAD {
            self.counters.rejected = self.counters.rejected.saturating_add(1);
            return Err(DatagramError::PayloadTooLarge {
                actual: payload.len(),
                maximum: MAX_DATAGRAM_APPLICATION_PAYLOAD,
            });
        }
        let from_hash = from
            .hash()
            .map_err(|_| self.malformed("datagram2 sender hash fails"))?;
        let mut preimage = Vec::with_capacity(32 + payload_end - from_bytes.len());
        preimage.extend_from_slice(&recipient_hash);
        preimage.extend_from_slice(&bytes[from_bytes.len()..payload_end]);
        let signature = SignatureValue::new(signing_key.key_type(), bytes[payload_end..].to_vec())
            .map_err(|_| self.malformed("datagram2 signature is malformed"))?;
        verify_signature(&signing_key, &preimage, &signature).map_err(|_| {
            self.counters.rejected = self.counters.rejected.saturating_add(1);
            DatagramError::InvalidSignature
        })?;
        let replay_hash = i2pr_proto::Hash::digest(bytes);
        while self
            .replay
            .front()
            .is_some_and(|(_, expires)| *expires <= now_ms)
        {
            self.replay.pop_front();
        }
        if self
            .replay
            .iter()
            .any(|(hash, _)| hash == replay_hash.as_bytes())
        {
            self.counters.rejected = self.counters.rejected.saturating_add(1);
            return Err(DatagramError::Replay);
        }
        if self.replay.len() >= MAX_DATAGRAM2_REPLAY_ENTRIES {
            self.counters.rejected = self.counters.rejected.saturating_add(1);
            return Err(DatagramError::ReplayCacheFull);
        }
        self.replay.push_back((
            *replay_hash.as_bytes(),
            now_ms.saturating_add(DATAGRAM2_REPLAY_TTL_MS),
        ));
        Ok(DatagramReceiveEvent {
            from_hash: *from_hash.as_bytes(),
            from_destination: Some(from_bytes),
            source_port,
            destination_port,
            protocol: DATAGRAM2_PROTOCOL,
            payload: payload.to_vec(),
            raw_payload: bytes.to_vec(),
            sender_authenticated: true,
            options,
            received_at_ms: now_ms,
        })
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
        Ok(DatagramReceiveEvent {
            from_hash: *from_hash.as_bytes(),
            from_destination: Some(from_bytes),
            source_port,
            destination_port,
            protocol: DATAGRAM1_PROTOCOL,
            payload: application.to_vec(),
            raw_payload: payload_bytes.to_vec(),
            sender_authenticated: true,
            options: None,
            received_at_ms: now_ms,
        })
    }

    /// Drains queued events in arrival order.
    pub fn drain_received(&mut self) -> Vec<DatagramReceiveEvent> {
        self.queue.drain(..).collect()
    }

    /// Drains only events owned by a SAM datagram child, preserving arrival
    /// order for both the selected child and every sibling child.
    pub fn drain_received_for(
        &mut self,
        protocol: u8,
        destination_port: u16,
    ) -> Vec<DatagramReceiveEvent> {
        let mut selected = Vec::new();
        let mut remaining = VecDeque::with_capacity(self.queue.len());
        while let Some(event) = self.queue.pop_front() {
            if event.protocol == protocol && event.destination_port == destination_port {
                selected.push(event);
            } else {
                remaining.push_back(event);
            }
        }
        self.queue = remaining;
        selected
    }

    /// Removes the oldest event for one child without draining its siblings.
    pub fn pop_received_for(
        &mut self,
        protocol: u8,
        destination_port: u16,
    ) -> Option<DatagramReceiveEvent> {
        self.pop_received_matching(protocol, destination_port, false)
    }

    /// Removes the oldest event routed by listener protocol and port. A
    /// listener protocol or port of zero is a wildcard, as in SAM PRIMARY.
    pub fn pop_received_for_listener(
        &mut self,
        listen_protocol: u8,
        listen_port: u16,
    ) -> Option<DatagramReceiveEvent> {
        self.pop_received_matching(listen_protocol, listen_port, true)
    }

    fn pop_received_matching(
        &mut self,
        protocol: u8,
        destination_port: u16,
        protocol_wildcard: bool,
    ) -> Option<DatagramReceiveEvent> {
        let index = self.queue.iter().position(|event| {
            (event.protocol == protocol || (protocol_wildcard && protocol == 0))
                && (destination_port == 0 || event.destination_port == destination_port)
        })?;
        self.queue.remove(index)
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

/// Returns whether an I2CP protocol can use RAW application payload framing.
/// Protocol 6 is Streaming and 17/19/20 have reserved datagram envelopes.
const fn is_raw_application_protocol(protocol: u8) -> bool {
    !matches!(protocol, 6 | 17 | 19 | 20)
}

fn decode_mapping_prefix(bytes: &[u8]) -> Result<(Option<Mapping>, usize), ()> {
    if bytes.len() < 2 {
        return Err(());
    }
    let body_len = usize::from(u16::from_be_bytes([bytes[0], bytes[1]]));
    let total = body_len.checked_add(2).ok_or(())?;
    if total > MAX_DATAGRAM_OPTIONS_BYTES || bytes.len() < total {
        return Err(());
    }
    let mapping = Mapping::decode(&bytes[..total], MAX_DATAGRAM_OPTIONS_BYTES).map_err(|_| ())?;
    Ok((Some(mapping), total))
}

fn decode_offline_delegation(
    bytes: &[u8],
    destination_key: &SigningPublicKey,
    now_seconds: u32,
) -> Result<(SigningPublicKey, usize), ()> {
    if bytes.len() < 6 {
        return Err(());
    }
    let expires_seconds = u32::from_be_bytes(bytes[..4].try_into().map_err(|_| ())?);
    if expires_seconds <= now_seconds {
        return Err(());
    }
    let key_type = SigningKeyType::from_code(u16::from_be_bytes([bytes[4], bytes[5]]));
    let key_len = key_type.public_key_len().ok_or(())?;
    let signature_len = destination_key.key_type().signature_len().ok_or(())?;
    let total = 6usize
        .checked_add(key_len)
        .and_then(|n| n.checked_add(signature_len))
        .ok_or(())?;
    if bytes.len() < total {
        return Err(());
    }
    let transient_key =
        SigningPublicKey::new(key_type, bytes[6..6 + key_len].to_vec()).map_err(|_| ())?;
    let delegation_signature = SignatureValue::new(
        destination_key.key_type(),
        bytes[6 + key_len..total].to_vec(),
    )
    .map_err(|_| ())?;
    let mut signed = Vec::with_capacity(6 + key_len);
    signed.extend_from_slice(&bytes[..6 + key_len]);
    verify_signature(destination_key, &signed, &delegation_signature).map_err(|_| ())?;
    Ok((transient_key, total))
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
                    options: None,
                },
            )
            .expect("send builds")
    }

    fn emitted_envelope(
        identity: &DestinationIdentity,
        protocol: u8,
        payload: &[u8],
        options: Option<Mapping>,
    ) -> (TransportSendRequest, [u8; 32]) {
        let mut manager = DatagramManager::new();
        let target = [0xA5; 32];
        let transport = manager
            .send(
                identity,
                &DatagramSendRequest {
                    destination_hash: target,
                    source_port: 41,
                    destination_port: 42,
                    protocol,
                    payload: payload.to_vec(),
                    options,
                },
            )
            .expect("datagram emits");
        (transport, target)
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
    fn datagram2_round_trip_binds_the_recipient_and_rejects_replay() {
        let sender = test_identity(31);
        let (transport, target) = emitted_envelope(&sender, DATAGRAM2_PROTOCOL, b"d2", None);
        let envelope =
            i2pr_proto::streaming::decode_client_payload(&transport.application_payload, 65_536)
                .expect("I2CP envelope");
        let mut receiver = DatagramManager::new();
        receiver
            .process_inbound_at(DatagramInboundRequest {
                protocol: envelope.protocol,
                source_port: envelope.source_port,
                destination_port: envelope.destination_port,
                payload: &envelope.payload,
                transport_sender: [0xCC; 32],
                recipient_hash: target,
                now_ms: 10_000,
                now_seconds: 1_000,
            })
            .expect("D2 verifies");
        let events = receiver.drain_received();
        assert_eq!(events[0].payload, b"d2");
        assert!(events[0].sender_authenticated);
        assert_eq!(receiver.counters().accepted_datagram2, 1);
        assert_eq!(receiver.counters().accepted_raw, 0);
        let replay = receiver.process_inbound_at(DatagramInboundRequest {
            protocol: envelope.protocol,
            source_port: envelope.source_port,
            destination_port: envelope.destination_port,
            payload: &envelope.payload,
            transport_sender: [0xCC; 32],
            recipient_hash: target,
            now_ms: 10_001,
            now_seconds: 1_000,
        });
        assert_eq!(replay, Err(DatagramError::Replay));
        let wrong_target = receiver.process_inbound_at(DatagramInboundRequest {
            protocol: envelope.protocol,
            source_port: envelope.source_port,
            destination_port: envelope.destination_port,
            payload: &envelope.payload,
            transport_sender: [0xCC; 32],
            recipient_hash: [0xA4; 32],
            now_ms: 10_002,
            now_seconds: 1_000,
        });
        assert_eq!(wrong_target, Err(DatagramError::InvalidSignature));
    }

    #[test]
    fn datagram2_full_replay_cache_does_not_evict_live_entries() {
        let sender = test_identity(34);
        let (transport, target) = emitted_envelope(&sender, DATAGRAM2_PROTOCOL, b"full", None);
        let envelope =
            i2pr_proto::streaming::decode_client_payload(&transport.application_payload, 65_536)
                .expect("I2CP envelope");
        let mut receiver = DatagramManager::new();
        receiver.replay.extend(
            (0..MAX_DATAGRAM2_REPLAY_ENTRIES)
                .map(|index| ([index as u8; 32], DATAGRAM2_REPLAY_TTL_MS + 1)),
        );

        let result = receiver.process_inbound_at(DatagramInboundRequest {
            protocol: envelope.protocol,
            source_port: envelope.source_port,
            destination_port: envelope.destination_port,
            payload: &envelope.payload,
            transport_sender: [0xCC; 32],
            recipient_hash: target,
            now_ms: 1,
            now_seconds: 1,
        });

        assert_eq!(result, Err(DatagramError::ReplayCacheFull));
        assert_eq!(receiver.replay.len(), MAX_DATAGRAM2_REPLAY_ENTRIES);
        assert!(receiver.queue.is_empty());
    }

    #[test]
    fn datagram2_accepts_valid_offline_delegation_and_rejects_expired_key() {
        let root = test_identity(32);
        let transient = test_identity(33);
        let target = [0x5A; 32];
        let from = root
            .destination()
            .encode_to_vec(MAX_DATAGRAM_FROM_BYTES)
            .expect("destination");
        let mut delegation = Vec::new();
        delegation.extend_from_slice(&200_u32.to_be_bytes());
        delegation.extend_from_slice(&ROUTER_SIGNING_KEY_TYPE.code().to_be_bytes());
        delegation.extend_from_slice(transient.signing_public_key().as_bytes());
        let delegated_signature = root.sign(&delegation).expect("delegation signature");
        delegation.extend_from_slice(delegated_signature.as_bytes());
        let flags = (DATAGRAM2_FLAGS_VERSION | DATAGRAM2_FLAG_OFFLINE).to_be_bytes();
        let mut preimage = Vec::new();
        preimage.extend_from_slice(&target);
        preimage.extend_from_slice(&flags);
        preimage.extend_from_slice(&delegation);
        preimage.extend_from_slice(b"offline");
        let signature = transient.sign(&preimage).expect("transient signature");
        let mut wire = from;
        wire.extend_from_slice(&flags);
        wire.extend_from_slice(&delegation);
        wire.extend_from_slice(b"offline");
        wire.extend_from_slice(signature.as_bytes());
        let mut receiver = DatagramManager::new();
        receiver
            .process_inbound_at(DatagramInboundRequest {
                protocol: DATAGRAM2_PROTOCOL,
                source_port: 1,
                destination_port: 2,
                payload: &wire,
                transport_sender: [0; 32],
                recipient_hash: target,
                now_ms: 20_000,
                now_seconds: 100,
            })
            .expect("delegated signature verifies");
        assert_eq!(receiver.drain_received()[0].payload, b"offline");
        let expired = receiver.process_inbound_at(DatagramInboundRequest {
            protocol: DATAGRAM2_PROTOCOL,
            source_port: 1,
            destination_port: 2,
            payload: &wire,
            transport_sender: [0; 32],
            recipient_hash: target,
            now_ms: 20_001,
            now_seconds: 200,
        });
        assert_eq!(expired, Err(DatagramError::InvalidOfflineSignature));
    }

    #[test]
    fn datagram3_round_trip_preserves_options_and_marks_source_unauthenticated() {
        let sender = test_identity(34);
        let options =
            Mapping::from_entries(vec![("app".to_owned(), "dht".to_owned())]).expect("mapping");
        let (transport, _) =
            emitted_envelope(&sender, DATAGRAM3_PROTOCOL, b"d3", Some(options.clone()));
        let envelope =
            i2pr_proto::streaming::decode_client_payload(&transport.application_payload, 65_536)
                .expect("I2CP envelope");
        let mut receiver = DatagramManager::new();
        receiver
            .process_inbound_at(DatagramInboundRequest {
                protocol: envelope.protocol,
                source_port: envelope.source_port,
                destination_port: envelope.destination_port,
                payload: &envelope.payload,
                transport_sender: [0; 32],
                recipient_hash: [0; 32],
                now_ms: 30_000,
                now_seconds: 1,
            })
            .expect("D3 decodes");
        let events = receiver.drain_received();
        assert_eq!(events[0].payload, b"d3");
        assert!(!events[0].sender_authenticated);
        assert_eq!(events[0].options, Some(options));
        assert_eq!(receiver.counters().accepted_datagram3, 1);
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
                options: None,
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
                options: None,
            },
        );
        assert_eq!(
            outcome,
            Err(DatagramError::UnsupportedProtocol { protocol: 6 })
        );
        let mut receiver = DatagramManager::new();
        let outcome = receiver.process_inbound(6, 0, 0, &[0x00], [0xCC_u8; 32], 6_000);
        assert_eq!(
            outcome,
            Err(DatagramError::UnsupportedProtocol { protocol: 6 })
        );
        assert_eq!(receiver.counters().rejected, 1);
    }

    #[test]
    fn raw_application_protocols_round_trip_without_reserved_envelopes() {
        let sender = test_identity(18);
        let mut manager = DatagramManager::new();
        let sent = manager
            .send(
                &sender,
                &DatagramSendRequest {
                    destination_hash: [0xA5; 32],
                    source_port: 4,
                    destination_port: 8,
                    protocol: 42,
                    payload: b"custom raw protocol".to_vec(),
                    options: None,
                },
            )
            .expect("custom raw protocol sends without a SAM envelope");
        let envelope =
            i2pr_proto::streaming::decode_client_payload(&sent.application_payload, 65_536)
                .expect("I2CP envelope");
        assert_eq!(envelope.protocol, 42);
        assert_eq!(envelope.payload, b"custom raw protocol");

        let mut receiver = DatagramManager::new();
        receiver
            .process_inbound(42, 4, 8, b"custom raw protocol", [0xCC; 32], 5000)
            .expect("custom raw protocol accepted");
        let event = receiver
            .pop_received_for_listener(0, 0)
            .expect("wildcard listener receives the custom protocol");
        assert_eq!(event.protocol, 42);
        assert_eq!(event.payload, b"custom raw protocol");
        for reserved in [
            6,
            DATAGRAM1_PROTOCOL,
            DATAGRAM2_PROTOCOL,
            DATAGRAM3_PROTOCOL,
        ] {
            assert!(!is_raw_application_protocol(reserved));
        }
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
        assert_eq!(
            receiver.counters().accepted_raw,
            MAX_DATAGRAM_RECEIVE_QUEUE as u64
        );
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
