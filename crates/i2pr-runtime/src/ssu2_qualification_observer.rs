//! Private, bounded source receipts for the Plan 480 SSU2 qualification seam.
//!
//! This observer is an in-process diagnostic only. It owns no socket and has
//! no serialization implementation; callers must explicitly project receipts
//! into a separately authorized, sanitized local artifact.

use std::collections::{HashSet, VecDeque};
use std::sync::{Arc, Mutex};

use i2pr_proto::Hash;
use i2pr_transport::{LinkId, PeerId};
use rand_core::{OsRng, TryRngCore};

const MAX_EVENTS: usize = 512;

/// Source-authenticated SSU2 event kinds. These describe observed owner
/// transitions; they do not imply a remote product's acceptance.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Ssu2QualificationEventKind {
    /// An authenticated link was promoted into the active owner.
    OutboundSessionAuthenticated,
    /// A remote peer completed the responder-side authenticated promotion.
    InboundSessionAuthenticated,
    /// An authenticated link was removed from the active owner.
    SessionClosed,
    /// An authenticated I2NP message reached the runtime-to-daemon handoff.
    AuthenticatedI2npReceived,
    /// A correlated I2NP message entered the bounded session queue.
    I2npQueueAccepted,
    /// A daemon owner reports an effect for an exact authenticated ingress.
    I2npProductEffectAccepted,
    /// A correlated I2NP message was accepted by the UDP socket writer.
    I2npDatagramWritten,
    /// A correlated I2NP message failed before a successful UDP socket write.
    I2npDatagramWriteFailed,
    /// A correlated datagram was refused by the bounded staging queue.
    I2npDatagramStageRejected,
    /// This runtime accepted its signed local RouterInfo and identity binding.
    LocalIdentityAttested,
    /// The explicitly qualified runtime completed its requested socket bind.
    SocketsBound,
    /// The local owner requested shutdown.
    RouterShutdown,
}

/// One sanitized in-memory event. The peer commitment is salted per observer
/// lifetime; the link generation is process-local and never persists by this
/// type. No endpoint or payload is retained.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Ssu2QualificationEvent {
    sequence: u64,
    kind: Ssu2QualificationEventKind,
    peer_commitment: [u8; 16],
    link_generation: u64,
    message_id: Option<u32>,
}

impl Ssu2QualificationEvent {
    /// Monotonic event sequence within this observer lifetime.
    pub const fn sequence(self) -> u64 {
        self.sequence
    }

    /// Authenticated source-owner transition.
    pub const fn kind(self) -> Ssu2QualificationEventKind {
        self.kind
    }

    /// Per-run peer commitment suitable for equality checks, not identity
    /// disclosure. It changes when the observer lifetime changes.
    pub const fn peer_commitment(self) -> [u8; 16] {
        self.peer_commitment
    }

    /// Exact runtime link generation, local to this process.
    pub const fn link_generation(self) -> u64 {
        self.link_generation
    }

    /// Ephemeral I2NP correlation ID. Consumers must not persist this value.
    pub const fn message_id(self) -> Option<u32> {
        self.message_id
    }
}

#[derive(Debug)]
struct ObserverState {
    epoch_salt: [u8; 32],
    next_sequence: u64,
    events: VecDeque<Ssu2QualificationEvent>,
    pending_ingress: HashSet<([u8; 16], u64, u32)>,
    overflowed: bool,
}

/// Cloneable read handle for one source-owned observer epoch.
#[derive(Clone, Debug)]
pub struct Ssu2QualificationObserver {
    state: Arc<Mutex<ObserverState>>,
}

impl Ssu2QualificationObserver {
    pub(crate) fn new() -> Result<Self, ()> {
        let mut epoch_salt = [0_u8; 32];
        OsRng.try_fill_bytes(&mut epoch_salt).map_err(|_| ())?;
        Ok(Self {
            state: Arc::new(Mutex::new(ObserverState {
                epoch_salt,
                next_sequence: 1,
                events: VecDeque::with_capacity(MAX_EVENTS),
                pending_ingress: HashSet::with_capacity(MAX_EVENTS),
                overflowed: false,
            })),
        })
    }

    /// Computes the observer-epoch commitment for an expected peer identity.
    /// The raw RouterHash is used only for this transient comparison.
    pub fn peer_commitment(&self, peer: PeerId) -> Option<[u8; 16]> {
        let state = self.state.lock().ok()?;
        Some(commitment(&state.epoch_salt, peer.hash()))
    }

    pub(crate) fn record(
        &self,
        peer: PeerId,
        link_id: LinkId,
        kind: Ssu2QualificationEventKind,
        message_ids: &[u32],
    ) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        if kind == Ssu2QualificationEventKind::SessionClosed {
            state
                .pending_ingress
                .retain(|(_, generation, _)| *generation != link_id.value());
        }
        let count = message_ids.len().max(1);
        for index in 0..count {
            if state.events.len() == MAX_EVENTS {
                state.overflowed = true;
                return;
            }
            let Some(next) = state.next_sequence.checked_add(1) else {
                state.overflowed = true;
                return;
            };
            let message_id = message_ids.get(index).copied();
            let event = Ssu2QualificationEvent {
                sequence: state.next_sequence,
                kind,
                peer_commitment: commitment(&state.epoch_salt, peer.hash()),
                link_generation: link_id.value(),
                message_id,
            };
            state.events.push_back(event);
            state.next_sequence = next;
        }
    }

    pub(crate) fn record_process(&self, peer: PeerId, kind: Ssu2QualificationEventKind) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        if state.events.len() == MAX_EVENTS {
            state.overflowed = true;
            return;
        }
        let Some(next) = state.next_sequence.checked_add(1) else {
            state.overflowed = true;
            return;
        };
        let sequence = state.next_sequence;
        let peer_commitment = commitment(&state.epoch_salt, peer.hash());
        state.events.push_back(Ssu2QualificationEvent {
            sequence,
            kind,
            peer_commitment,
            link_generation: 0,
            message_id: None,
        });
        state.next_sequence = next;
    }

    pub(crate) fn record_authenticated_ingress(
        &self,
        peer: PeerId,
        link_id: LinkId,
        message_id: u32,
    ) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        if state.events.len() == MAX_EVENTS || state.pending_ingress.len() >= MAX_EVENTS {
            state.overflowed = true;
            return;
        }
        let Some(next) = state.next_sequence.checked_add(1) else {
            state.overflowed = true;
            return;
        };
        let peer_commitment = commitment(&state.epoch_salt, peer.hash());
        let sequence = state.next_sequence;
        state
            .pending_ingress
            .insert((peer_commitment, link_id.value(), message_id));
        state.events.push_back(Ssu2QualificationEvent {
            sequence,
            kind: Ssu2QualificationEventKind::AuthenticatedI2npReceived,
            peer_commitment,
            link_generation: link_id.value(),
            message_id: Some(message_id),
        });
        state.next_sequence = next;
    }

    /// Records an actual daemon-side owner effect only when it matches a
    /// preceding authenticated ingress in this observer epoch. Duplicate,
    /// fabricated, stale, or cross-peer claims are rejected.
    pub fn record_product_effect(&self, peer: PeerId, link_id: LinkId, message_id: u32) -> bool {
        let Ok(mut state) = self.state.lock() else {
            return false;
        };
        let peer_commitment = commitment(&state.epoch_salt, peer.hash());
        let key = (peer_commitment, link_id.value(), message_id);
        if !state.pending_ingress.contains(&key) {
            return false;
        }
        if state.events.len() == MAX_EVENTS {
            state.overflowed = true;
            return false;
        }
        let Some(next) = state.next_sequence.checked_add(1) else {
            state.overflowed = true;
            return false;
        };
        state.pending_ingress.remove(&key);
        let sequence = state.next_sequence;
        state.events.push_back(Ssu2QualificationEvent {
            sequence,
            kind: Ssu2QualificationEventKind::I2npProductEffectAccepted,
            peer_commitment,
            link_generation: link_id.value(),
            message_id: Some(message_id),
        });
        state.next_sequence = next;
        true
    }

    /// Drains the bounded event queue. Overflow is sticky and must be treated
    /// as a failed qualification receipt.
    pub fn drain(&self) -> (Vec<Ssu2QualificationEvent>, bool) {
        let Ok(mut state) = self.state.lock() else {
            return (Vec::new(), true);
        };
        (state.events.drain(..).collect(), state.overflowed)
    }
}

fn commitment(salt: &[u8; 32], peer: Hash) -> [u8; 16] {
    let mut input = Vec::with_capacity(32 + 32 + 24);
    input.extend_from_slice(b"i2pr-plan480-peer-v1\0");
    input.extend_from_slice(salt);
    input.extend_from_slice(peer.as_bytes());
    let digest = i2pr_crypto::sha256(&input);
    let mut output = [0_u8; 16];
    output.copy_from_slice(&digest.as_bytes()[..16]);
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peer_commitment_is_epoch_local_and_events_are_bounded() {
        let observer = Ssu2QualificationObserver::new().expect("observer");
        let same_epoch = observer.clone();
        let peer = PeerId::from_bytes([7; 32]);
        assert_eq!(
            observer.peer_commitment(peer),
            same_epoch.peer_commitment(peer)
        );
        assert_ne!(
            observer.peer_commitment(peer),
            Ssu2QualificationObserver::new()
                .expect("second epoch")
                .peer_commitment(peer)
        );
        let link = LinkId::new(11).expect("link");
        for _ in 0..=MAX_EVENTS {
            observer.record(
                peer,
                link,
                Ssu2QualificationEventKind::OutboundSessionAuthenticated,
                &[],
            );
        }
        let (events, overflowed) = observer.drain();
        assert_eq!(events.len(), MAX_EVENTS);
        assert!(overflowed);
        assert!(
            events
                .windows(2)
                .all(|pair| pair[0].sequence < pair[1].sequence)
        );
    }

    #[test]
    fn message_ids_are_ephemeral_event_fields_and_multiple_ids_are_sequenced() {
        let observer = Ssu2QualificationObserver::new().expect("observer");
        observer.record(
            PeerId::from_bytes([3; 32]),
            LinkId::new(9).expect("link"),
            Ssu2QualificationEventKind::I2npDatagramWritten,
            &[13, 14],
        );
        let (events, overflowed) = observer.drain();
        assert!(!overflowed);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].message_id(), Some(13));
        assert_eq!(events[1].message_id(), Some(14));
    }

    #[test]
    fn product_receipt_requires_exact_authenticated_peer_link_and_message() {
        let observer = Ssu2QualificationObserver::new().expect("observer");
        let peer = PeerId::from_bytes([3; 32]);
        let other_peer = PeerId::from_bytes([4; 32]);
        let link = LinkId::new(19).expect("link");
        let other_link = LinkId::new(20).expect("other link");
        assert!(!observer.record_product_effect(peer, link, 81));
        observer.record_authenticated_ingress(peer, link, 81);
        assert!(!observer.record_product_effect(other_peer, link, 81));
        assert!(!observer.record_product_effect(peer, other_link, 81));
        assert!(!observer.record_product_effect(peer, link, 82));
        assert!(observer.record_product_effect(peer, link, 81));
        assert!(!observer.record_product_effect(peer, link, 81));
        let (events, overflowed) = observer.drain();
        assert!(!overflowed);
        assert_eq!(events.len(), 2);
        assert_eq!(
            events[1].kind(),
            Ssu2QualificationEventKind::I2npProductEffectAccepted
        );
        assert_eq!(events[0].peer_commitment(), events[1].peer_commitment());
        assert_eq!(events[0].link_generation(), events[1].link_generation());
        assert_eq!(events[0].message_id(), events[1].message_id());
    }
}
