//! Deterministic Plan 036 transport and adversarial-validation evidence.

use std::time::Duration;

use i2pr_core::ResourceClass;
use i2pr_testkit::{
    FaultAction, FaultMatcher, FaultRule, FaultScript, LinkDirection, ManualClock,
    NetworkScheduler, ReproducibilitySeed, SchedulerConfig, StreamConfig, assert_payload_bounds,
    assert_snapshot_redaction, resource_usage, synthetic_i2np_payload, synthetic_link_candidate,
    transport_manager_for_test,
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

#[test]
fn fixed_seed_integrated_matrix_covers_256_bounded_schedules() {
    for seed_value in 0_u128..=255 {
        let seed = ReproducibilitySeed::from_u128(seed_value);
        let link_id = i2pr_testkit::LinkId::new(seed_value as u32 + 1).expect("link id");
        let delay = std::time::Duration::from_millis((seed_value % 8) as u64);
        let faults = FaultScript::new(
            seed,
            vec![FaultRule::new(
                36,
                FaultMatcher::any()
                    .kind(i2pr_testkit::FaultUnitKind::Stream)
                    .direction(LinkDirection::AtoB),
                FaultAction::Delay(delay),
            )],
        )
        .expect("bounded fault script");
        let scheduler = NetworkScheduler::new(ManualClock::new(), SchedulerConfig::default())
            .expect("scheduler");
        let link = scheduler
            .stream_link(
                link_id,
                StreamConfig::new(16, 4).expect("stream config"),
                faults,
            )
            .expect("stream link");
        let payload = [seed_value as u8, (seed_value >> 8) as u8, 0x36, 0xa5];
        assert_eq!(
            link.left().try_write(&payload).expect("write"),
            payload.len()
        );
        scheduler
            .advance(std::time::Duration::from_millis(8))
            .expect("advance");
        let mut received = [0_u8; 4];
        assert_eq!(
            link.right().try_read(&mut received).expect("read"),
            Some(payload.len())
        );
        assert_eq!(received, payload);
        let snapshot = scheduler.snapshot();
        assert_eq!(snapshot.pending_deliveries, 0);
        assert_eq!(snapshot.buffered_bytes, 0);
        scheduler.close();
    }
}
