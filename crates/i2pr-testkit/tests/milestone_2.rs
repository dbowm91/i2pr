//! Integrated, deterministic Milestone 2 validation scenarios.
//!
//! These tests stay below the transport boundary.  They use only supervised
//! services, bounded channels/resources, and the manual simulation scheduler.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use i2pr_core::{
    ResourceBudget, ResourceClass, ResourceLimit, ServiceClassification, ServiceCompletion,
    ServiceFailure, ServiceFailureCategory, ServiceName, ShutdownReason,
};
use i2pr_runtime::{
    ChannelSpec, RestartExhaustion, RestartPolicy, RouterLifecycle, RuntimeSnapshot, ServiceGraph,
    ServiceResult, ServiceSpec, SimulationSnapshot, Supervisor, SupervisorError, command_channel,
};
use i2pr_testkit::{
    DatagramConfig, FaultAction, FaultMatcher, FaultRule, FaultScript, FaultUnitKind,
    LinkDirection, LinkId, ManualClock, NetworkScheduler, ReproducibilitySeed, SchedulerConfig,
    StreamConfig,
};
use tokio::sync::Notify;

fn name(value: &str) -> ServiceName {
    ServiceName::new(value).expect("static service name")
}

fn forever_service(value: &str, classification: ServiceClassification) -> ServiceSpec {
    ServiceSpec::new(name(value), classification, |context| async move {
        context.signal_ready().expect("readiness is one-shot");
        context.cancellation().cancelled().await;
        ServiceResult::RequestedShutdown
    })
}

fn graph_with_services(services: impl IntoIterator<Item = ServiceSpec>) -> ServiceGraph {
    let mut builder = ServiceGraph::builder(8).expect("bounded graph");
    for service in services {
        builder.register(service).expect("unique service");
    }
    builder.build().expect("validated graph")
}

async fn wait_until_ready(handle: &i2pr_runtime::SupervisorHandle, maximum: usize) {
    for _ in 0..maximum {
        if handle.snapshot().ready {
            return;
        }
        tokio::task::yield_now().await;
    }
    panic!(
        "service graph did not become ready: {:?}",
        handle.snapshot()
    );
}

fn command_budget(maximum: u64) -> ResourceBudget {
    ResourceBudget::new([
        ResourceLimit::new(ResourceClass::CommandQueueItems, maximum).expect("positive limit"),
    ])
    .expect("budget")
}

#[tokio::test(start_paused = true)]
async fn scenario_clean_startup_shutdown_has_zero_final_usage() {
    let worker_policy = RestartPolicy::new(2, Duration::from_secs(1), Duration::from_secs(2))
        .expect("restart policy")
        .on_exhaustion(RestartExhaustion::Degrade);
    let coordinator = forever_service("coordinator", ServiceClassification::Essential);
    let worker = forever_service("worker", ServiceClassification::Restartable)
        .depends_on(name("coordinator"))
        .restart_policy(worker_policy);
    let observer = forever_service("observer", ServiceClassification::Degradable)
        .depends_on(name("coordinator"));
    let reporter =
        forever_service("reporter", ServiceClassification::Optional).depends_on(name("worker"));
    let supervisor = Supervisor::new(
        graph_with_services([coordinator, worker, observer, reporter]),
        Duration::from_secs(5),
    )
    .expect("supervisor");
    let handle = supervisor.handle();
    let budget = command_budget(8);
    let spec = ChannelSpec::command("scenario.command", name("coordinator"), 2)
        .expect("channel spec")
        .with_item_charge(ResourceClass::CommandQueueItems, 1)
        .expect("charge")
        .with_budget(budget.clone());
    let (sender, receiver) = command_channel::<u8>(spec).expect("channel");
    let task = tokio::spawn(supervisor.run());
    wait_until_ready(&handle, 64).await;
    assert_eq!(handle.snapshot().lifecycle, RouterLifecycle::Ready);

    handle.shutdown(ShutdownReason::Test);
    let report = task
        .await
        .expect("supervisor joined")
        .expect("graceful report");
    assert!(report.was_graceful());
    assert_eq!(report.remaining_tasks(), 0);
    assert!(report.forced_services().is_empty());
    drop(receiver);

    let snapshot = RuntimeSnapshot::try_new(
        handle.snapshot(),
        vec![sender.snapshot()],
        budget.snapshot().expect("resource snapshot"),
        SimulationSnapshot::default(),
    )
    .expect("aggregate snapshot");
    assert_eq!(snapshot.supervisor.lifecycle, RouterLifecycle::Stopped);
    assert_eq!(snapshot.supervisor.owned_service_tasks, 0);
    assert_eq!(snapshot.supervisor.owned_child_tasks, 0);
    assert_eq!(snapshot.channels[0].queued, 0);
    assert!(snapshot.resources.iter().all(|usage| usage.used == 0));
    assert_eq!(snapshot.simulation, SimulationSnapshot::default());
}

#[tokio::test(start_paused = true)]
async fn scenario_bounded_overload_reports_typed_denial_and_releases_lease() {
    let supervisor = Supervisor::new(
        graph_with_services([forever_service(
            "coordinator",
            ServiceClassification::Essential,
        )]),
        Duration::from_secs(2),
    )
    .expect("supervisor");
    let handle = supervisor.handle();
    let budget = command_budget(1);
    let spec = ChannelSpec::command("overload.command", name("coordinator"), 2)
        .expect("channel spec")
        .with_item_charge(ResourceClass::CommandQueueItems, 1)
        .expect("charge")
        .with_budget(budget.clone());
    let (sender, receiver) = command_channel::<u8>(spec).expect("channel");
    let task = tokio::spawn(supervisor.run());
    wait_until_ready(&handle, 32).await;

    sender.try_send(1).expect("first item admitted");
    let denied = sender.try_send(2).expect_err("second item must be denied");
    assert!(matches!(
        denied,
        i2pr_runtime::SendError::ResourceDenied { .. }
    ));
    let channel = sender.snapshot();
    assert_eq!(channel.queued, 1);
    assert_eq!(channel.resource_denied, 1);
    assert!(channel.queued <= channel.capacity);
    assert_eq!(
        budget.usage(ResourceClass::CommandQueueItems).unwrap().used,
        1
    );

    drop(receiver);
    assert_eq!(
        budget.usage(ResourceClass::CommandQueueItems).unwrap().used,
        0
    );
    handle.shutdown(ShutdownReason::Test);
    let report = task
        .await
        .expect("supervisor joined")
        .expect("graceful report");
    assert!(report.was_graceful());
    assert_eq!(handle.snapshot().owned_service_tasks, 0);
}

#[tokio::test(start_paused = true)]
async fn scenario_restart_recovery_uses_deterministic_backoff() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let attempts_for_factory = Arc::clone(&attempts);
    let policy = RestartPolicy::new(2, Duration::from_secs(1), Duration::from_secs(2))
        .expect("restart policy")
        .on_exhaustion(RestartExhaustion::Degrade);
    let worker = ServiceSpec::new(
        name("worker"),
        ServiceClassification::Restartable,
        move |context| {
            let attempts = Arc::clone(&attempts_for_factory);
            async move {
                let attempt = attempts.fetch_add(1, Ordering::SeqCst);
                if attempt == 0 {
                    return ServiceResult::Failed(ServiceFailure::new(
                        ServiceFailureCategory::Internal,
                        None,
                    ));
                }
                context.signal_ready().expect("readiness");
                context.cancellation().cancelled().await;
                ServiceResult::RequestedShutdown
            }
        },
    )
    .restart_policy(policy);
    let supervisor = Supervisor::new(
        graph_with_services([
            forever_service("coordinator", ServiceClassification::Essential),
            worker,
        ]),
        Duration::from_secs(5),
    )
    .expect("supervisor");
    let handle = supervisor.handle();
    let task = tokio::spawn(supervisor.run());
    for _ in 0..32 {
        if attempts.load(Ordering::SeqCst) == 1 {
            break;
        }
        tokio::task::yield_now().await;
    }
    assert_eq!(attempts.load(Ordering::SeqCst), 1);
    tokio::time::advance(Duration::from_secs(1)).await;
    for _ in 0..32 {
        if attempts.load(Ordering::SeqCst) == 2 {
            break;
        }
        tokio::task::yield_now().await;
    }
    wait_until_ready(&handle, 32).await;
    let worker_snapshot = handle
        .snapshot()
        .services
        .into_iter()
        .find(|service| service.service.as_str() == "worker")
        .expect("worker snapshot");
    assert_eq!(worker_snapshot.restart_count, 1);

    handle.shutdown(ShutdownReason::Test);
    let report = task
        .await
        .expect("supervisor joined")
        .expect("graceful report");
    assert!(report.was_graceful());
}

#[tokio::test(start_paused = true)]
async fn scenario_essential_failure_forces_only_noncooperative_service() {
    let fail = Arc::new(Notify::new());
    let fail_for_factory = Arc::clone(&fail);
    let essential = ServiceSpec::new(
        name("coordinator"),
        ServiceClassification::Essential,
        move |context| {
            let fail = Arc::clone(&fail_for_factory);
            async move {
                context.signal_ready().expect("readiness");
                tokio::select! {
                    _ = fail.notified() => ServiceResult::Failed(ServiceFailure::new(
                        ServiceFailureCategory::Internal,
                        i2pr_core::HealthDetail::new("bounded internal failure").ok(),
                    )),
                    _ = context.cancellation().cancelled() => ServiceResult::RequestedShutdown,
                }
            }
        },
    )
    .timeouts(
        Duration::from_secs(1),
        Duration::from_secs(1),
        Duration::from_secs(1),
    );
    let optional = ServiceSpec::new(
        name("reporter"),
        ServiceClassification::Optional,
        |context| async move {
            context.signal_ready().expect("readiness");
            std::future::pending::<ServiceResult>().await
        },
    )
    .depends_on(name("coordinator"))
    .timeouts(
        Duration::from_secs(1),
        Duration::from_secs(1),
        Duration::from_secs(1),
    );
    let supervisor = Supervisor::new(
        graph_with_services([essential, optional]),
        Duration::from_secs(1),
    )
    .expect("supervisor");
    let handle = supervisor.handle();
    let task = tokio::spawn(supervisor.run());
    wait_until_ready(&handle, 64).await;
    fail.notify_one();
    for _ in 0..16 {
        tokio::task::yield_now().await;
    }
    tokio::time::advance(Duration::from_secs(1)).await;
    let result = task.await.expect("supervisor joined");
    let Err(SupervisorError::EssentialServiceFailed {
        completion, report, ..
    }) = result
    else {
        panic!("essential failure must fail the graph");
    };
    assert!(matches!(completion, ServiceCompletion::Failed(_)));
    assert_eq!(
        report.outcome(),
        i2pr_runtime::ShutdownOutcome::PartiallyForced
    );
    assert_eq!(report.remaining_tasks(), 0);
    assert!(
        report
            .forced_services()
            .iter()
            .any(|service| service.as_str() == "reporter")
    );
    assert!(!format!("{completion:?}").contains("bounded internal failure"));
    assert_eq!(handle.snapshot().owned_service_tasks, 0);
}

fn fault_replay(seed: ReproducibilitySeed) -> i2pr_testkit::ReplayRecord {
    let clock = ManualClock::new();
    let scheduler = NetworkScheduler::new(clock, SchedulerConfig::default()).expect("scheduler");
    let stream_faults = FaultScript::new(
        seed,
        vec![
            FaultRule::new(
                1,
                FaultMatcher::any().kind(FaultUnitKind::Stream),
                FaultAction::Delay(Duration::from_millis(1)),
            ),
            FaultRule::new(
                2,
                FaultMatcher::any().kind(FaultUnitKind::Stream).sequence(0),
                FaultAction::duplicate(1).expect("duplicate bound"),
            ),
            FaultRule::new(
                3,
                FaultMatcher::any().kind(FaultUnitKind::Stream),
                FaultAction::reorder(2).expect("reorder bound"),
            ),
        ],
    )
    .expect("stream fault script");
    let stream = scheduler
        .stream_link(
            LinkId::new(1).expect("link"),
            StreamConfig::new(32, 4).expect("stream config"),
            stream_faults,
        )
        .expect("stream link");
    let stream_left = stream.left();
    let stream_right = stream.right();
    stream_left.try_write(b"abcd").expect("stream segment");
    stream_left.try_write(b"efgh").expect("stream segment");
    scheduler
        .advance(Duration::from_millis(1))
        .expect("advance stream");
    let mut output = [0_u8; 8];
    let _ = stream_right.try_read(&mut output).expect("stream read");

    let datagram_faults = FaultScript::new(
        seed,
        vec![
            FaultRule::new(
                4,
                FaultMatcher::any()
                    .kind(FaultUnitKind::Datagram)
                    .sequence(0),
                FaultAction::truncate(2).expect("truncate bound"),
            ),
            FaultRule::new(
                5,
                FaultMatcher::any()
                    .kind(FaultUnitKind::Datagram)
                    .sequence(1),
                FaultAction::Drop,
            ),
        ],
    )
    .expect("datagram fault script");
    let datagram = scheduler
        .datagram_link(
            LinkId::new(2).expect("link"),
            DatagramConfig::new(16, 4).expect("datagram config"),
            datagram_faults,
        )
        .expect("datagram link");
    let datagram_left = datagram.left();
    let datagram_right = datagram.right();
    datagram_left.try_send(b"one").expect("datagram");
    datagram_left.try_send(b"two").expect("datagram");
    scheduler
        .advance(Duration::ZERO)
        .expect("advance datagrams");
    let packet = datagram_right
        .try_recv()
        .expect("datagram receive")
        .expect("truncated packet");
    assert_eq!(packet.payload, b"on");
    assert!(datagram_right.try_recv().expect("drop receive").is_none());

    let reset_faults = FaultScript::new(
        seed,
        vec![FaultRule::new(
            6,
            FaultMatcher::any()
                .kind(FaultUnitKind::Datagram)
                .direction(LinkDirection::AtoB),
            FaultAction::Reset,
        )],
    )
    .expect("reset fault script");
    let reset_link = scheduler
        .datagram_link(
            LinkId::new(3).expect("link"),
            DatagramConfig::default(),
            reset_faults,
        )
        .expect("reset link");
    reset_link.left().try_send(b"reset").expect("reset packet");
    scheduler.advance(Duration::ZERO).expect("advance reset");

    let disconnect_faults = FaultScript::new(
        seed,
        vec![FaultRule::new(
            7,
            FaultMatcher::any().kind(FaultUnitKind::Stream),
            FaultAction::Disconnect,
        )],
    )
    .expect("disconnect fault script");
    let disconnect_link = scheduler
        .stream_link(
            LinkId::new(4).expect("link"),
            StreamConfig::default(),
            disconnect_faults,
        )
        .expect("disconnect link");
    disconnect_link
        .left()
        .try_write(b"eof")
        .expect("disconnect segment");
    scheduler
        .advance(Duration::ZERO)
        .expect("advance disconnect");

    drop(stream_left);
    drop(stream_right);
    drop(stream);
    drop(datagram_left);
    drop(datagram_right);
    drop(datagram);
    drop(reset_link);
    drop(disconnect_link);
    scheduler.close();
    let replay = scheduler.replay(seed, "simulated-link-faults", 8);
    assert_eq!(replay.snapshot.pending_deliveries, 0);
    assert_eq!(replay.snapshot.buffered_bytes, 0);
    assert_eq!(replay.snapshot.stream_links, 0);
    assert_eq!(replay.snapshot.datagram_links, 0);
    replay
}

#[test]
fn scenario_simulated_link_faults_replay_identically() {
    let seed = ReproducibilitySeed::from_u128(0x024);
    let first = fault_replay(seed);
    let second = fault_replay(seed);
    assert_eq!(first, second);
    assert!(first.events.iter().any(|event| event.rules.contains(&2)));
    assert!(first.events.iter().any(|event| event.rules.contains(&4)));
    assert!(first.events.iter().any(|event| event.rules.contains(&5)));
    assert!(first.events.iter().any(|event| event.rules.contains(&6)));
    assert!(first.events.iter().any(|event| event.rules.contains(&7)));
}

#[test]
fn scenario_fixed_32_seed_soak_matrix_is_reproducible() {
    for seed_value in 0_u128..32 {
        let seed = ReproducibilitySeed::from_u128(seed_value);
        assert_eq!(fault_replay(seed), fault_replay(seed));
    }
}

#[test]
fn scenario_capacity_and_resource_boundaries_are_explicit() {
    assert!(ResourceLimit::new(ResourceClass::CommandQueueItems, 0).is_err());
    for capacity in [1_usize, 2, 4] {
        let budget = command_budget(capacity as u64);
        let spec = ChannelSpec::command("boundary.command", name("coordinator"), capacity)
            .expect("channel spec")
            .with_item_charge(ResourceClass::CommandQueueItems, 1)
            .expect("charge")
            .with_budget(budget.clone());
        let (sender, receiver) = command_channel::<u8>(spec).expect("channel");
        for value in 0..capacity {
            sender.try_send(value as u8).expect("within capacity");
        }
        assert!(matches!(
            sender.try_send(255),
            Err(i2pr_runtime::SendError::Full(_))
        ));
        assert_eq!(sender.snapshot().queued, capacity);
        drop(receiver);
        assert_eq!(
            budget.usage(ResourceClass::CommandQueueItems).unwrap().used,
            0
        );
    }
}
