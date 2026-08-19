//! Deterministic destination fixtures.
//!
//! Plan 120 §6/§12 forbids production placeholder established tunnels. The
//! helpers below therefore drive the real `i2pr-tunnel` short tunnel-build
//! state machine to `Established` with a deterministic responder and hand back
//! the genuine one-shot [`EstablishedMaterial`]. They exist so unit tests and
//! the Plan 120 integration trajectory exercise the same production seams; they
//! are never used by the destination runtime itself.

use i2pr_proto::{Date, Hash};
use i2pr_tunnel::{
    BuildAttemptId, BuildEvent, BuildOptions, EPHEMERAL_KEY_LEN, EciesX25519BuildCryptography,
    EstablishedMaterial, HopRole, HopSpec, MessageHopProcessor, ShortBuildOutcome, ShortBuildPath,
    ShortBuildStateMachine, ShortResponseCode, TunnelDirection, TunnelId,
};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;
use zeroize::Zeroizing;

const FIXTURE_HOP_COUNT: u8 = 2;

/// Deterministic X25519 private key for the fixture hop `index` of the tunnel
/// identified by `seed`.
fn hop_private_key(seed: u64, index: u8) -> [u8; EPHEMERAL_KEY_LEN] {
    let mut bytes = [0_u8; EPHEMERAL_KEY_LEN];
    let mut cursor = (seed as usize)
        .wrapping_mul(31)
        .wrapping_add(index as usize);
    for byte in bytes.iter_mut() {
        cursor = cursor.wrapping_mul(17).wrapping_add(11) % 251;
        *byte = cursor as u8;
    }
    bytes
}

fn hop_public_key(seed: u64, index: u8) -> [u8; EPHEMERAL_KEY_LEN] {
    let secret = x25519_dalek::StaticSecret::from(hop_private_key(seed, index));
    x25519_dalek::PublicKey::from(&secret).to_bytes()
}

fn hop_router_hash(seed: u64, index: u8) -> Hash {
    let mut bytes = [0_u8; 32];
    for (offset, byte) in bytes.iter_mut().enumerate() {
        *byte = index.wrapping_add(offset as u8) ^ (seed as u8).wrapping_add(offset as u8);
    }
    Hash::from_bytes(bytes)
}

/// Builds a deterministic two-hop [`ShortBuildPath`] for the requested
/// direction. Every tunnel identifier is derived from `seed` so two fixtures
/// never collide inside one bounded pool.
pub fn fixture_path(seed: u64, direction: TunnelDirection) -> ShortBuildPath {
    let mut hops = Vec::new();
    for index in 1..=FIXTURE_HOP_COUNT {
        let receive = TunnelId::new(
            0x0010_0000_u32
                .wrapping_add((seed as u32).wrapping_mul(0x100))
                .wrapping_add(u32::from(index)),
        )
        .expect("nonzero receive tunnel id");
        let role = match (direction, index) {
            (TunnelDirection::Inbound, 1) => HopRole::InboundGateway,
            (TunnelDirection::Outbound, value) if value == FIXTURE_HOP_COUNT => {
                HopRole::OutboundEndpoint
            }
            _ => HopRole::Participant,
        };
        hops.push(HopSpec::new(
            hop_router_hash(seed, index),
            hop_public_key(seed, index),
            role,
            receive,
            receive,
        ));
    }
    // Plan 114 §4.4: every intermediate hop forwards into the following hop's
    // receive tunnel.
    for index in 0..hops.len().saturating_sub(1) {
        hops[index].next_tunnel = hops[index + 1].receive_tunnel;
    }
    let (originator_hash, outbound_reply_router) = match direction {
        TunnelDirection::Outbound => (None, Some(hop_router_hash(seed, 0xCD))),
        TunnelDirection::Inbound => (Some(hop_router_hash(seed, 0xAB)), None),
    };
    ShortBuildPath {
        attempt_id: BuildAttemptId::new(seed),
        direction,
        originator_hash,
        outbound_reply_router,
        creator_tunnel_id: TunnelId::new(0x0100_0000_u32.wrapping_add(seed as u32))
            .expect("nonzero creator tunnel id"),
        hops,
        request_time: Date::from_millis(60_000),
        next_message_id: 0x1234_5678,
        options: BuildOptions::empty(),
    }
}

/// Drives a deterministic short tunnel build to `Established` and returns the
/// genuine [`EstablishedMaterial`] the destination pool consumes.
///
/// # Panics
///
/// Panics when the deterministic trajectory fails to reach `Established`; that
/// would be a regression in the `i2pr-tunnel` short-build implementation.
pub fn established_material(seed: u64, direction: TunnelDirection) -> EstablishedMaterial {
    let path = fixture_path(seed, direction);
    path.validate().expect("fixture path is valid");
    let mut machine = ShortBuildStateMachine::new(path, 60_000);
    let mut rng = ChaCha8Rng::seed_from_u64(seed ^ 0x5EED);
    let message = machine.prepare(&mut rng).expect("prepare");
    let _action = machine.deliver_action(message).expect("deliver action");
    machine.mark_dispatched().expect("dispatch");
    let cryptography = EciesX25519BuildCryptography::new();
    let mut payload = machine.last_payload().expect("prepared payload").to_vec();
    for index in 1..=FIXTURE_HOP_COUNT {
        let (next_payload, _result) = MessageHopProcessor::process_hop(
            &cryptography,
            &payload,
            &hop_private_key(seed, index),
            &hop_router_hash(seed, index),
            ShortResponseCode::Accepted,
            &mut rng,
        )
        .expect("deterministic hop processing");
        payload = next_payload;
    }
    let outcome = machine
        .handle_event(BuildEvent::BuildReply {
            reply: Zeroizing::new(payload),
        })
        .expect("build reply");
    assert!(
        matches!(outcome, Some(ShortBuildOutcome::Established { .. })),
        "deterministic fixture must reach Established"
    );
    machine
        .take_established_material(0)
        .expect("established material")
}

/// Deterministic inbound established material.
pub fn established_inbound(seed: u64) -> EstablishedMaterial {
    established_material(seed, TunnelDirection::Inbound)
}

/// Deterministic outbound established material.
pub fn established_outbound(seed: u64) -> EstablishedMaterial {
    established_material(seed, TunnelDirection::Outbound)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inbound_fixture_produces_real_inbound_material() {
        let material = established_inbound(1);
        assert_eq!(material.direction(), TunnelDirection::Inbound);
        assert_eq!(material.hops().len(), usize::from(FIXTURE_HOP_COUNT));
        assert_eq!(
            material.inbound_gateway().0.hash(),
            hop_router_hash(1, 1),
            "the inbound gateway is the first remote hop"
        );
    }

    #[test]
    fn outbound_fixture_produces_real_outbound_material() {
        let material = established_outbound(2);
        assert_eq!(material.direction(), TunnelDirection::Outbound);
        assert_eq!(material.hops().len(), usize::from(FIXTURE_HOP_COUNT));
    }

    #[test]
    fn distinct_seeds_produce_distinct_gateways_and_tunnel_ids() {
        let first = established_inbound(3);
        let second = established_inbound(4);
        assert_ne!(
            first.inbound_gateway().0.hash(),
            second.inbound_gateway().0.hash()
        );
        assert_ne!(first.creator_tunnel_id(), second.creator_tunnel_id());
    }
}
