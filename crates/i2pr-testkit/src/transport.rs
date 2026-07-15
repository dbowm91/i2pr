//! Deterministic synthetic helpers for Plan 031 transport contracts.
//!
//! These factories create local identifiers and bounded bytes only. They do
//! not create router identities, sockets, addresses, or interoperability
//! fixtures.

use std::time::Duration;

use i2pr_core::{ResourceClass, ResourceError};
use i2pr_transport::{
    EncodedI2npMessage, LinkCandidate, LinkId, MAX_I2NP_MESSAGE_BYTES, PeerId, TransportKind,
    TransportLimits, TransportManager, TransportResources,
};

/// Creates a deterministic transport peer reference from a bounded index.
pub const fn synthetic_transport_peer(index: u8) -> PeerId {
    PeerId::from_bytes([index; 32])
}

/// Creates bounded synthetic encoded bytes for a transport contract test.
pub fn synthetic_i2np_payload(
    length: usize,
) -> Result<EncodedI2npMessage, i2pr_transport::PayloadError> {
    let mut bytes = vec![0_u8; length];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = (index as u8).wrapping_add(1);
    }
    EncodedI2npMessage::new(bytes)
}

/// Creates one deterministic authenticated link candidate.
pub fn synthetic_link_candidate(index: u64, peer: u8) -> LinkCandidate {
    let mut candidate = LinkCandidate::with_id(
        LinkId::new(index).expect("synthetic link identifier is bounded and nonzero"),
        synthetic_transport_peer(peer),
        TransportKind::Ntcp2,
        i2pr_transport::Direction::Inbound,
    );
    candidate
        .begin_handshake()
        .expect("synthetic handshake transition");
    candidate
        .authenticate()
        .expect("synthetic authentication transition");
    candidate
}

/// Returns a small deterministic transport resource fixture.
pub fn transport_resources_for_test() -> TransportResources {
    TransportResources::new(TransportLimits::for_test()).expect("test limits are valid")
}

/// Returns a manager using the bounded synthetic transport ceilings.
pub fn transport_manager_for_test() -> TransportManager {
    TransportManager::new(TransportLimits::for_test()).expect("test limits are valid")
}

/// Asserts the transport payload bound without retaining payload bytes.
pub fn assert_payload_bounds() {
    assert!(synthetic_i2np_payload(0).is_err());
    assert_eq!(
        synthetic_i2np_payload(MAX_I2NP_MESSAGE_BYTES)
            .unwrap()
            .len(),
        MAX_I2NP_MESSAGE_BYTES
    );
    assert!(synthetic_i2np_payload(MAX_I2NP_MESSAGE_BYTES + 1).is_err());
}

/// Asserts the default snapshot/debug boundary is payload- and peer-redacted.
pub fn assert_snapshot_redaction(manager: &TransportManager) {
    let snapshot = manager
        .snapshot(Duration::from_secs(1))
        .expect("bounded snapshot");
    let debug = format!("{snapshot:?}");
    assert!(!debug.contains("PeerId"));
    assert!(!debug.contains("EncodedI2npMessage"));
}

/// Reads one resource usage value for deterministic teardown assertions.
pub fn resource_usage(
    manager: &TransportManager,
    class: ResourceClass,
) -> Result<u64, ResourceError> {
    manager.resources().usage(class).map(|usage| usage.used)
}
