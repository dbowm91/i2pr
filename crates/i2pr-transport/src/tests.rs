use std::time::Duration;

use crate::{
    AddressFamily, AddressOrigin, CandidateDecision, Confidence, Deadline, DeliveryOutcome,
    DeliveryRequest, Direction, DuplicateResolution, EncodedI2npMessage, LinkCandidate, LinkId,
    LinkState, MAX_I2NP_MESSAGE_BYTES, MAX_REACHABILITY_OBSERVATIONS, PeerId, Reachability,
    ReachabilityObservation, ReachabilityRecordOutcome, ResourceClass, TerminationCategory,
    TransportKind, TransportLimits, TransportManager, ValidationState,
};

fn peer(value: u8) -> PeerId {
    PeerId::from_bytes([value; 32])
}

fn candidate(id: u64, peer_value: u8) -> LinkCandidate {
    let mut candidate = LinkCandidate::with_id(
        LinkId::new(id).expect("bounded link id"),
        peer(peer_value),
        TransportKind::Ntcp2,
        Direction::Inbound,
    );
    candidate.begin_handshake().expect("handshake transition");
    candidate.authenticate().expect("authentication transition");
    candidate
}

fn limits(
    pending: u64,
    active: u64,
    per_peer: u64,
    queue: u64,
    bytes: u64,
    per_link_queue: u64,
    per_link_bytes: u64,
) -> TransportLimits {
    TransportLimits::new(
        pending,
        active,
        bytes,
        queue,
        per_peer,
        per_link_queue,
        per_link_bytes,
    )
    .expect("valid test limits")
}

fn manager() -> TransportManager {
    TransportManager::new(limits(1, 2, 1, 1, 64, 1, 64)).expect("manager")
}

fn request(peer_value: u8, length: usize, deadline: u64) -> DeliveryRequest {
    DeliveryRequest::new(
        peer(peer_value),
        EncodedI2npMessage::new(vec![peer_value; length]).expect("bounded payload"),
        Deadline::new(Duration::from_secs(deadline)).expect("bounded deadline"),
    )
    .expect("delivery id")
}

#[test]
fn payload_bounds_and_diagnostics_are_safe() {
    assert!(LinkId::new(0).is_err());
    assert!(LinkId::new(crate::MAX_LINK_ID).is_ok());
    assert!(LinkId::new(crate::MAX_LINK_ID + 1).is_err());
    assert!(EncodedI2npMessage::new(Vec::new()).is_err());
    let maximum =
        EncodedI2npMessage::new(vec![0; MAX_I2NP_MESSAGE_BYTES]).expect("maximum payload");
    assert_eq!(maximum.len(), MAX_I2NP_MESSAGE_BYTES);
    assert!(EncodedI2npMessage::new(vec![0; MAX_I2NP_MESSAGE_BYTES + 1]).is_err());
    assert!(!format!("{maximum:?}").contains("0000"));
    assert!(!format!("{:?}", peer(0xab)).contains("ab"));
}

#[test]
fn first_link_limits_and_duplicate_decisions_are_typed() {
    let manager = manager();
    assert_eq!(
        manager
            .register_authenticated(
                candidate(1, 1),
                Duration::ZERO,
                DuplicateResolution::RejectNew,
            )
            .expect("first"),
        CandidateDecision::AcceptFirst {
            link_id: LinkId::new(1).expect("id"),
        }
    );
    assert_eq!(
        manager
            .register_authenticated(
                candidate(2, 1),
                Duration::ZERO,
                DuplicateResolution::RejectNew,
            )
            .expect("duplicate"),
        CandidateDecision::RejectNewDuplicate {
            existing: LinkId::new(1).expect("id"),
        }
    );
    assert_eq!(
        manager
            .register_authenticated(
                candidate(2, 1),
                Duration::ZERO,
                DuplicateResolution::RetainExistingDrainNew,
            )
            .expect("drain"),
        CandidateDecision::RetainExistingDrainNew {
            existing: LinkId::new(1).expect("id"),
            candidate: LinkId::new(2).expect("id"),
        }
    );
    assert_eq!(
        manager
            .register_authenticated(
                candidate(2, 1),
                Duration::ZERO,
                DuplicateResolution::ReplaceExisting,
            )
            .expect("replace"),
        CandidateDecision::ReplaceExisting {
            existing: LinkId::new(1).expect("id"),
            candidate: LinkId::new(2).expect("id"),
        }
    );
}

#[test]
fn active_limit_and_stale_close_preserve_replacements() {
    let manager = manager();
    manager
        .register_authenticated(
            candidate(1, 1),
            Duration::ZERO,
            DuplicateResolution::RejectNew,
        )
        .expect("first");
    manager
        .register_authenticated(
            candidate(2, 2),
            Duration::ZERO,
            DuplicateResolution::RejectNew,
        )
        .expect("second");
    assert_eq!(
        manager
            .register_authenticated(
                candidate(3, 3),
                Duration::ZERO,
                DuplicateResolution::RejectNew,
            )
            .expect("global limit"),
        CandidateDecision::RejectGlobalLimit { maximum: 2 }
    );
    manager
        .close_link(LinkId::new(1).expect("old"), TerminationCategory::IoClosure)
        .expect("close");
    assert_eq!(
        manager
            .resource_usage(ResourceClass::ActiveLinks)
            .expect("resource")
            .used,
        1
    );
    assert_eq!(
        manager
            .close_link(
                LinkId::new(1).expect("stale"),
                TerminationCategory::IoClosure
            )
            .expect("stale close"),
        crate::CloseOutcome::Stale {
            link_id: LinkId::new(1).expect("stale"),
        }
    );
}

#[test]
fn pending_handshake_lease_releases_on_drop_and_completion() {
    let pending_manager = manager();
    let first = pending_manager
        .begin_handshake(peer(2))
        .expect("first pending");
    assert!(pending_manager.begin_handshake(peer(3)).is_err());
    drop(first);
    let manager = manager();
    let pending = manager.begin_handshake(peer(1)).expect("pending");
    assert_eq!(
        manager
            .resource_usage(ResourceClass::PendingHandshakes)
            .expect("resource")
            .used,
        1
    );
    drop(pending);
    assert_eq!(
        manager
            .resource_usage(ResourceClass::PendingHandshakes)
            .expect("resource")
            .used,
        0
    );
    let pending = manager.begin_handshake(peer(1)).expect("pending");
    pending
        .register(
            &manager,
            candidate(1, 1),
            Duration::ZERO,
            DuplicateResolution::RejectNew,
        )
        .expect("completion");
    assert_eq!(
        manager
            .resource_usage(ResourceClass::PendingHandshakes)
            .expect("resource")
            .used,
        0
    );
}

#[test]
fn queue_item_and_byte_leases_release_on_drop_and_handoff() {
    let manager = manager();
    manager
        .register_authenticated(
            candidate(1, 1),
            Duration::ZERO,
            DuplicateResolution::RejectNew,
        )
        .expect("link");
    let queued = manager
        .enqueue_delivery(request(1, 4, 10), Duration::ZERO)
        .expect("queue");
    assert_eq!(
        manager
            .resource_usage(ResourceClass::CommandQueueItems)
            .expect("items")
            .used,
        1
    );
    assert_eq!(
        manager
            .resource_usage(ResourceClass::BufferedBytes)
            .expect("bytes")
            .used,
        4
    );
    drop(queued);
    assert_eq!(
        manager
            .resource_usage(ResourceClass::BufferedBytes)
            .expect("bytes")
            .used,
        0
    );
    let queued = manager
        .enqueue_delivery(request(1, 4, 10), Duration::ZERO)
        .expect("queue");
    let request = queued.into_request();
    assert_eq!(request.message_len(), 4);
    assert_eq!(
        manager.snapshot(Duration::ZERO).expect("snapshot").links[0].queued_messages,
        0
    );
}

#[test]
fn queue_deadline_resource_and_closed_link_outcomes_are_typed() {
    let manager = manager();
    assert!(matches!(
        manager.enqueue_delivery(request(1, 1, 10), Duration::ZERO),
        Err(DeliveryOutcome::NoActiveLink)
    ));
    manager
        .register_authenticated(
            candidate(1, 1),
            Duration::ZERO,
            DuplicateResolution::RejectNew,
        )
        .expect("link");
    let first = manager
        .enqueue_delivery(request(1, 4, 10), Duration::ZERO)
        .expect("first");
    assert!(matches!(
        manager.enqueue_delivery(request(1, 4, 10), Duration::ZERO),
        Err(DeliveryOutcome::QueueFull)
    ));
    drop(first);
    assert!(matches!(
        manager.enqueue_delivery(request(1, 4, 1), Duration::from_secs(1)),
        Err(DeliveryOutcome::DeadlineElapsed)
    ));
    let cancellation = i2pr_core::CancellationToken::default();
    cancellation.cancel();
    assert!(matches!(
        manager.enqueue_delivery(
            request(1, 4, 10).with_cancellation(cancellation),
            Duration::ZERO,
        ),
        Err(DeliveryOutcome::Cancelled)
    ));
    let capability = manager.delivery_capability(peer(1)).expect("capability");
    manager
        .transition_link(capability.link_id(), LinkState::Draining)
        .expect("draining");
    assert!(matches!(
        manager.enqueue_on_link(capability, request(1, 4, 10), Duration::ZERO),
        Err(DeliveryOutcome::LinkClosedBeforeWrite)
    ));

    let limited = TransportManager::new(limits(1, 2, 1, 2, 4, 2, 4)).expect("limited");
    limited
        .register_authenticated(
            candidate(10, 10),
            Duration::ZERO,
            DuplicateResolution::RejectNew,
        )
        .expect("link");
    limited
        .register_authenticated(
            candidate(11, 11),
            Duration::ZERO,
            DuplicateResolution::RejectNew,
        )
        .expect("second link");
    let held = limited
        .enqueue_delivery(request(10, 3, 10), Duration::ZERO)
        .expect("first limited queue item");
    assert!(matches!(
        limited.enqueue_delivery(request(11, 2, 10), Duration::ZERO),
        Err(DeliveryOutcome::ResourceDenied)
    ));
    drop(held);
    assert_eq!(
        limited
            .resource_usage(ResourceClass::CommandQueueItems)
            .expect("items")
            .used,
        0
    );
}

#[test]
fn bounded_observations_and_snapshots_are_privacy_safe() {
    let manager = manager();
    for index in 0..=MAX_REACHABILITY_OBSERVATIONS {
        let outcome = manager
            .record_reachability(ReachabilityObservation {
                transport: TransportKind::Ntcp2,
                origin: AddressOrigin::Observed,
                family: AddressFamily::Ipv4,
                reachability: Reachability::Unknown,
                observed_at: Duration::from_secs(index as u64),
                validation: ValidationState::Unvalidated,
                confidence: Some(Confidence::new(0).expect("confidence")),
            })
            .expect("observation");
        if index == MAX_REACHABILITY_OBSERVATIONS {
            assert_eq!(outcome, ReachabilityRecordOutcome::EvictedOldest);
        }
    }
    manager
        .register_authenticated(
            candidate(1, 0xab),
            Duration::ZERO,
            DuplicateResolution::RejectNew,
        )
        .expect("link");
    let snapshot = manager.snapshot(Duration::from_secs(2)).expect("snapshot");
    assert_eq!(snapshot.observations.len(), MAX_REACHABILITY_OBSERVATIONS);
    assert!(!format!("{snapshot:?}").contains("171"));
}

#[test]
fn snapshot_links_are_sorted_by_local_id() {
    let manager = manager();
    manager
        .register_authenticated(
            candidate(2, 2),
            Duration::ZERO,
            DuplicateResolution::RejectNew,
        )
        .expect("second id");
    manager
        .register_authenticated(
            candidate(1, 1),
            Duration::ZERO,
            DuplicateResolution::RejectNew,
        )
        .expect("first id");
    let snapshot = manager.snapshot(Duration::ZERO).expect("snapshot");
    assert_eq!(
        snapshot
            .links
            .iter()
            .map(|entry| entry.link_id.value())
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
}

#[test]
fn lifecycle_authentication_is_one_way() {
    assert!(
        LinkState::Candidate
            .transition(LinkState::Handshaking)
            .is_ok()
    );
    assert!(
        LinkState::Candidate
            .transition(LinkState::Authenticated)
            .is_err()
    );
    assert!(
        LinkState::Authenticated
            .transition(LinkState::Draining)
            .is_ok()
    );
    assert!(
        LinkState::Draining
            .transition(LinkState::Authenticated)
            .is_err()
    );
    assert!(
        LinkState::Closed
            .transition(LinkState::Authenticated)
            .is_err()
    );
}
