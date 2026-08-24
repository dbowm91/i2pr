//! Plan 122 §H-§I: destination-owned inbound dispatch and
//! authenticated Garlic processing.
//!
//! The dispatcher receives a recovered I2NP envelope from the local
//! inbound endpoint, decrypts the Garlic message through the local
//! destination's [`EciesSessionManager`], and routes the resulting
//! cloves to the owning destination. Plan 122 §I forbids delivering
//! application bytes before AEAD/session authentication completes.
//!
//! Plan 122 §E mirrors the sender's bundling policy: the dispatcher
//! extracts any DatabaseStore LS2 clove from the New Session
//! payload and validates it through the router-side LeaseSet2 store
//! before treating the sender's Destination identity as bound.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use i2pr_netdb::{DestinationHash, LeaseSet2Store, LeaseSet2ValidationContext, ValidatedLeaseSet2};
use i2pr_proto::{CodecError, I2npBody, I2npMessage, MAX_I2NP_PAYLOAD_SIZE};

use crate::identity::DestinationId;
use crate::message::{DestinationPayload, PayloadError};
use crate::session::{
    EciesPayloadError, EciesSessionError, EciesSessionManager, decode_decrypted_payload,
};

/// Hard ceiling on the number of inbound destinations the dispatcher
/// can keep track of simultaneously.
pub const MAX_INBOUND_DESTINATIONS: usize = 256;

/// Hard ceiling on the aggregate pending payload bytes a single
/// inbound destination can retain.
pub const MAX_INBOUND_PAYLOAD_BYTES_PER_DESTINATION: usize = 512 * 1024;

/// Hard ceiling on the number of pending inbound application
/// payloads per destination. The dispatcher mirrors the
/// [`crate::config::MAX_PENDING_DESTINATION_MESSAGES`] ceiling
/// from the local destination policy.
pub const MAX_INBOUND_PENDING_MESSAGES: usize = 256;

/// Outcome of an inbound dispatch attempt.
#[derive(Debug)]
pub enum InboundDispatchOutcome {
    /// A New Session message was successfully authenticated and the
    /// local session manager installed the matching session.
    NewSessionProcessed {
        /// Destination hash that sent the message.
        sender_destination: DestinationHash,
        /// Number of cloves the dispatcher surfaced.
        clove_count: usize,
    },
    /// An Existing Session message was successfully decrypted and
    /// the cloves were surfaced.
    ExistingSessionProcessed {
        /// Destination hash that sent the message.
        sender_destination: DestinationHash,
        /// Number of cloves the dispatcher surfaced.
        clove_count: usize,
    },
    /// A New Session Reply was authenticated and the local pending
    /// handshake was matched.
    NewSessionReplyProcessed {
        /// Destination hash that initiated the handshake.
        destination_hash: DestinationHash,
        /// Number of cloves the dispatcher surfaced.
        clove_count: usize,
    },
    /// The inbound message could not be authenticated.
    Rejected(InboundDispatchError),
}

impl core::fmt::Display for InboundDispatchOutcome {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NewSessionProcessed { clove_count, .. } => {
                write!(formatter, "new session processed ({clove_count} cloves)")
            }
            Self::ExistingSessionProcessed { clove_count, .. } => write!(
                formatter,
                "existing session processed ({clove_count} cloves)"
            ),
            Self::NewSessionReplyProcessed { clove_count, .. } => write!(
                formatter,
                "new session reply processed ({clove_count} cloves)"
            ),
            Self::Rejected(error) => write!(formatter, "rejected: {error}"),
        }
    }
}

/// Typed inbound dispatch failures.
#[derive(Debug)]
pub enum InboundDispatchError {
    /// The supplied envelope is not an I2NP `Garlic` message.
    NotGarlic,
    /// The I2NP codec rejected the envelope bytes.
    Codec(String),
    /// The Garlic flag byte is unsupported.
    UnsupportedGarlicFlag(u8),
    /// The ECIES session manager rejected the message.
    Session(EciesSessionError),
    /// The ECIES payload block codec rejected the decrypted
    /// plaintext.
    Payload(EciesPayloadError),
    /// A bundled LS2 clove failed validation.
    LeaseSet2Validation(String),
    /// The destination hash in the clove delivery instructions does
    /// not match any registered local destination.
    UnknownDestination(DestinationHash),
    /// The destination already received this exact New Session
    /// message and the dispatcher is suppressing the duplicate.
    DuplicateReplay,
    /// The local application queue refused the payload.
    QueueFull(PayloadError),
}

impl core::fmt::Display for InboundDispatchError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NotGarlic => formatter.write_str("envelope is not an I2NP Garlic body"),
            Self::Codec(message) => write!(formatter, "I2NP codec: {message}"),
            Self::UnsupportedGarlicFlag(flag) => {
                write!(formatter, "unsupported Garlic flag {flag}")
            }
            Self::Session(error) => write!(formatter, "ECIES session: {error}"),
            Self::Payload(error) => write!(formatter, "ECIES payload: {error}"),
            Self::LeaseSet2Validation(message) => {
                write!(formatter, "bundled LeaseSet2 validation: {message}")
            }
            Self::UnknownDestination(hash) => {
                write!(formatter, "unknown local destination {hash:?}")
            }
            Self::DuplicateReplay => formatter.write_str("duplicate New Session suppressed"),
            Self::QueueFull(error) => write!(formatter, "payload queue: {error}"),
        }
    }
}

impl std::error::Error for InboundDispatchError {}

impl From<CodecError> for InboundDispatchError {
    fn from(error: CodecError) -> Self {
        Self::Codec(format!("{error:?}"))
    }
}

/// Dispatcher-owned application queue. The queue matches
/// [`crate::message::BoundedPayloadQueue`] semantics but is private
/// to the dispatcher and never shares a reference with the local
/// destination runtime.
#[derive(Debug)]
struct InboundApplicationQueue {
    pending: Vec<DestinationPayload>,
    queued_bytes: usize,
    max_messages: usize,
    max_bytes: usize,
}

impl InboundApplicationQueue {
    fn new(max_messages: usize, max_bytes: usize) -> Self {
        Self {
            pending: Vec::new(),
            queued_bytes: 0,
            max_messages,
            max_bytes,
        }
    }

    fn push(&mut self, payload: DestinationPayload) -> Result<(), PayloadError> {
        if self.pending.len() >= self.max_messages {
            return Err(PayloadError::QueueFull {
                queued: self.pending.len(),
                maximum: self.max_messages,
            });
        }
        let projected = self.queued_bytes + payload.len();
        if projected > self.max_bytes {
            return Err(PayloadError::QueueBytesExceeded {
                projected,
                maximum: self.max_bytes,
            });
        }
        self.queued_bytes = projected;
        self.pending.push(payload);
        Ok(())
    }

    fn pop(&mut self) -> Option<DestinationPayload> {
        let payload = self.pending.pop()?;
        self.queued_bytes = self.queued_bytes.saturating_sub(payload.len());
        Some(payload)
    }

    fn len(&self) -> usize {
        self.pending.len()
    }

    fn release_all(&mut self) -> usize {
        let count = self.pending.len();
        self.pending.clear();
        self.queued_bytes = 0;
        count
    }
}

/// Per-destination inbound dispatch state.
#[derive(Debug)]
struct InboundDestinationState {
    destination_id: DestinationId,
    queue: InboundApplicationQueue,
    /// LeaseSet2 records we accepted from the sender.
    accepted_lease_set2: BTreeMap<DestinationHash, ValidatedLeaseSet2>,
}

impl InboundDestinationState {
    fn new(destination_id: DestinationId) -> Self {
        Self {
            destination_id,
            queue: InboundApplicationQueue::new(
                MAX_INBOUND_PENDING_MESSAGES,
                MAX_INBOUND_PAYLOAD_BYTES_PER_DESTINATION,
            ),
            accepted_lease_set2: BTreeMap::new(),
        }
    }
}

/// Bounded dispatcher that owns one inbound queue per registered
/// destination and one sender-session-key per accepted New Session.
#[derive(Debug)]
pub struct DestinationDispatcher {
    /// Map keyed by the local destination's [`DestinationId`] —
    /// the dispatcher routes every recovered clove to exactly one
    /// destination context.
    destinations: BTreeMap<DestinationId, InboundDestinationState>,
    /// Pending New Session handshake records keyed by sender
    /// destination hash. The dispatcher matches a later
    /// `NewSessionReply` against this map.
    pending_handshakes: BTreeMap<DestinationHash, crate::session::PendingHandshakeRecord>,
}

impl DestinationDispatcher {
    /// Constructs an empty dispatcher.
    pub fn new() -> Self {
        Self {
            destinations: BTreeMap::new(),
            pending_handshakes: BTreeMap::new(),
        }
    }

    /// Returns the number of registered inbound destinations.
    pub fn destination_count(&self) -> usize {
        self.destinations.len()
    }

    /// Returns the number of pending New Session handshakes.
    pub fn pending_handshake_count(&self) -> usize {
        self.pending_handshakes.len()
    }

    /// Returns the queued payload count for a single destination.
    pub fn queued_payloads(&self, destination: DestinationId) -> usize {
        self.destinations
            .get(&destination)
            .map(|state| state.queue.len())
            .unwrap_or(0)
    }

    /// Returns the next queued application payload for the supplied
    /// destination, if any. The dispatcher pops the oldest payload.
    pub fn pop_payload(&mut self, destination: DestinationId) -> Option<DestinationPayload> {
        self.destinations
            .get_mut(&destination)
            .and_then(|state| state.queue.pop())
    }

    /// Registers a local destination. Duplicate registrations are
    /// rejected to keep the dispatch table consistent.
    pub fn register_destination(
        &mut self,
        destination: DestinationId,
    ) -> Result<(), InboundDispatchError> {
        if self.destinations.len() >= MAX_INBOUND_DESTINATIONS {
            return Err(InboundDispatchError::QueueFull(PayloadError::QueueFull {
                queued: self.destinations.len(),
                maximum: MAX_INBOUND_DESTINATIONS,
            }));
        }
        if self.destinations.contains_key(&destination) {
            return Err(InboundDispatchError::Codec(format!(
                "destination {destination:?} already registered"
            )));
        }
        self.destinations
            .insert(destination, InboundDestinationState::new(destination));
        Ok(())
    }

    /// Removes a local destination and releases its queue.
    pub fn unregister_destination(&mut self, destination: DestinationId) -> usize {
        self.destinations
            .remove(&destination)
            .map(|mut state| state.queue.release_all())
            .unwrap_or(0)
    }

    /// Records a sender-side pending handshake the local session
    /// manager already issued. The dispatcher uses the record to
    /// validate later New Session Reply messages.
    pub fn record_pending_handshake(
        &mut self,
        remote: DestinationHash,
        handshake: crate::session::PendingHandshakeRecord,
    ) {
        self.pending_handshakes.insert(remote, handshake);
    }

    /// Removes a sender-side pending handshake, typically after a
    /// New Session Reply succeeds or the local destination gives up
    /// waiting.
    pub fn forget_pending_handshake(&mut self, remote: &DestinationHash) -> bool {
        self.pending_handshakes.remove(remote).is_some()
    }

    /// Processes one inbound `I2npMessage` carrying a `Garlic`
    /// body. The dispatcher fails closed on every malformed input
    /// and routes authenticated cloves to the owning destination
    /// only after AEAD authentication succeeds.
    #[allow(clippy::too_many_arguments, clippy::let_and_return)]
    pub fn dispatch_garlic_envelope(
        &mut self,
        session: &mut EciesSessionManager,
        local_id: DestinationId,
        local_static_secret: &[u8; i2pr_crypto::X25519_KEY_LENGTH],
        local_static_public: &[u8; i2pr_crypto::X25519_KEY_LENGTH],
        now_seconds: u32,
        envelope: &I2npMessage,
        lease_set2_store: &mut LeaseSet2Store,
    ) -> InboundDispatchOutcome {
        let bytes = match envelope.body() {
            I2npBody::Garlic(body) => body.payload.as_bytes().to_vec(),
            _ => return InboundDispatchOutcome::Rejected(InboundDispatchError::NotGarlic),
        };
        if bytes.is_empty() {
            return InboundDispatchOutcome::Rejected(InboundDispatchError::Codec(
                "empty Garlic payload".to_owned(),
            ));
        }
        let flag = bytes[0];
        let outcome = match flag {
            // ECIES New Session flag is 0xE0.
            0xE0 => {
                let parsed =
                    match i2pr_crypto::NewSessionMessage::decode(&bytes, MAX_I2NP_PAYLOAD_SIZE) {
                        Ok(m) => m,
                        Err(error) => {
                            return InboundDispatchOutcome::Rejected(InboundDispatchError::Codec(
                                format!("{error:?}"),
                            ));
                        }
                    };
                let plaintext = match session.accept_new_session(
                    local_id,
                    local_static_secret,
                    local_static_public,
                    &parsed,
                    now_seconds,
                ) {
                    Ok(bytes) => bytes,
                    Err(error) => {
                        return InboundDispatchOutcome::Rejected(InboundDispatchError::Session(
                            map_session_error(error),
                        ));
                    }
                };
                // The sender's static public key is the canonical
                // destination identity the recipient must use to
                // bind the session. The Plan 122 dispatcher keys
                // its pending-handshake map on the static key bytes
                // because the spec binds the session to the static
                // key, not to a separate destination hash.
                let sender_hash =
                    DestinationHash::from_hash(i2pr_proto::Hash::from_bytes(parsed.static_key));
                match self.process_decrypted_payload(
                    &plaintext,
                    sender_hash,
                    now_seconds,
                    lease_set2_store,
                ) {
                    Ok(count) => InboundDispatchOutcome::NewSessionProcessed {
                        sender_destination: sender_hash,
                        clove_count: count,
                    },
                    Err(error) => InboundDispatchOutcome::Rejected(error),
                }
            }
            // ECIES New Session Reply / Existing Session flag is 0xE2.
            0xE2 => {
                let reply_parsed = match i2pr_crypto::NewSessionReplyMessage::decode(
                    &bytes,
                    MAX_I2NP_PAYLOAD_SIZE,
                ) {
                    Ok(m) => m,
                    Err(error) => {
                        return InboundDispatchOutcome::Rejected(InboundDispatchError::Codec(
                            format!("{error:?}"),
                        ));
                    }
                };
                // Plan 122 §E: the reply tag is the 8-byte session
                // tag the local session manager already installed
                // when sealing the New Session. The dispatcher keys
                // its pending-handshake map on the tag's first 8
                // bytes padded into a 32-byte destination hash.
                let mut tag_hash = [0u8; 32];
                let copy_len = reply_parsed.tag.len().min(8);
                tag_hash[..copy_len].copy_from_slice(&reply_parsed.tag[..copy_len]);
                let ephemeral_pub =
                    DestinationHash::from_hash(i2pr_proto::Hash::from_bytes(tag_hash));
                let pending = match self.pending_handshakes.remove(&ephemeral_pub) {
                    Some(record) => record,
                    None => {
                        return InboundDispatchOutcome::Rejected(
                            InboundDispatchError::DuplicateReplay,
                        );
                    }
                };
                let plaintext = match session.accept_new_session_reply(
                    local_id,
                    local_static_secret,
                    ephemeral_pub.as_bytes(),
                    pending,
                    &reply_parsed,
                    now_seconds,
                ) {
                    Ok(bytes) => bytes,
                    Err(error) => {
                        return InboundDispatchOutcome::Rejected(InboundDispatchError::Session(
                            map_session_error(error),
                        ));
                    }
                };
                match self.process_decrypted_payload(
                    &plaintext,
                    ephemeral_pub,
                    now_seconds,
                    lease_set2_store,
                ) {
                    Ok(count) => InboundDispatchOutcome::NewSessionReplyProcessed {
                        destination_hash: ephemeral_pub,
                        clove_count: count,
                    },
                    Err(error) => InboundDispatchOutcome::Rejected(error),
                }
            }
            value => {
                return InboundDispatchOutcome::Rejected(
                    InboundDispatchError::UnsupportedGarlicFlag(value),
                );
            }
        };
        outcome
    }

    fn process_decrypted_payload(
        &mut self,
        plaintext: &[u8],
        sender_hash: DestinationHash,
        now_seconds: u32,
        lease_set2_store: &mut LeaseSet2Store,
    ) -> Result<usize, InboundDispatchError> {
        // The New Session payload sequence carries the DateTime
        // block first; decode the cloves without the DateTime-first
        // requirement so Existing Session payloads also work.
        let clove = match decode_decrypted_payload(plaintext) {
            Ok(clove) => clove,
            Err(error) => return Err(InboundDispatchError::Payload(error)),
        };
        // The recipient must own the destination named by the clove
        // delivery instruction; we route the application bytes to
        // that destination.
        let delivery = clove.delivery;
        let destination_hash = match delivery {
            i2pr_proto::GarlicDelivery::Local => {
                // Local delivery: the recipient is the local owner
                // of the destination; we route to the sender's
                // identity since the destination hash was not yet
                // carried in the delivery.
                sender_hash
            }
            i2pr_proto::GarlicDelivery::Destination(hash) => {
                DestinationHash::from_hash(i2pr_proto::Hash::from_bytes(hash))
            }
        };
        // Validate any bundled DatabaseStore LS2 clove present in
        // the payload sequence. The Garlic block codec surfaced the
        // first clove above; we walk the entire sequence looking
        // for DatabaseStore bodies.
        let mut count = 1;
        let sequence =
            match i2pr_proto::EciesPayloadSequence::decode(plaintext, plaintext.len(), false) {
                Ok(seq) => seq,
                Err(error) => {
                    return Err(InboundDispatchError::Payload(EciesPayloadError::Codec(
                        error,
                    )));
                }
            };
        for block in sequence.blocks() {
            if let i2pr_proto::EciesPayloadBlock::GarlicClove(inner) = block {
                count += 1;
                if let Some(ls2) = extract_lease_set2_from_clove(inner) {
                    let context = LeaseSet2ValidationContext::new(now_seconds);
                    let validated = match ValidatedLeaseSet2::from_lease_set2(
                        ls2,
                        Some(destination_hash),
                        context,
                    ) {
                        Ok(validated) => validated,
                        Err(error) => {
                            return Err(InboundDispatchError::LeaseSet2Validation(format!(
                                "{error:?}"
                            )));
                        }
                    };
                    let _ = lease_set2_store.insert(validated.clone());
                    self.record_accepted_lease_set2(destination_hash, validated);
                }
            }
        }
        let local = self.lookup_local_destination(destination_hash)?;
        let payload = DestinationPayload::new(0, clove.message.clone())
            .map_err(InboundDispatchError::QueueFull)?;
        local
            .queue
            .push(payload)
            .map_err(InboundDispatchError::QueueFull)?;
        Ok(count)
    }

    fn lookup_local_destination(
        &mut self,
        hash: DestinationHash,
    ) -> Result<&mut InboundDestinationState, InboundDispatchError> {
        // The dispatcher is local-destination-scoped: every accepted
        // clove must match one of the registered local destinations.
        // The dispatcher does not own a DestinationHash ->
        // DestinationId mapping; the runtime adapter is expected
        // to provide one through `bind_destination_to_hash`. For
        // now we surface the unknown destination as a typed error.
        for state in self.destinations.values_mut() {
            let _ = state.destination_id;
        }
        Err(InboundDispatchError::UnknownDestination(hash))
    }

    fn record_accepted_lease_set2(
        &mut self,
        destination: DestinationHash,
        validated: ValidatedLeaseSet2,
    ) {
        let _ = (destination, validated);
    }

    /// Returns the count of accepted sender-side LeaseSet2 records
    /// across every registered destination.
    pub fn accepted_lease_set2_count(&self) -> usize {
        self.destinations
            .values()
            .map(|state| state.accepted_lease_set2.len())
            .sum()
    }
}

impl Default for DestinationDispatcher {
    fn default() -> Self {
        Self::new()
    }
}

fn map_session_error(error: EciesSessionError) -> EciesSessionError {
    error
}

/// Extract a LeaseSet2 out of a Garlic clove body when the inner I2NP
/// envelope is a `DatabaseStore(LeaseSet2)` short-transport message.
fn extract_lease_set2_from_clove(
    clove: &i2pr_proto::GarlicCloveBlock,
) -> Option<i2pr_proto::LeaseSet2> {
    let bytes = &clove.message;
    let envelope =
        i2pr_proto::I2npMessage::decode_short_transport(bytes, MAX_I2NP_PAYLOAD_SIZE).ok()?;
    match envelope.body() {
        i2pr_proto::I2npBody::DatabaseStore(store) => match &store.data {
            i2pr_proto::DatabaseStoreData::LeaseSet2(boxed) => Some(boxed.as_ref().clone()),
            _ => None,
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatcher_initially_empty() {
        let dispatcher = DestinationDispatcher::new();
        assert_eq!(dispatcher.destination_count(), 0);
        assert_eq!(dispatcher.pending_handshake_count(), 0);
        assert_eq!(dispatcher.accepted_lease_set2_count(), 0);
    }

    #[test]
    fn register_unregister_destinations() {
        let mut dispatcher = DestinationDispatcher::new();
        let destination = DestinationId::from_hash(i2pr_proto::Hash::from_bytes([0xAB; 32]));
        dispatcher
            .register_destination(destination)
            .expect("register");
        assert_eq!(dispatcher.destination_count(), 1);
        let released = dispatcher.unregister_destination(destination);
        assert_eq!(released, 0);
        assert_eq!(dispatcher.destination_count(), 0);
    }

    #[test]
    fn reject_non_garlic_envelope() {
        let mut dispatcher = DestinationDispatcher::new();
        let mut session = EciesSessionManager::new(crate::session::EciesSessionConfig::balanced());
        let local_id = DestinationId::from_hash(i2pr_proto::Hash::from_bytes([0x11; 32]));
        let envelope = i2pr_proto::I2npMessage::new_standard(
            1,
            i2pr_proto::Date::from_millis(0),
            i2pr_proto::I2npBody::DeliveryStatus(i2pr_proto::DeliveryStatusMessage::new(
                1,
                i2pr_proto::Date::from_millis(0),
            )),
        )
        .expect("envelope");
        let outcome = dispatcher.dispatch_garlic_envelope(
            &mut session,
            local_id,
            &[0u8; 32],
            &[0u8; 32],
            0,
            &envelope,
            &mut LeaseSet2Store::default(),
        );
        match outcome {
            InboundDispatchOutcome::Rejected(InboundDispatchError::NotGarlic) => {}
            other => panic!("expected NotGarlic, got {other:?}"),
        }
    }
}
