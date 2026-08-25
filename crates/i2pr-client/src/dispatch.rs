//! Plan 122 §H-§I / Plan 127 §2-§8: destination-owned inbound
//! dispatch and authenticated Garlic processing.
//!
//! The dispatcher receives a recovered I2NP envelope from the local
//! inbound endpoint, classifies the raw ECIES encrypted data through
//! the local destination's [`EciesSessionManager`], and routes the
//! resulting cloves to the owning destination only after session
//! authentication succeeds. There is no legacy message-type byte:
//! classification is structure- and tag-driven, never inferred from
//! a magic first byte (Plan 127 §8).
//!
//! Plan 127 owns the bound New Session processing order:
//!
//! ```text
//! authenticate/decrypt NS
//!  -> obtain authenticated A static X25519 key
//!  -> decode all payload blocks
//!  -> find bundled DatabaseStore(Standard LeaseSet2)
//!  -> validate LS2 signature/time/structure/leases under its own
//!     contained Destination hash
//!  -> verify LS2 usable type-4 X25519 key == authenticated static key
//!  -> only then bind provisional state to A DestinationHash
//!  -> make validated A LS2 available to reverse routing
//! ```
//!
//! The remote identity is derived exclusively from the validated
//! bundled LeaseSet2's contained Destination. It is never taken from
//! NS static-key bytes, an NSR tag, or an ES tag. When the binding
//! fails the retained New Session Reply context is dropped so no
//! reply can be emitted for an unbindable session.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use i2pr_netdb::{DestinationHash, LeaseSet2Store, LeaseSet2ValidationContext, ValidatedLeaseSet2};
use i2pr_proto::{
    CodecError, EciesPayloadBlock, EciesPayloadSequence, GarlicCloveBlock, GarlicDelivery, Hash,
    I2npBody, I2npMessage, MAX_I2NP_PAYLOAD_SIZE,
};

use crate::identity::DestinationId;
use crate::message::{DestinationPayload, PayloadError};
use crate::session::{
    ClassifiedInbound, ClassifiedUnknown, EciesPayloadError, EciesSessionError, EciesSessionManager,
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
    /// A bound New Session was authenticated and the sender's
    /// bundled LeaseSet2 was validated under its own contained
    /// Destination hash with a matching type-4 static key (Plan
    /// 127 §2). The session is bound to the remote DestinationHash
    /// and the validated record is handed back for reverse-routing
    /// installation without any raw reparse.
    NewSessionProcessed {
        /// The owning local destination context that decrypted the
        /// message (selected before ECIES processing through the
        /// inbound tunnel ownership).
        local_destination: DestinationId,
        /// Remote peer identity after binding: the NetDB key derived
        /// from the Destination contained in the sender's validated
        /// LeaseSet2. Never derived from NS static-key bytes.
        remote_destination_hash: DestinationHash,
        /// The validated sender Standard LeaseSet2 ready to install
        /// into the local [`crate::routing::DestinationRouting`] for
        /// reverse routing (Plan 127 §3/§4).
        validated_remote_lease_set2: Box<ValidatedLeaseSet2>,
        /// Number of cloves the dispatcher surfaced.
        clove_count: usize,
    },
    /// An Existing Session message was successfully decrypted and
    /// the cloves were surfaced.
    ExistingSessionProcessed {
        /// The sender's X25519 static public key (the paired session
        /// identity).
        remote_static_public: [u8; i2pr_crypto::X25519_KEY_LENGTH],
        /// The sender's DestinationHash when the dispatcher can bind
        /// it against previously validated knowledge; `None` when no
        /// validated record carries that static key yet.
        sender_destination: Option<DestinationHash>,
        /// Number of cloves the dispatcher surfaced.
        clove_count: usize,
    },
    /// A New Session Reply was authenticated and the local pending
    /// handshake was matched through its retained reply tag/context.
    NewSessionReplyProcessed {
        /// The remote X25519 static public key the pending handshake
        /// was bound to at initiation time.
        remote_static_public: [u8; i2pr_crypto::X25519_KEY_LENGTH],
        /// The sender's DestinationHash when the dispatcher can bind
        /// it against previously validated knowledge; `None` when no
        /// validated record carries that static key yet.
        sender_destination: Option<DestinationHash>,
        /// Number of cloves the dispatcher surfaced.
        clove_count: usize,
    },
    /// The inbound message could not be authenticated or processed.
    Rejected(InboundDispatchError),
}

impl core::fmt::Display for InboundDispatchOutcome {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NewSessionProcessed {
                remote_destination_hash,
                clove_count,
                ..
            } => write!(
                formatter,
                "new session processed ({clove_count} cloves, remote {remote_destination_hash:?})"
            ),
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
    /// The encrypted envelope is shorter than the smallest valid
    /// ECIES message.
    EnvelopeTooShort {
        /// Observed byte count.
        actual: usize,
        /// Smallest acceptable byte count.
        minimum: usize,
    },
    /// The Garlic flag byte is unsupported.
    UnsupportedGarlicFlag(u8),
    /// The ECIES session manager rejected the message.
    Session(EciesSessionError),
    /// The ECIES payload block codec rejected the decrypted
    /// plaintext.
    Payload(EciesPayloadError),
    /// A bound New Session carried no bundled sender LeaseSet2 or
    /// more than one ambiguous candidate; the repliable M6 path
    /// requires exactly one (Plan 127 §2).
    MissingSenderLeaseSet2,
    /// A bundled sender LeaseSet2 failed validation under its own
    /// contained Destination hash.
    LeaseSet2Validation(String),
    /// The bundled sender LeaseSet2 is valid but its usable type-4
    /// X25519 key does not match the authenticated NS static key;
    /// the binding is rejected and no reply may be sent (Plan 127
    /// §2).
    SenderKeyMismatch,
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
            Self::EnvelopeTooShort { actual, minimum } => write!(
                formatter,
                "encrypted envelope too short: {actual} bytes, minimum {minimum}"
            ),
            Self::UnsupportedGarlicFlag(flag) => {
                write!(formatter, "unsupported Garlic flag {flag}")
            }
            Self::Session(error) => write!(formatter, "ECIES session: {error}"),
            Self::Payload(error) => write!(formatter, "ECIES payload: {error}"),
            Self::MissingSenderLeaseSet2 => {
                formatter.write_str("bound New Session carried no unambiguous bundled LeaseSet2")
            }
            Self::LeaseSet2Validation(message) => {
                write!(formatter, "bundled LeaseSet2 validation: {message}")
            }
            Self::SenderKeyMismatch => write!(
                formatter,
                "bundled LeaseSet2 type-4 key does not match the authenticated NS static key"
            ),
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
    /// Owning local destination id; retained for diagnostics and
    /// for atomically removing the matching hash binding when the
    /// destination is unregistered.
    #[allow(dead_code)]
    destination_id: DestinationId,
    queue: InboundApplicationQueue,
    /// Validated sender-side LeaseSet2 records this destination
    /// accepted from bound New Sessions, keyed by the remote
    /// DestinationHash derived from each record's own contained
    /// Destination (Plan 127 §3).
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
    /// Reverse map from the local destination's
    /// [`i2pr_netdb::DestinationHash`] to the local
    /// [`DestinationId`]. The dispatcher uses the binding to look up
    /// the owning destination for every accepted Garlic clove.
    destination_hashes: BTreeMap<DestinationHash, DestinationId>,
}

/// Decrypted clove set extracted from one authenticated payload.
struct DecryptedCloves {
    application_clove: GarlicCloveBlock,
    clove_count: usize,
    /// Bundled DatabaseStore LeaseSet2 candidates found in the
    /// payload sequence.
    sender_lease_set2s: Vec<i2pr_proto::LeaseSet2>,
}

impl DestinationDispatcher {
    /// Constructs an empty dispatcher.
    pub fn new() -> Self {
        Self {
            destinations: BTreeMap::new(),
            destination_hashes: BTreeMap::new(),
        }
    }

    /// Returns the number of registered inbound destinations.
    pub fn destination_count(&self) -> usize {
        self.destinations.len()
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

    /// Binds the supplied destination hash to the supplied
    /// registered local destination. The dispatcher uses the binding
    /// to look up the owning destination for every accepted Garlic
    /// clove. Replacing an existing binding fails closed to keep the
    /// ownership table consistent.
    pub fn bind_destination_hash(
        &mut self,
        destination: DestinationId,
        hash: DestinationHash,
    ) -> Result<(), InboundDispatchError> {
        if !self.destinations.contains_key(&destination) {
            return Err(InboundDispatchError::UnknownDestination(hash));
        }
        self.destination_hashes.insert(hash, destination);
        Ok(())
    }

    /// Removes a local destination and releases its queue. The
    /// matching hash bindings are removed atomically.
    pub fn unregister_destination(&mut self, destination: DestinationId) -> usize {
        self.destination_hashes
            .retain(|_, bound| *bound != destination);
        self.destinations
            .remove(&destination)
            .map(|mut state| state.queue.release_all())
            .unwrap_or(0)
    }

    /// Processes one inbound `I2npMessage` carrying a `Garlic`
    /// body. The dispatcher fails closed on every malformed input,
    /// passes the raw ECIES encrypted data to the session manager's
    /// structure-driven classifier, and processes payload blocks
    /// only after session authentication succeeds (Plan 127 §8).
    #[allow(clippy::too_many_arguments)]
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
        let classified = session.classify(&bytes);
        match classified {
            ClassifiedInbound::NewSessionReply => {
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
                // The reply tag selects the pending handshake; no
                // caller-supplied remote identity is consulted.
                let accepted = match session.accept_new_session_reply(
                    local_id,
                    local_static_secret,
                    &reply_parsed,
                    now_seconds,
                ) {
                    Ok(accepted) => accepted,
                    Err(error) => {
                        return InboundDispatchOutcome::Rejected(InboundDispatchError::Session(
                            error,
                        ));
                    }
                };
                let sender_destination = self
                    .resolve_remote_destination(&accepted.remote_static_public, lease_set2_store);
                match self.process_existing_payload(local_id, &accepted.payload) {
                    Ok(clove_count) => InboundDispatchOutcome::NewSessionReplyProcessed {
                        remote_static_public: accepted.remote_static_public,
                        sender_destination,
                        clove_count,
                    },
                    Err(error) => InboundDispatchOutcome::Rejected(error),
                }
            }
            ClassifiedInbound::ExistingSession => {
                let parsed = match i2pr_crypto::ExistingSessionMessage::decode(
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
                let accepted = match session.accept_existing_session(&parsed) {
                    Ok(accepted) => accepted,
                    Err(error) => {
                        return InboundDispatchOutcome::Rejected(InboundDispatchError::Session(
                            error,
                        ));
                    }
                };
                let sender_destination = self
                    .resolve_remote_destination(&accepted.remote_static_public, lease_set2_store);
                match self.process_existing_payload(local_id, &accepted.payload) {
                    Ok(clove_count) => InboundDispatchOutcome::ExistingSessionProcessed {
                        remote_static_public: accepted.remote_static_public,
                        sender_destination,
                        clove_count,
                    },
                    Err(error) => InboundDispatchOutcome::Rejected(error),
                }
            }
            ClassifiedInbound::CandidateNewSession | ClassifiedInbound::Unknown(_) => {
                let parsed = match i2pr_crypto::BoundNewSessionMessage::decode(
                    &bytes,
                    MAX_I2NP_PAYLOAD_SIZE,
                ) {
                    Ok(m) => m,
                    Err(_) => {
                        return InboundDispatchOutcome::Rejected(match classified {
                            ClassifiedInbound::Unknown(ClassifiedUnknown::TooShort {
                                actual,
                                minimum,
                            }) => InboundDispatchError::EnvelopeTooShort { actual, minimum },
                            _ => InboundDispatchError::Codec(
                                "unclassified garlic envelope rejected".to_owned(),
                            ),
                        });
                    }
                };
                let accepted = match session.accept_new_session(
                    local_id,
                    local_static_secret,
                    local_static_public,
                    &parsed,
                    now_seconds,
                ) {
                    Ok(accepted) => accepted,
                    Err(error) => {
                        return InboundDispatchOutcome::Rejected(InboundDispatchError::Session(
                            error,
                        ));
                    }
                };
                match self.process_bound_new_session_payload(
                    local_id,
                    &accepted,
                    now_seconds,
                    lease_set2_store,
                ) {
                    Ok(outcome) => outcome,
                    Err(error) => {
                        // Plan 127 §2: a failed binding must never leave a
                        // sealable reply context behind — no NSR may be
                        // sent for an unbindable session.
                        session.drop_provisional_responder(&accepted.alice_static_public);
                        InboundDispatchOutcome::Rejected(error)
                    }
                }
            }
        }
    }

    /// Plan 127 §2 processing order for an authenticated inbound
    /// bound New Session: decode every payload block, require the
    /// bundled sender LeaseSet2, validate it under **its own**
    /// contained Destination hash, verify its usable type-4 X25519
    /// key equals the authenticated NS static key, and only then
    /// bind the session to the derived remote DestinationHash and
    /// hand the validated record back for reverse routing.
    fn process_bound_new_session_payload(
        &mut self,
        local_id: DestinationId,
        accepted: &crate::session::AcceptedNewSession,
        now_seconds: u32,
        lease_set2_store: &mut LeaseSet2Store,
    ) -> Result<InboundDispatchOutcome, InboundDispatchError> {
        let cloves = decode_cloves(&accepted.payload)?;
        // Exactly one bundled sender LeaseSet2 is required on the
        // repliable M6 path.
        let [sender_ls2] = cloves.sender_lease_set2s.as_slice() else {
            return Err(InboundDispatchError::MissingSenderLeaseSet2);
        };
        // Validate under the record's OWN contained Destination hash:
        // `expected_key = None` derives the NetDB key from the
        // embedded Destination instead of assuming the local
        // recipient's hash (Plan 127 §3).
        let context = LeaseSet2ValidationContext::new(now_seconds);
        let validated = ValidatedLeaseSet2::from_lease_set2(sender_ls2.clone(), None, context)
            .map_err(|error| InboundDispatchError::LeaseSet2Validation(format!("{error:?}")))?;
        let remote_destination_hash = validated.key();
        // The LS2 usable type-4 X25519 key must equal the
        // authenticated NS static key before the binding completes.
        let ls2_key = validated
            .lease_set2()
            .usable_x25519_key()
            .map_err(|_| InboundDispatchError::SenderKeyMismatch)?;
        if ls2_key.as_bytes() != accepted.alice_static_public {
            return Err(InboundDispatchError::SenderKeyMismatch);
        }
        // Binding succeeds: record the validated sender LS2 under the
        // derived remote DestinationHash (Plan 127 §3) and make the
        // record available to router-side routing through the typed
        // handoff store (Plan 127 §4). No raw reparse happens later.
        self.record_accepted_lease_set2(local_id, remote_destination_hash, validated.clone());
        let _ = lease_set2_store.insert(validated.clone());
        // Local target ownership is selected before/independently of
        // the remote identity: the delivery instruction names the
        // local owner, and the sender identity stays separate
        // (Plan 127 §6).
        let clove_count = cloves.clove_count;
        self.route_application_clove(local_id, &cloves.application_clove)?;
        Ok(InboundDispatchOutcome::NewSessionProcessed {
            local_destination: local_id,
            remote_destination_hash,
            validated_remote_lease_set2: Box::new(validated),
            clove_count,
        })
    }

    /// Processing for already-bound traffic (NSR and ES): the
    /// session identity was established during the handshake, so the
    /// payload needs no re-binding; cloves are routed to the owning
    /// local destination after authentication.
    fn process_existing_payload(
        &mut self,
        local_id: DestinationId,
        plaintext: &[u8],
    ) -> Result<usize, InboundDispatchError> {
        let cloves = decode_cloves(plaintext)?;
        let clove_count = cloves.clove_count;
        self.route_application_clove(local_id, &cloves.application_clove)?;
        Ok(clove_count)
    }

    /// Routes the application clove to the local owner named by the
    /// delivery instruction. `Local` delivery resolves to the
    /// tunnel-owned local destination context; a `Destination`
    /// instruction must name that same local destination. The sender
    /// identity is never consulted for local routing (Plan 127 §6),
    /// and no trial decryption across local keys ever occurs because
    /// exactly one destination-scoped session manager decrypts.
    fn route_application_clove(
        &mut self,
        local_id: DestinationId,
        clove: &GarlicCloveBlock,
    ) -> Result<(), InboundDispatchError> {
        let target_hash = match clove.delivery {
            GarlicDelivery::Local => *local_id.as_hash(),
            GarlicDelivery::Destination(bytes) => Hash::from_bytes(bytes),
        };
        if target_hash != *local_id.as_hash() {
            return Err(InboundDispatchError::UnknownDestination(
                DestinationHash::from_hash(target_hash),
            ));
        }
        let local = self.destinations.get_mut(&local_id).ok_or_else(|| {
            InboundDispatchError::UnknownDestination(DestinationHash::from_hash(target_hash))
        })?;
        let payload = DestinationPayload::new(0, clove.message.clone())
            .map_err(InboundDispatchError::QueueFull)?;
        local
            .queue
            .push(payload)
            .map_err(InboundDispatchError::QueueFull)
    }

    /// Records a validated sender LeaseSet2 under the remote
    /// DestinationHash derived from the record's own contained
    /// Destination (Plan 127 §3 — replaces the former no-op).
    fn record_accepted_lease_set2(
        &mut self,
        local_id: DestinationId,
        remote: DestinationHash,
        validated: ValidatedLeaseSet2,
    ) {
        if let Some(state) = self.destinations.get_mut(&local_id) {
            state.accepted_lease_set2.insert(remote, validated);
        }
    }

    /// Resolves a paired-session static key to a remote
    /// DestinationHash using previously validated knowledge only:
    /// first the accepted sender LeaseSet2 records, then the
    /// router-side LeaseSet2 cache. The static key bytes themselves
    /// are never hashed into a destination identity (Plan 127 §2).
    fn resolve_remote_destination(
        &self,
        remote_static_public: &[u8; i2pr_crypto::X25519_KEY_LENGTH],
        lease_set2_store: &LeaseSet2Store,
    ) -> Option<DestinationHash> {
        for state in self.destinations.values() {
            for (hash, validated) in &state.accepted_lease_set2 {
                if let Ok(key) = validated.lease_set2().usable_x25519_key()
                    && key.as_bytes() == remote_static_public
                {
                    return Some(*hash);
                }
            }
        }
        lease_set2_store.iter().find_map(|(hash, validated)| {
            let key = validated.lease_set2().usable_x25519_key().ok()?;
            (key.as_bytes() == remote_static_public).then_some(*hash)
        })
    }

    /// Returns the count of accepted sender-side LeaseSet2 records
    /// across every registered destination.
    pub fn accepted_lease_set2_count(&self) -> usize {
        self.destinations
            .values()
            .map(|state| state.accepted_lease_set2.len())
            .sum()
    }

    /// Returns the validated sender LeaseSet2 recorded for the
    /// supplied remote DestinationHash, if any (Plan 127 §3 read
    /// side for composition callers).
    pub fn accepted_lease_set2_for(
        &self,
        local_id: DestinationId,
        remote: DestinationHash,
    ) -> Option<&ValidatedLeaseSet2> {
        self.destinations
            .get(&local_id)?
            .accepted_lease_set2
            .get(&remote)
    }
}

impl Default for DestinationDispatcher {
    fn default() -> Self {
        Self::new()
    }
}

/// Decodes the full ECIES payload sequence into its clove set: the
/// first Garlic Clove is the application carrier, later cloves are
/// counted and inspected for bundled DatabaseStore LeaseSet2
/// records.
fn decode_cloves(plaintext: &[u8]) -> Result<DecryptedCloves, InboundDispatchError> {
    // The sequence decoder runs without the leading-DateTime
    // requirement so New Session, New Session Reply, and Existing
    // Session payloads share one path.
    let sequence = EciesPayloadSequence::decode(plaintext, plaintext.len(), false)
        .map_err(EciesPayloadError::Codec)
        .map_err(InboundDispatchError::Payload)?;
    let mut application_clove: Option<GarlicCloveBlock> = None;
    let mut clove_count = 0_usize;
    let mut sender_lease_set2s = Vec::new();
    for block in sequence.blocks() {
        if let EciesPayloadBlock::GarlicClove(clove) = block {
            clove_count += 1;
            if application_clove.is_none() {
                application_clove = Some(clove.clone());
            }
            if let Some(ls2) = extract_lease_set2_from_clove(clove) {
                sender_lease_set2s.push(ls2);
            }
        }
    }
    let application_clove =
        application_clove.ok_or(InboundDispatchError::Payload(EciesPayloadError::NoClove))?;
    Ok(DecryptedCloves {
        application_clove,
        clove_count,
        sender_lease_set2s,
    })
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
