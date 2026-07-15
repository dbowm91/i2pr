//! Contract-level tests below the runtime and socket boundary.

use std::time::Duration;

use i2pr_core::ResourceClass;
use i2pr_transport::{
    Deadline, DeliveryRequest, DuplicateResolution, EncodedI2npMessage, LinkCandidate, LinkId,
    LinkState, MAX_I2NP_MESSAGE_BYTES, PeerId, RegistrationOutcome, TerminationCategory,
    TransportKind, TransportLimits, TransportManager,
};

fn candidate(id: u64, peer: u8) -> LinkCandidate {
    let mut candidate = LinkCandidate::with_id(
        LinkId::new(id).expect("bounded id"),
        PeerId::from_bytes([peer; 32]),
        TransportKind::Ntcp2,
        i2pr_transport::Direction::Inbound,
    );
    candidate.begin_handshake().expect("handshake transition");
    candidate.authenticate().expect("candidate authentication");
    candidate
}

fn request(peer: u8, length: usize) -> DeliveryRequest {
    DeliveryRequest::new(
        PeerId::from_bytes([peer; 32]),
        EncodedI2npMessage::new(vec![0xA5; length]).expect("bounded payload"),
        Deadline::new(Duration::from_secs(60)).expect("bounded deadline"),
    )
    .expect("delivery id")
}

#[test]
fn payload_bounds_and_debug_are_strict() {
    assert!(EncodedI2npMessage::new(Vec::new()).is_err());
    let maximum = EncodedI2npMessage::new(vec![0; MAX_I2NP_MESSAGE_BYTES]).unwrap();
    assert_eq!(maximum.len(), MAX_I2NP_MESSAGE_BYTES);
    assert!(EncodedI2npMessage::new(vec![0; MAX_I2NP_MESSAGE_BYTES + 1]).is_err());
    assert!(!format!("{maximum:?}").contains("0, 0"));
}

#[test]
fn lifecycle_rejects_reauthentication_from_terminal_state() {
    assert!(
        LinkState::Authenticated
            .transition(LinkState::Closing)
            .is_ok()
    );
    assert!(
        LinkState::Closed
            .transition(LinkState::Authenticated)
            .is_err()
    );
    assert!(
        LinkState::Failed
            .transition(LinkState::Authenticated)
            .is_err()
    );
}

#[test]
fn first_duplicate_replace_and_stale_close_are_typed() {
    let manager = TransportManager::new(TransportLimits::for_test()).unwrap();
    assert_eq!(
        manager
            .resolve_candidate(
                candidate(1, 1),
                Duration::ZERO,
                DuplicateResolution::RejectNew
            )
            .unwrap(),
        RegistrationOutcome::AcceptFirst {
            link_id: LinkId::new(1).unwrap()
        }
    );
    assert_eq!(
        manager
            .resolve_candidate(
                candidate(2, 1),
                Duration::ZERO,
                DuplicateResolution::RejectNew
            )
            .unwrap(),
        RegistrationOutcome::RejectNewDuplicate {
            existing: LinkId::new(1).unwrap()
        }
    );
    assert_eq!(
        manager
            .resolve_candidate(
                candidate(2, 1),
                Duration::ZERO,
                DuplicateResolution::ReplaceExisting
            )
            .unwrap(),
        RegistrationOutcome::ReplaceExisting {
            existing: LinkId::new(1).unwrap(),
            candidate: LinkId::new(2).unwrap()
        }
    );
    assert!(matches!(
        manager
            .close_link(LinkId::new(1).unwrap(), TerminationCategory::IoClosure)
            .unwrap(),
        i2pr_transport::CloseOutcome::Stale { .. }
    ));
    assert_eq!(
        manager
            .resource_usage(ResourceClass::ActiveLinks)
            .unwrap()
            .used,
        1
    );
}

#[test]
fn queue_and_pending_handshake_leases_release_exactly() {
    let manager = TransportManager::new(TransportLimits::for_test()).unwrap();
    manager
        .resolve_candidate(
            candidate(1, 1),
            Duration::ZERO,
            DuplicateResolution::RejectNew,
        )
        .unwrap();
    let first = manager
        .admit_handshake(PeerId::from_bytes([1; 32]))
        .unwrap();
    let second = manager
        .admit_handshake(PeerId::from_bytes([2; 32]))
        .unwrap();
    assert!(
        manager
            .admit_handshake(PeerId::from_bytes([3; 32]))
            .is_err()
    );
    drop(first);
    drop(second);
    assert_eq!(
        manager
            .resource_usage(ResourceClass::PendingHandshakes)
            .unwrap()
            .used,
        0
    );

    let queued = manager
        .enqueue_delivery(request(1, 8), Duration::ZERO)
        .unwrap();
    assert_eq!(
        manager
            .resource_usage(ResourceClass::BufferedBytes)
            .unwrap()
            .used,
        8
    );
    drop(queued);
    assert_eq!(
        manager
            .resource_usage(ResourceClass::BufferedBytes)
            .unwrap()
            .used,
        0
    );
    assert_eq!(
        manager
            .resource_usage(ResourceClass::CommandQueueItems)
            .unwrap()
            .used,
        0
    );
}

#[test]
fn cancelled_delivery_is_reported_without_resource_admission() {
    let manager = TransportManager::new(TransportLimits::for_test()).unwrap();
    manager
        .resolve_candidate(
            candidate(1, 1),
            Duration::ZERO,
            DuplicateResolution::RejectNew,
        )
        .unwrap();
    let cancellation = i2pr_core::CancellationToken::default();
    cancellation.cancel();
    let request = request(1, 4);
    let request = request.with_cancellation(cancellation);
    assert!(matches!(
        manager.enqueue_delivery(request, Duration::ZERO),
        Err(i2pr_transport::DeliveryOutcome::Cancelled)
    ));
    assert_eq!(
        manager
            .resource_usage(ResourceClass::BufferedBytes)
            .unwrap()
            .used,
        0
    );
}

#[test]
fn capacity_one_rejects_second_link_without_partial_usage() {
    let limits = TransportLimits::new(1, 1, 1024, 1, 1, 1, 1024).unwrap();
    let manager = TransportManager::new(limits).unwrap();
    manager
        .resolve_candidate(
            candidate(1, 1),
            Duration::ZERO,
            DuplicateResolution::RejectNew,
        )
        .unwrap();
    let result = manager
        .resolve_candidate(
            candidate(2, 2),
            Duration::ZERO,
            DuplicateResolution::RejectNew,
        )
        .unwrap();
    assert!(matches!(
        result,
        RegistrationOutcome::RejectGlobalLimit { .. }
    ));
    assert_eq!(
        manager
            .resource_usage(ResourceClass::ActiveLinks)
            .unwrap()
            .used,
        1
    );
}
