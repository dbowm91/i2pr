//! Deterministic Plan 031 transport-contract evidence.

use std::time::Duration;

use i2pr_core::ResourceClass;
use i2pr_testkit::{
    assert_payload_bounds, assert_snapshot_redaction, resource_usage, synthetic_i2np_payload,
    synthetic_link_candidate, transport_manager_for_test,
};
use i2pr_transport::{
    Deadline, DeliveryRequest, DuplicateResolution, EncodedI2npMessage, LinkId,
    MAX_I2NP_MESSAGE_BYTES, RegistrationOutcome, TerminationCategory,
};

#[test]
fn payload_bounds_cover_zero_maximum_and_plus_one() {
    assert_payload_bounds();
    assert_eq!(
        synthetic_i2np_payload(MAX_I2NP_MESSAGE_BYTES)
            .unwrap()
            .len(),
        MAX_I2NP_MESSAGE_BYTES
    );
}

#[test]
fn candidate_and_duplicate_decisions_are_deterministic() {
    let manager = transport_manager_for_test();
    assert_eq!(
        manager
            .resolve_candidate(
                synthetic_link_candidate(1, 1),
                Duration::ZERO,
                DuplicateResolution::RejectNew
            )
            .unwrap(),
        RegistrationOutcome::AcceptFirst {
            link_id: LinkId::new(1).unwrap()
        }
    );
    assert!(matches!(
        manager
            .resolve_candidate(
                synthetic_link_candidate(2, 1),
                Duration::ZERO,
                DuplicateResolution::RejectNew
            )
            .unwrap(),
        RegistrationOutcome::RejectNewDuplicate { .. }
    ));
    assert_eq!(
        manager
            .resolve_candidate(
                synthetic_link_candidate(2, 1),
                Duration::ZERO,
                DuplicateResolution::ReplaceExisting
            )
            .unwrap(),
        RegistrationOutcome::ReplaceExisting {
            existing: LinkId::new(1).unwrap(),
            candidate: LinkId::new(2).unwrap()
        }
    );
    assert_eq!(
        manager
            .close_link(LinkId::new(1).unwrap(), TerminationCategory::IoClosure)
            .unwrap(),
        i2pr_transport::CloseOutcome::Stale {
            link_id: LinkId::new(1).unwrap()
        }
    );
}

#[test]
fn queue_and_handshake_leases_return_to_zero() {
    let manager = transport_manager_for_test();
    manager
        .resolve_candidate(
            synthetic_link_candidate(1, 1),
            Duration::ZERO,
            DuplicateResolution::RejectNew,
        )
        .unwrap();
    let first = manager
        .admit_handshake(i2pr_testkit::synthetic_transport_peer(1))
        .unwrap();
    let second = manager
        .admit_handshake(i2pr_testkit::synthetic_transport_peer(2))
        .unwrap();
    assert!(
        manager
            .admit_handshake(i2pr_testkit::synthetic_transport_peer(3))
            .is_err()
    );
    drop(first);
    drop(second);
    assert_eq!(
        resource_usage(&manager, ResourceClass::PendingHandshakes).unwrap(),
        0
    );

    let request = DeliveryRequest::new(
        i2pr_testkit::synthetic_transport_peer(1),
        EncodedI2npMessage::new(vec![1, 2, 3]).unwrap(),
        Deadline::new(Duration::from_secs(60)).unwrap(),
    )
    .unwrap();
    let queued = manager.enqueue_delivery(request, Duration::ZERO).unwrap();
    assert_eq!(
        resource_usage(&manager, ResourceClass::BufferedBytes).unwrap(),
        3
    );
    let _ = manager
        .close_link(LinkId::new(1).unwrap(), TerminationCategory::IoClosure)
        .unwrap();
    drop(queued);
    assert_eq!(
        resource_usage(&manager, ResourceClass::BufferedBytes).unwrap(),
        0
    );
    assert_eq!(
        resource_usage(&manager, ResourceClass::CommandQueueItems).unwrap(),
        0
    );
    assert_eq!(
        resource_usage(&manager, ResourceClass::ActiveLinks).unwrap(),
        0
    );
}

#[test]
fn snapshots_redact_peer_and_payload_data() {
    let manager = transport_manager_for_test();
    assert_snapshot_redaction(&manager);
    let payload = synthetic_i2np_payload(8).unwrap();
    assert!(!format!("{payload:?}").contains("1, 2"));
}
