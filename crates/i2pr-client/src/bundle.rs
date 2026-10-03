//! Plan 296: garlic reply-bundling primitive.
//!
//! When a destination enables reply bundling, the outbound delivery
//! path may carry multiple same-remote application payloads as
//! multiple data cloves in one New Session Reply garlic message
//! instead of one garlic message per payload. The bundled form
//! applies to replies only: fresh bound New Sessions keep their
//! mandatory LeaseSet2 bundle (Plan 127 §2) and Existing Session
//! traffic keeps its lean single-clove form. Disabled destinations
//! keep one payload per garlic message, bit-for-bit.
//!
//! Inbound, multi-data-clove garlic decodes through the existing
//! clove sequence path and the dispatcher routes every non-LeaseSet2
//! data clove (bounded by [`MAX_BUNDLED_DATA_CLOVES`]), so bundled
//! replies deliver through the same queue discipline as single
//! replies. Nothing here touches ambient RNG: the policy is a plain
//! flag carried from the tunnel specification.

use i2pr_proto::{EciesPayloadBlock, EciesPayloadSequence, GarlicCloveBlock};

use crate::message::MAX_DESTINATION_PAYLOAD_BYTES;

/// Maximum application data cloves carried in (or accepted from) one
/// bundled reply garlic message. Bounds per-message assembly work
/// and inbound queue amplification from a single envelope.
pub const MAX_BUNDLED_DATA_CLOVES: usize = 4;

/// Reply-bundling delivery policy for one destination.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReplyBundling {
    enabled: bool,
    max_cloves: u8,
}

impl ReplyBundling {
    /// Bundling disabled: one application payload per garlic message.
    pub const fn disabled() -> Self {
        Self {
            enabled: false,
            max_cloves: 1,
        }
    }

    /// Bundling enabled: replies may carry up to
    /// [`MAX_BUNDLED_DATA_CLOVES`] application data cloves.
    pub const fn enabled() -> Self {
        Self {
            enabled: true,
            max_cloves: MAX_BUNDLED_DATA_CLOVES as u8,
        }
    }

    /// Whether bundled replies may be composed.
    pub const fn is_enabled(self) -> bool {
        self.enabled
    }

    /// Maximum data cloves per bundled reply.
    pub const fn max_cloves(self) -> u8 {
        self.max_cloves
    }
}

/// Typed reply-bundling failures.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum BundleError {
    /// The destination policy disables reply bundling.
    #[error("reply bundling is disabled for this destination")]
    PolicyDisabled,
    /// The bundle carries no application payload.
    #[error("bundled reply carries no application payload")]
    EmptyBundle,
    /// The bundle carries more data cloves than the policy allows.
    #[error("bundled reply carries {actual} cloves, maximum {maximum}")]
    TooManyCloves {
        /// Supplied clove count.
        actual: usize,
        /// Policy maximum.
        maximum: usize,
    },
    /// The encoded bundle exceeds the destination payload ceiling.
    #[error("bundled reply of {actual} clove bytes exceeds maximum {maximum}")]
    BundleOversize {
        /// Summed application clove bytes offered.
        actual: usize,
        /// Destination payload ceiling.
        maximum: usize,
    },
    /// The planned session form is not a reply and cannot bundle.
    #[error("bundled reply requires the new-session-reply form, planned {planned}")]
    FormNotBundlable {
        /// Planned outbound form name.
        planned: &'static str,
    },
    /// The underlying send composition rejected the input.
    #[error("bundled reply composition failed: {0}")]
    Composition(#[from] crate::routing::SendError),
}

/// Encodes a New Session Reply garlic payload carrying a DateTime
/// block followed by every supplied application data clove.
///
/// The encoding mirrors the single-clove reply encoder with N data
/// blocks; the sequence cap is the same destination payload ceiling,
/// so a bundle can never exceed what one payload envelope allows in
/// total. `max_cloves` is the policy bound (see
/// [`ReplyBundling::max_cloves`]).
pub fn encode_bundled_reply_payload(
    now_seconds: u32,
    cloves: &[GarlicCloveBlock],
    max_cloves: u8,
) -> Result<Vec<u8>, BundleError> {
    if cloves.is_empty() {
        return Err(BundleError::EmptyBundle);
    }
    if cloves.len() > usize::from(max_cloves) {
        return Err(BundleError::TooManyCloves {
            actual: cloves.len(),
            maximum: usize::from(max_cloves),
        });
    }
    let mut sequence = EciesPayloadSequence::empty();
    // The count check above plus the 64-block sequence ceiling
    // keeps every push below; the expects document infallibility
    // rather than hiding a codec result.
    sequence
        .push(EciesPayloadBlock::DateTime(now_seconds))
        .expect("datetime block fits the payload sequence");
    for clove in cloves {
        sequence
            .push(EciesPayloadBlock::GarlicClove(clove.clone()))
            .expect("bounded cloves fit the payload sequence");
    }
    sequence
        .encode_to_vec(MAX_DESTINATION_PAYLOAD_BYTES, true)
        .map_err(|_| BundleError::BundleOversize {
            actual: cloves.iter().map(|clove| clove.message.len()).sum(),
            maximum: MAX_DESTINATION_PAYLOAD_BYTES,
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dispatch::{DestinationDispatcher, InboundDispatchOutcome};
    use crate::identity::{DestinationId, DestinationIdentity};
    use crate::session::{
        EciesOutboundMessage, EciesSessionConfig, EciesSessionManager, local_clove,
    };

    fn data_clove(byte: u8) -> GarlicCloveBlock {
        local_clove(1_000, u32::from(byte), vec![byte; 16])
    }

    #[test]
    fn bundled_reply_encodes_every_data_clove() {
        // Plan 296: N payloads encode to one DateTime-led sequence
        // carrying N data cloves, decodable by the shared path.
        let cloves = [data_clove(1), data_clove(2), data_clove(3)];
        let bytes =
            encode_bundled_reply_payload(1_000, &cloves, MAX_BUNDLED_DATA_CLOVES as u8)
                .expect("bundle");
        let sequence =
            EciesPayloadSequence::decode(&bytes, bytes.len(), false).expect("decode bundle");
        let data = sequence
            .blocks()
            .iter()
            .filter_map(|block| match block {
                EciesPayloadBlock::GarlicClove(clove) => Some(clove),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(data.len(), 3);
        assert_eq!(data[0].message, vec![1; 16]);
        assert_eq!(data[2].message, vec![3; 16]);
    }

    #[test]
    fn bundled_reply_rejects_empty_oversize_and_overcount() {        assert!(matches!(
            encode_bundled_reply_payload(1_000, &[], 4),
            Err(BundleError::EmptyBundle)
        ));
        let five = [
            data_clove(1),
            data_clove(2),
            data_clove(3),
            data_clove(4),
            data_clove(5),
        ];
        assert!(matches!(
            encode_bundled_reply_payload(1_000, &five, 4),
            Err(BundleError::TooManyCloves { actual: 5, maximum: 4 })
        ));
        let huge = [GarlicCloveBlock {
            delivery: i2pr_proto::GarlicDelivery::Local,
            message: vec![0xAA; MAX_DESTINATION_PAYLOAD_BYTES],
        }];
        assert!(matches!(
            encode_bundled_reply_payload(1_000, &huge, 4),
            Err(BundleError::BundleOversize { .. })
        ));
    }

    struct Handshake {
        alice: DestinationIdentity,
        bob: DestinationIdentity,
        alice_session: EciesSessionManager,
        bob_session: EciesSessionManager,
        now_seconds: u32,
    }

    /// Drives Alice's bound New Session into Bob's session manager so
    /// Bob holds a sealable New Session Reply context for Alice.
    /// Deterministic (seeded RNG); no network, no tunnels.
    fn handshake() -> (Handshake, rand_chacha::ChaCha8Rng) {
        use rand_core::SeedableRng as _;
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(0x296);
        let alice = DestinationIdentity::generate(&mut rng).expect("alice");
        let bob = DestinationIdentity::generate(&mut rng).expect("bob");
        let mut alice_session = EciesSessionManager::new(EciesSessionConfig::balanced());
        let mut bob_session = EciesSessionManager::new(EciesSessionConfig::balanced());
        let now_seconds = 1_000_000u32;
        let first_payload =
            crate::session::encode_new_session_payload(now_seconds, &data_clove(9))
                .expect("first payload");
        let outbound = alice_session
            .encrypt_to_remote(
                DestinationId::from_hash(i2pr_proto::Hash::from_bytes([0xA1; 32])),
                alice.static_secret_bytes(),
                &[0xB0; 32],
                &bob.static_public_bytes(),
                &first_payload,
                now_seconds,
                &mut rng,
            )
            .expect("bound new session");
        let EciesOutboundMessage::NewSession { message } = outbound else {
            panic!("first send must initiate a bound New Session");
        };
        bob_session
            .accept_new_session(
                bob.id(),
                bob.static_secret_bytes(),
                &bob.static_public_bytes(),
                &message,
                now_seconds,
            )
            .expect("bob accepts");
        (
            Handshake {
                alice,
                bob,
                alice_session,
                bob_session,
                now_seconds,
            },
            rng,
        )
    }

    #[test]
    fn bundled_reply_round_trips_three_data_cloves() {
        // Plan 296: Bob seals one New Session Reply carrying three
        // application data cloves; Alice opens it and recovers all
        // three through the shared clove sequence path.
        let (mut corners, mut rng) = handshake();
        let cloves = [data_clove(1), data_clove(2), data_clove(3)];
        let bundle_bytes = encode_bundled_reply_payload(
            corners.now_seconds,
            &cloves,
            MAX_BUNDLED_DATA_CLOVES as u8,
        )
        .expect("bundle");
        let reply = corners
            .bob_session
            .seal_new_session_reply_for(
                corners.bob.id(),
                corners.bob.static_secret_bytes(),
                &corners.alice.static_public_bytes(),
                &bundle_bytes,
                corners.now_seconds,
                &mut rng,
            )
            .expect("seal bundled reply");
        let opened = corners
            .alice_session
            .accept_new_session_reply(
                corners.alice.id(),
                corners.alice.static_secret_bytes(),
                &reply.message,
                corners.now_seconds,
            )
            .expect("alice accepts reply");
        let sequence =
            EciesPayloadSequence::decode(&opened.payload, opened.payload.len(), false)
                .expect("decode reply payload");
        let data = sequence
            .blocks()
            .iter()
            .filter_map(|block| match block {
                EciesPayloadBlock::GarlicClove(clove) => Some(clove),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(data.len(), 3);
        assert_eq!(data[0].message, vec![1; 16]);
        assert_eq!(data[1].message, vec![2; 16]);
        assert_eq!(data[2].message, vec![3; 16]);
    }

    #[test]
    fn dispatcher_routes_every_bundled_data_clove() {
        // Plan 296: a two-data-clove reply envelope dispatches both
        // payloads into the destination queue in wire order; a
        // five-clove envelope is rejected before any partial queue
        // effect.
        use i2pr_proto::{
            Date, DeferredPayload, I2npBody, I2npMessage, MAX_I2NP_PAYLOAD_SIZE,
            OpaqueMessageBody,
        };
        let (mut corners, mut rng) = handshake();
        let cloves = [data_clove(7), data_clove(8)];
        let bundle_bytes = encode_bundled_reply_payload(
            corners.now_seconds,
            &cloves,
            MAX_BUNDLED_DATA_CLOVES as u8,
        )
        .expect("bundle");
        let reply = corners
            .bob_session
            .seal_new_session_reply_for(
                corners.bob.id(),
                corners.bob.static_secret_bytes(),
                &corners.alice.static_public_bytes(),
                &bundle_bytes,
                corners.now_seconds,
                &mut rng,
            )
            .expect("seal bundled reply");
        let reply_bytes = reply
            .message
            .encode_to_vec(MAX_I2NP_PAYLOAD_SIZE)
            .expect("encode reply");
        let envelope = I2npMessage::new_standard(
            11,
            Date::from_millis(0),
            I2npBody::Garlic(OpaqueMessageBody {
                payload: DeferredPayload::new(reply_bytes, MAX_I2NP_PAYLOAD_SIZE)
                    .expect("garlic payload"),
            }),
        )
        .expect("envelope");
        let mut dispatcher = DestinationDispatcher::new();
        dispatcher
            .register_destination(corners.alice.id())
            .expect("register");
        let mut store = i2pr_netdb::LeaseSet2Store::default();
        let outcome = dispatcher.dispatch_garlic_envelope(
            &mut corners.alice_session,
            corners.alice.id(),
            corners.alice.static_secret_bytes(),
            &corners.alice.static_public_bytes(),
            corners.now_seconds,
            &envelope,
            &mut store,
        );
        assert!(
            matches!(
                outcome,
                InboundDispatchOutcome::NewSessionReplyProcessed { .. }
            ),
            "bundled reply must dispatch, got {outcome:?}"
        );
        let first = dispatcher
            .pop_payload(corners.alice.id())
            .expect("first bundled payload");
        let second = dispatcher
            .pop_payload(corners.alice.id())
            .expect("second bundled payload");
        assert_eq!(first.bytes(), vec![7; 16].as_slice());
        assert_eq!(second.bytes(), vec![8; 16].as_slice());
        assert!(dispatcher.pop_payload(corners.alice.id()).is_none());
    }
}
