# `i2pr-runtime` — Deep Dive

Crate: **`i2pr-runtime`** · Path: `crates/i2pr-runtime/` · ~19.5k lines (15 modules + `lib.rs`)

**Purpose:** the sole production owner of Tokio in the workspace — it owns
sockets, timers, bounded channels, tasks, and wakeable cancellation, and
fulfills the actions emitted by the runtime-neutral transport crates.

Path: `crates/i2pr-runtime/`

## Purpose

`i2pr-runtime` is the seam between the runtime-neutral crates and the rest of
the world. It is where:

- Tokio tasks, `tokio::net` sockets, and `tokio::time` timers are allowed and
  owned.
- Supervision trees run (topological ordering, restart policy, graceful/forced
  shutdown) via `supervisor`.
- Wakeable cancellation is implemented with first-reason-wins semantics and a
  parent-chain reason walk via `cancel`.
- Bounded service channels are built (command, event, request, latest-state)
  with resource charging via `channel`.
- A process-shared, bounded byte-rate governor is provided by `bandwidth` and
  injected by the daemon into the SSU2 socket owner. Its inbound limit applies
  before packet processing; outbound bytes are reserved immediately before
  `send_to`. It is disabled unless explicitly configured. The current SSU2
  adapter has bounded per-peer waiters and FIFO pacing per direction, but does
  not yet provide traffic-class priority or qualify an advertised capacity.
- TCP listeners and NTCP2 link children are owned via `ntcp2_runtime`.
- UDP sockets and SSU2 session children are owned via `ssu2_runtime`
  (Plans 158–160).
- NTCP2 handshake actions are fulfilled by `ntcp2_driver`; authenticated
  data-frame links by `ntcp2_link`.
- Privacy-safe aggregate snapshots are produced by `observability`.

What it must **not** own:

- Protocol semantics. `i2pr-transport`, `i2pr-transport-ntcp2`, and
  `i2pr-transport-ssu2` are runtime-neutral: no Tokio, no sockets, no
  filesystem, no `async fn`/`async_trait`, and no dependence on routing
  clients. The runtime executes their actions; it does not define them.
- Composition. `i2pr-daemon` is the composition root and owns the SAM/I2CP/
  service-tunnel listeners. This crate supplies the services those listeners
  are built from.

This boundary is enforced, not merely documented:
`scripts/check-runtime-boundaries.sh` fails the build if `i2pr-transport`,
`i2pr-transport-ntcp2`, or `i2pr-transport-ssu2` match
`tokio::|std::net|std::fs|TcpStream|TcpListener|UdpSocket|UnixStream|OpenOptions|File::`
or if their manifests name `i2pr-runtime` / `i2pr-daemon` / `i2pr-testkit`. The
same script allows `tokio`/`tokio-util` manifests only in `i2pr-runtime` and
`i2pr-testkit`, which is exactly the positive statement that this crate is
the production runtime owner. ADR 0002 records that decision.

## Module layout

Line counts are `wc -l` on `crates/i2pr-runtime/src/*.rs` (total 21,126).

| Module | File | Lines | Responsibility | Key public types |
| --- | --- | --- | --- | --- |
| `lib` | `src/lib.rs` | 149 | Crate root: module declarations, all `pub use` re-exports, `run_blocking`/`bounded_timeout` | `#![forbid(unsafe_code)]`, `run_blocking`, `bounded_timeout` |
| `cancel` | `src/cancel.rs` | 169 | Hierarchical wakeable cancellation with first-reason-wins and parent-chain reason walking over `tokio_util::CancellationToken` | `CancellationToken` |
| `channel` | `src/channel.rs` | 1908 | Typed bounded service channels (command/event/request/latest-state) with resource charging, overflow policies, privacy-safe counters | `ChannelSpec`, `ChannelName`, `CommunicationClass`, `OverflowPolicy`, `QueueCharge`, `RequestChannelParts`, `LatestState`, `*Sender`/`*Receiver`, `Received`, `ReceivedRequest`, `ChannelSnapshot`, all error types |
| `bandwidth` | `src/bandwidth.rs` | 558 | Shared cancellable inbound/outbound byte token buckets with bounded FIFO waiter queues, independent direction scheduling and redacted peer keys | `BandwidthGovernor`, `BandwidthGovernorConfig`, `BandwidthDirection`, `BandwidthPeerKey`, `BandwidthGovernorSnapshot` |
| `context` | `src/context.rs` | 692 | Per-service context bundle, readiness, health publication, `ChildScope` with bounded join/forced abort, internal `RuntimeClock` | `ServiceContext`, `Readiness`, `HealthReporter`, `HealthReceiver`, `ChildScope`, `ChildFailurePolicy`, `ChildTaskFailure`, `ChildScopeError`, `ChildShutdownReport` |
| `graph` | `src/graph.rs` | 648 | Service registration, deterministic topological ordering, full-graph validation before startup, restart policy | `ServiceGraph`, `ServiceGraphBuilder`, `ServiceSpec`, `ServiceFuture`, `ServiceResult`, `RestartPolicy`, `RestartExhaustion`, `RestartPolicyError`, `GraphError` |
| `ntcp2_data_oracle` | `src/ntcp2_data_oracle.rs` | 359 | Bounded data-phase receive oracle: one absolute deadline plus cumulative frame/byte/block/non-target-I2NP bounds, strict RouterInfo signature validation, exact DeliveryStatus correlation. Never wired into the production service graph | `OracleConfig`, `OracleCounters`, `OracleAccept`, `MatchedTarget`, `PeerRouterHashBinding`, `DataOracleError` (re-exported also as `DataOracle`) |
| `ntcp2_driver` | `src/ntcp2_driver.rs` | 1220 | Runtime-owned handshake action executor: exact reads/writes, bounded deadlines, cancellation, replay admission, clock, padding, RouterInfo handoff | `HandshakeDriverConfig`, `HandshakeDriverError`, `HandshakeRun`, `HandshakeRunOutcome`, `HandshakeClock`, `HandshakeCounterSnapshot`, `PaddingProfile`, `drive_initiator_handshake`, `drive_responder_handshake`, `*_observed` variants |
| `ntcp2_handshake_observer` | `src/ntcp2_handshake_observer.rs` | 140 | Privacy-safe handshake progress observer trait. Records bounded stage name, octet counts, typed I/O result, elapsed ms — never raw payload, keys, Noise state, transcript, ciphertext, or RouterInfo bytes. Synchronous, infallible, non-blocking | `HandshakeProgressObserver` (trait), `HandshakeStageObservation`, `HandshakeIoResult`, `NoopHandshakeObserver` |
| `ntcp2_link` | `src/ntcp2_link.rs` | 605 | Authenticated frame reader/writer children and per-frame inbound/outbound queue accounting leases | `AuthenticatedLink`, `AuthenticatedLinkError`, `AuthenticatedLinkStartError`, `AuthenticatedLinkSnapshot`, `ReceivedFrameLease` |
| `ntcp2_runtime` | `src/ntcp2_runtime.rs` | 2357 | Bounded NTCP2 socket/link lifecycle, TCP listener ownership, global/IP/subnet admission, replay cache, dial backoff, link children, exact I/O helpers | `Ntcp2RuntimeService`, `BoundNtcp2Listener`, `ListenerHandle`, `LinkHandle`, `InboundAdmission`, `InboundPermit`, `ActiveLinkAdmission`, `ActiveLinkPermit`, `ReplayCache`, `DialAdmission`, `DialKey`, `IpPrefixPolicy`, `AdmittedInboundStream`, `AuthenticatedInboundStream`, `ExactIoError`, `read_exact`, `write_all_exact` |
| `observability` | `src/observability.rs` | 360 | Privacy-aware tracing events, bounded aggregate snapshots, shared `pub(crate) TaskCounters` | `RuntimeSnapshot`, `SupervisorSnapshot`, `ServiceSnapshot`, `SimulationSnapshot`, `RouterLifecycle`, `SnapshotError`, `event` |
| `ssu2_controlled_peer_test` | `src/ssu2_controlled_peer_test.rs` | 767 | Bounded driver for the wire-real 7-message peer-test exchange; spawns ephemeral helper loop tasks under a caller-owned `ChildScope` and records nothing itself — outcomes return to the caller | `run_controlled_peer_test`, `ControlledPeerTestParams`, `ControlledPeerTestOutcome`, `ControlledPeerTestSigner` |
| `ssu2_peer_relay` | `src/ssu2_peer_relay.rs` | 1191 | Plan 160 bounded peer-test/relay coordination: table ownership, per-source rate limits, bounded signer registry, central expiry scheduler, reachability mirroring, privacy-safe snapshots | `Ssu2PeerRelayService`, `Ssu2PeerRelayConfig`, `Ssu2PeerRelaySnapshot`, `PeerRelayAdmission` |
| `ssu2_runtime` | `src/ssu2_runtime.rs` | 7310 | Plan 158 bounded SSU2 UDP socket/session lifecycle, Plan 159 path validation/migration and conservative reachability observations, Plan 282 publication material, Plan 283 controlled peer test, optional process-shared byte admission | `Ssu2RuntimeService`, `Ssu2ServiceHandle`, `Ssu2RuntimeConfig`, `Ssu2RuntimeLimits`, `Ssu2RuntimeDeadlines`, `Ssu2SocketConfig`, `Ssu2DialTarget`, `Ssu2LinkHandle`, `Ssu2EstablishedLink`, `Ssu2InboundI2np`, `Ssu2Snapshot`, `Ssu2IdentityMaterial`, `Ssu2NetworkCondition`, `Ssu2PublicationMaterial`, `Ssu2PublicationUnavailable`, `Ssu2TestFaults`, `Ssu2BindError`, `ControlledPeerTestError` |
| `supervisor` | `src/supervisor.rs` | 1703 | Service startup sequencing, health tracking, `JoinSet`-managed restart with bounded exponential backoff, graceful/forced shutdown | `Supervisor`, `SupervisorHandle`, `SupervisorError`, `SupervisorConfigError`, `ShutdownReport`, `ShutdownOutcome` |

`ssu2_runtime` is by far the largest module (7,235 lines, ~37% of the crate),
followed by `ntcp2_runtime` (2,357) and `channel` (1,908).

## Public surface

Re-exports from `lib.rs`. Modules are private (`mod x;`), so this list is the
complete external surface; `context::RuntimeClock` and `observability::TaskCounters`
are `pub(crate)` and are **not** part of it.

- `cancel`: `CancellationToken`
- `bandwidth`: `BandwidthAcquireError`, `BandwidthDirection`,
  `BandwidthGovernor`, `BandwidthGovernorConfig`,
  `BandwidthGovernorConfigError`, `BandwidthGovernorSnapshot`,
  `BandwidthPeerKey`, and hard burst/rate/queue ceilings
- `channel`: `ChannelConfigError`, `ChannelName`, `ChannelNameError`,
  `ChannelSnapshot`, `ChannelSpec`, `CommunicationClass`, `EventReceiver`,
  `EventSendError`, `EventSender`, `LatestState`, `LatestStateReceiver`,
  `LatestStateSender`, `MAX_CHANNEL_CAPACITY`, `MAX_CHANNEL_NAME_BYTES`,
  `MAX_QUEUE_ITEM_BYTES`, `OverflowPolicy`, `QueueCharge`, `ReceiveError`,
  `Received`, `ReceivedRequest`, `RequestChannelParts`, `RequestError`,
  `RequestReceiver`, `RequestSender`, `SendError`, `StateUpdateError`,
  `TryReceiveError`, `command_channel`, `event_channel`, `latest_state_channel`,
  `request_channel`
- `context`: `ChildFailurePolicy`, `ChildScope`, `ChildScopeError`,
  `ChildShutdownReport`, `ChildTaskFailure`, `HealthReceiver`, `HealthReporter`,
  `MAX_CHILD_TASKS`, `Readiness`, `ReadinessError`, `ServiceContext`
- `graph`: `GraphError`, `MAX_RESTART_ATTEMPTS`, `MAX_SERVICE_COUNT`,
  `MAX_SERVICE_TIMEOUT`, `RestartExhaustion`, `RestartPolicy`,
  `RestartPolicyError`, `ServiceFuture`, `ServiceGraph`, `ServiceGraphBuilder`,
  `ServiceResult`, `ServiceSpec`
- `ntcp2_data_oracle`: `BOUNDED_STEP_BUDGET`, `DataOracleError`,
  `DataOracleError as DataOracle`, `MatchedTarget`, `ORACLE_MAX_BLOCKS`,
  `ORACLE_MAX_FRAMES`, `ORACLE_MAX_NON_TARGET_I2NP`,
  `ORACLE_MAX_PLAINTEXT_BYTES`, `ORACLE_SCHEMA`, `OracleAccept`,
  `OracleConfig`, `OracleCounters`, `PeerRouterHashBinding`,
  `bounded_deadline_step`, `drop_lease as drop_received_frame_lease`,
  `receive_correlated_delivery_status`
- `ntcp2_driver`: `HandshakeClock`, `HandshakeCounterSnapshot`,
  `HandshakeDriverConfig`, `HandshakeDriverError`, `HandshakeRun`,
  `HandshakeRunOutcome`, `PaddingProfile`, `drive_initiator_handshake`,
  `drive_initiator_handshake_observed`, `drive_responder_handshake`,
  `drive_responder_handshake_observed`
- `ntcp2_handshake_observer`: `HandshakeIoResult`, `HandshakeProgressObserver`,
  `HandshakeStageObservation`, `NoopHandshakeObserver`
- `ntcp2_link`: `AuthenticatedLink`, `AuthenticatedLinkError`,
  `AuthenticatedLinkSnapshot`, `AuthenticatedLinkStartError`,
  `ReceivedFrameLease`
- `ntcp2_runtime`: `ActiveLinkAdmission`, `ActiveLinkAdmissionError`,
  `ActiveLinkPermit`, `ActiveLinkSnapshot`, `AddressFamily`, `AdmissionDenied`,
  `AdmissionRejection`, `AdmissionSnapshot`, `AdmittedInboundStream`,
  `AuthenticatedInboundStream`, `BoundNtcp2Listener`, `DialAdmission`,
  `DialAttempt`, `DialBackoffConfig`, `DialBackoffDecision`,
  `DialBackoffSnapshot`, `DialKey`, `DialKeyError`, `DialOutcome`,
  `ExactIoError`, `InboundAdmission`, `InboundChunk`, `InboundPermit`,
  `IoErrorKind`, `IpPrefixPolicy`, `LinkHandle`, `LinkId`, `LinkSendError`,
  `LinkSnapshot`, `LinkStartError`, `LinkTermination`, `ListenerHandle`,
  `ListenerSnapshot`, `Ntcp2Deadline`, `Ntcp2DeadlineError`, `Ntcp2Event`,
  `Ntcp2EventKind`, `Ntcp2RuntimeConfig`, `Ntcp2RuntimeConfigError`,
  `Ntcp2RuntimeDeadlines`, `Ntcp2RuntimeLimits`, `Ntcp2RuntimeService`,
  `ReplayCache`, `ReplayCacheDecision`, `ReplayCacheSnapshot`,
  `RuntimeLimitKind`, `WriteOutcome`, `read_exact`, `write_all_exact`
- `observability`: `MAX_SNAPSHOT_CHANNELS`, `MAX_SNAPSHOT_RESOURCES`,
  `RouterLifecycle`, `RuntimeSnapshot`, `ServiceSnapshot`, `SimulationSnapshot`,
  `SnapshotError`, `SupervisorSnapshot`, `event`
- `ssu2_controlled_peer_test`: `ControlledPeerTestOutcome`,
  `ControlledPeerTestParams`, `ControlledPeerTestSigner`,
  `run_controlled_peer_test`
- `ssu2_peer_relay`: `PEER_RELAY_DATAGRAMS_PER_SECOND`, `PEER_RELAY_MAX_SIGNERS`,
  `PEER_RELAY_RATE_SOURCES`, `PEER_RELAY_RESPONSE_BUDGET_NUMERATOR`,
  `PeerRelayAdmission`, `Ssu2PeerRelayConfig`, `Ssu2PeerRelayService`,
  `Ssu2PeerRelaySnapshot`
- `ssu2_runtime`: `MAX_SSU2_ACTIVE_CEILING`, `MAX_SSU2_INBOUND_QUEUE_CEILING`,
  `MAX_SSU2_PENDING_CEILING`, `MAX_SSU2_RUNTIME_DURATION`,
  `MAX_SSU2_STAGED_BYTES_CEILING`, `MAX_SSU2_STAGED_DATAGRAMS_CEILING`,
  `SSU2_BACKOFF_ENTRIES`, `SSU2_CACHED_TOKEN_GRACE`, `SSU2_CONFIRMED_MTU_PAYLOAD`,
  `SSU2_DEFAULT_MTU`, `SSU2_LINKS_PER_PEER`, `SSU2_MANAGER_BYTES_PER_LINK`,
  `SSU2_MANAGER_MESSAGES_PER_LINK`, `SSU2_MAX_DRAIN_PER_SESSION`,
  `SSU2_MAX_PATH_CANDIDATES_GLOBAL`, `SSU2_SEND_QUEUE_BYTES`,
  `SSU2_SEND_QUEUE_MESSAGES`, `SSU2_TOKEN_CACHE_PEERS`,
  `SSU2_TOKEN_CACHE_PER_PEER`, `SSU2_TOKEN_REQUEST_SOURCES`,
  `SSU2_TOKEN_REQUESTS_PER_SECOND`, `ControlledPeerTestError`, `Ssu2BindError`,
  `Ssu2DialOutcome`, `Ssu2DialTarget`, `Ssu2DialTargetError`,
  `Ssu2EstablishedLink`, `Ssu2IdentityMaterial`, `Ssu2InboundI2np`,
  `Ssu2LimitKind`, `Ssu2LinkHandle`, `Ssu2NetworkCondition`,
  `Ssu2PublicationMaterial`, `Ssu2PublicationUnavailable`, `Ssu2RuntimeConfig`,
  `Ssu2RuntimeConfigError`, `Ssu2RuntimeDeadlines`, `Ssu2RuntimeLimits`,
  `Ssu2RuntimeService`, `Ssu2SendOutcome`, `Ssu2ServiceHandle`, `Ssu2Snapshot`,
  `Ssu2SocketConfig`, `Ssu2TestFaults`
- `supervisor`: `MAX_SHUTDOWN_DEADLINE`, `ShutdownOutcome`, `ShutdownReport`,
  `Supervisor`, `SupervisorConfigError`, `SupervisorError`, `SupervisorHandle`
- Crate-root free functions: `run_blocking` (builds a disposable
  current-thread runtime for non-production launchers such as
  `tools/i2pr-interop`), `bounded_timeout`
- Re-exported from `i2pr-transport-ssu2` so `i2pr-daemon` composes controlled
  identity/address material without depending on the transport crate directly
  (Plan 184; `pq` KEM-scheme metadata added in Plan 197): `IntroKey`,
  `PqCapabilities`, `Ssu2PqKem`, `Ssu2PublicKey`, `Ssu2RouterAddress`,
  `constants`
- Re-exported from `i2pr-core`: `CancellationReason`, `DegradationCode`,
  `FailureCategory`, `HealthDetail`, `HealthSnapshot`, `HealthState`,
  `InvalidLifecycleTransition`, `LifecycleState`, `ServiceClassification`,
  `ServiceCompletion`, `ServiceFailure`, `ServiceFailureCategory`, `ServiceName`,
  `ServiceNameError`, `ShutdownReason`

## Key contracts

### `ServiceGraph` — topological validation (`graph.rs`)

`ServiceGraphBuilder::new(maximum)` rejects a zero or excessive maximum
(`GraphError::InvalidMaximum`). `register()` accumulates `ServiceSpec`s;
`build()` validates the **whole** graph before any service starts, so an
invalid graph is never partially started. `startup_order()` returns a
deterministic (lexically ordered) topological sequence.

`GraphError` variants, all verified against `graph.rs`:

- `InvalidMaximum { maximum }` — router-wide service maximum invalid.
- `DuplicateService { name }` — identifier registered twice.
- `TooManyServices { maximum }` — exceeded the explicit count maximum.
- `MissingEssentialService` — an essential service was required but none registered.
- `MissingDependency { service, dependency }` — a declared dependency was never
  registered. **The graph fails to build.**
- `SelfDependency { service }` — a service depends on itself. Fails to build.
- `DependencyCycle { services }` — cycle. Fails to build; the cycle members are
  named in the error.
- `InvalidTimeout { service, field }` — a timeout was zero or above the maximum.
- `InvalidRestartPolicy { service, classification }` — a restartable service has
  no valid policy.
- `ContradictoryStartupRequirement { service, classification }` — a service was
  declared `StartupRequirement::Optional` while classified `Essential`. The two
  statements contradict each other, so the graph refuses to resolve it silently.
- `DescriptionTooLong { service }` — static diagnostic text exceeded the bound.

A cycle or a missing dependency is therefore a **construction-time**
(`build()`) failure, not a startup-time surprise. `MAX_SERVICE_COUNT = 128`,
`MAX_SERVICE_TIMEOUT = 3_600 s`, `MAX_RESTART_ATTEMPTS = 32`. Internal
(non-exported) defaults: `DEFAULT_STARTUP_TIMEOUT = 30 s`,
`DEFAULT_READINESS_TIMEOUT = 30 s`, `DEFAULT_SHUTDOWN_GRACE = 5 s`.

`RestartPolicy::new` validates through `RestartPolicyError`:
`InvalidAttemptCount { maximum }`, `ZeroDelay` (would permit a hot loop),
`MaximumBeforeInitial`, `DelayTooLong { maximum }`.
`RestartExhaustion` is `Degrade` (keep running, publish degraded) or `Shutdown`
(cancel the graph and fail the router).

### Startup requirement (Plan 371)

`ServiceClassification` describes how a service behaves **once the router is
running**. `StartupRequirement` is a separate, orthogonal policy describing
whether the router **needs it to become ready before startup completes**:

- `StartupRequirement::Required` — the default. A service that never signals
  readiness fails router startup exactly as before.
- `StartupRequirement::Optional` — opt-in. A service that never signals
  readiness is recorded as `Degraded`/`LocalPolicy`, startup continues, and the
  service is excluded from `SupervisorSnapshot::ready`.

Two independent policies can therefore make a startup failure non-fatal, and
both are decided at registration time rather than inferred from the failure:

1. `StartupRequirement::Optional`, for a service that may be unusable from the
   start (a manager whose handshake never completes, an unresolvable executable);
2. `RestartExhaustion::Degrade` on a `Restartable` service, which is honoured
   **during startup as well as after**. Before Plan 371 the budget was only
   consulted in the steady-state handler, so a restartable service that broke in
   its first seconds took down a router that would have survived the identical
   failure a minute later.

Degrading a service and releasing the router's readiness are separate decisions:
a `Restartable` service that degraded still gates `ready` unless it *also* opts
into `StartupRequirement::Optional`. Plan 369's `app-runtime` service carries
both, because those are exactly the two distinct failures it must survive.

Two further semantics are load-bearing and pinned by tests:

- **A dependency edge constrains start order, not availability.** Once a
  dependency has degraded at startup, its dependents are still given their own
  attempt rather than being failed on its behalf with
  `DependencyUnavailable`. Without this, `Optional` would only work for leaf
  services and any real optional subsystem — which always has dependants —
  would still take the router down.
- **`SupervisorSnapshot::ready` reports operator-usable truth.** It is computed
  over services whose `startup_requirement` gates readiness *and* whose
  classification is `Essential` or `Restartable`. `RouterLifecycle::Ready` and
  `ready` are therefore consistent: an optional feature that never started does
  not leave a usable router permanently reported as unready.

### Supervisor (`supervisor.rs`)

`Supervisor::new(graph, shutdown_deadline)` → `.run() -> Result<ShutdownReport, SupervisorError>`;
`SupervisorHandle` is returned to callers via `.handle()` and offers
`shutdown(reason)`, `is_shutdown_requested()`, `snapshot()`, `runtime_snapshot(..)`,
`health(service)`, plus `forced_services()`, `completion(..)`, `completions()`.

One manager task per service, spawned onto a single
`JoinSet<ManagerOutput>` (`tasks.spawn(..)`, `supervisor.rs:575`). The manager
body is wrapped in `AssertUnwindSafe(..).catch_unwind()`, so a **panic** is
classified as `ServiceCompletion::Panic` and never propagates a payload.

Per-attempt outcome handling in `run_manager`:

- **Cancellation / requested shutdown** → returns immediately with
  `ServiceCompletion::RequestedShutdown`; no restart is attempted.
- **Non-restartable, or a non-failure completion** → returns the completion.
- **Restartable and failed, budget remaining** → `restart_count += 1`, sleep
  `policy.delay_for(restart_count)` (bounded exponential) in a `select!` against
  `token.cancelled()` so a shutdown during backoff returns immediately rather
  than waiting out the delay. If the attempt had been continuously ready and
  `reset_after_ready` is set, the counter resets to zero.
- **Budget exhausted** → `ServiceCompletion::RestartBudgetExhausted`; health
  becomes `Degraded`/`LocalPolicy` under `RestartExhaustion::Degrade`, or
  `Failed` under `RestartExhaustion::Shutdown`.

`SupervisorError`: `StartupFailed { service, completion, report }`,
`EssentialServiceFailed { .. }`, `RestartBudgetExhausted { service, report }`.
`SupervisorConfigError::InvalidShutdownDeadline` for a zero/excessive deadline.
`MAX_SHUTDOWN_DEADLINE == MAX_SERVICE_TIMEOUT` (3,600 s).

Graceful shutdown races a `tokio::time::sleep(graceful_period)` deadline
against `tasks.join_next()`. On expiry it records every still-active service as
`ServiceCompletion::ForcedAbort`, calls `tasks.abort_all()`, and drains the
`JoinSet`. Outcome is `ShutdownOutcome::Graceful` (nothing forced),
`PartiallyForced`, or `FailedCleanup`.

### `ServiceContext` and `ChildScope` (`context.rs`)

`ServiceContext` narrows the API a service receives to exactly five fields:
`name: ServiceName`, `cancellation: CancellationToken`, `readiness: Readiness`,
`health: HealthReporter`, `children: ChildScope`. A service never gets a handle
to the supervisor. Accessors: `name()`, `cancellation()`, `readiness()`,
`signal_ready()`, `health()`, `children()`.

`ChildScope::spawn(factory)` registers a child in the scope's internal
`AsyncMutex<Option<JoinSet<ChildTaskOutput>>>`; the `JoinSet` is the explicit
owner — there is no raw `JoinHandle` anywhere in the crate (the boundary script
bans the identifier outright). Each child receives its own child
`CancellationToken`, so cancelling the service cascades hierarchically
(ADR 0008). Panics are caught with `AssertUnwindSafe + catch_unwind`
(`context.rs:470`) and recorded as `ChildTaskFailure::Panic` under the scope's
`ChildFailurePolicy`.

`ChildScope::shutdown()` is the public graceful path returning a
`ChildShutdownReport`; `force_shutdown()` is `pub(crate)` — the supervisor's
internal abort path, which aborts all children and drains with a bounded poll
budget of `0..=MAX_CHILD_TASKS` iterations interleaved with `yield_now()`
(`context.rs:522`) so a non-cooperative child cannot extend shutdown
indefinitely. Finished children are reaped so that `MAX_CHILD_TASKS = 64` bounds
*live* tasks, not lifetime.

`ChildScopeError`: `Closed` (scope begun shutdown, accepts no new work),
`TooManyTasks { maximum }`. `ReadinessError`: `Duplicate`, `Closed`.
`ChildTaskFailure`: `Explicit`, `Panic`.

Health publication is backed by `tokio::sync::watch<HealthSnapshot>` — latest
state only, no unbounded history — with a monotonic `transition_sequence`.
`HealthReporter::report/ready/degraded`, `HealthReceiver::snapshot/changed`.

### Bounded channels (`channel.rs`)

Four paradigms, all bounded, all backed by `tokio::sync::mpsc` / `oneshot` /
`watch`:

| Constructor | Shape | Capacity source |
| --- | --- | --- |
| `command_channel(spec)` | multi-producer, ordered, resource-charged | `ChannelSpec.capacity`, `<= MAX_CHANNEL_CAPACITY` |
| `event_channel(spec)` | single consumer, drop-newest | `ChannelSpec.capacity` |
| `request_channel(spec)` | request/response with per-request deadline | `ChannelSpec.capacity` + oneshot reply |
| `latest_state_channel(spec)` | `watch`, latest value only | `ChannelSpec.capacity` |

`MAX_CHANNEL_CAPACITY = 4_096`, `MAX_CHANNEL_NAME_BYTES = 64`,
`MAX_QUEUE_ITEM_BYTES = 1 << 20`.

Every send is `try_send` or `send_until`. The ordering matters:
`send_until` (`channel.rs:769`) selects on
`{ cancellation.cancelled(), tokio::time::sleep_until(deadline), sender.reserve_owned() }`
with `biased;` cancellation first, and only **after** winning a queue permit
does it call `state.admit(estimate)` to take the resource lease. A blocked
sender therefore never holds a resource lease while waiting for a queue slot.
Both a `receiver_closed` re-check and a permit-drop-on-failure path exist, so a
send that loses the race releases cleanly.

Backpressure/close semantics: `try_send` returns
`SendError::Full` (bounded queue full, nothing consumed) or
`SendError::Closed`; `send_until` adds `DeadlineElapsed` and `Cancelled`. Every
terminal path increments a privacy-safe counter and emits
`event::CHANNEL_REJECTED` / `event::RESOURCE_DENIED`. `SendError<T>` variants:
`Full`, `DeadlineElapsed`, `Cancelled`, `Closed`, `ResourceDenied`,
`PayloadEstimateRequired`, `PayloadEstimateTooLarge`, `PayloadEstimateOutOfRange`
(each carrying the value where a return is possible). `ReceiveError`:
`Cancelled`, `DeadlineElapsed`, `Closed`. `TryReceiveError`: `Empty`, `Closed`.
`RequestError` adds `ResponseClosed`, `ResponseCancelled`,
`ResponseDeadlineElapsed`. `ChannelConfigError`: `InvalidName(ChannelNameError)`,
`ZeroCapacity`, `CapacityTooLarge`, `InvalidOverflowPolicy`,
`LatestStateCapacity`, `ZeroCharge`, `ItemEstimateTooLarge`, `MissingBudget`,
`WrongCommunicationClass`. `ChannelNameError`: `Empty`,
`TooLong { maximum }`, `InvalidCharacter`.

Resource charging: `QueueCharge::PerItem` or `PerBytes`, validated against a
budget before admission. `Received<T>` / `ReceivedRequest` own their lease — the
charge lives as long as the queue entry, and dropping the owner releases it.

### Cancellation (`cancel.rs`)

`CancellationToken::new()`, `.child_token()`, `.cancel(reason) -> bool`,
`.is_cancelled()`, `.reason()`, `.cancelled().await`,
`.cancelled_reason().await`. Wraps `tokio_util::CancellationToken`.

First-reason-wins: `cancel()` returns `true` only for the caller that recorded
the reason, and returns `false` if a reason is already recorded **or** an
inherited parent reason exists — a child cannot replace the reason it inherited.
`reason()` returns the local reason, else walks `parent_reason()` recursively up
the chain to the root (`cancel.rs:85-98`).

### Timers and the manual clock

The crate takes an `Arc<RuntimeClock>` (`context.rs:24`, `pub(crate)`) rather
than calling `tokio::time::Instant::now()` inline, so the supervisor, the
orchestrator, and the snapshot layer read one clock. Deadlines are always
explicit and bounded: `Ntcp2Deadline::after(dur)` returns
`Ntcp2DeadlineError::Zero` / `TooLong` rather than accepting a bad bound.

`#[tokio::test(start_paused = true)]` works because time is only ever reached
through Tokio's time source (`tokio::time::sleep`, `sleep_until`, `timeout`) —
there are no `std::time` sleeps, and `check-runtime-boundaries.sh` bans
`std::thread::sleep`, `thread::sleep`, `std::mem::forget`, and `mem::forget`
across `i2pr-runtime` and `i2pr-testkit`. Real-socket suites that need actual
elapsed time use explicit bounded `tokio::time::sleep` waits and never paused
time.

### NTCP2 admission, replay, and dial backoff (`ntcp2_runtime.rs`)

- `InboundAdmission::admit(address)` applies global, per-IP, and per-subnet
  limits; `InboundPermit` is the RAII guard released on drop.
  `AdmissionRejection`: `GlobalLimit`, `IpLimit`, `SubnetLimit`, `StatePoisoned`.
- `DialAdmission::check(key)` / `.record_failure(key)` apply bounded
  exponential backoff keyed by `DialKey`, which redacts its `[u8; 32]` in
  `Debug`. `DialKeyError::Zero`.
- `ReplayCache::new(maximum)` / `.check_and_record(token, now, retention)` is a
  bounded map with time-based expiry that **fails closed** when full;
  `ReplayCacheDecision` reports accept/reject/expired.
- `IpPrefixPolicy` carries configurable IPv4/IPv6 prefix widths.
- `BoundNtcp2Listener::bind()` → `.start(scope)` → `ListenerHandle::next()` is
  the socket-opening constructor; the accept loop is
  `scope.spawn(move |child| ..)` under the caller's `ChildScope`. Each link
  spawns reader and writer as **separate** supervised children with their own
  child token. `LinkStartError`: `ActiveLink(..)`, `ChildScope(..)`.
- `AdmittedInboundStream` carries the non-`Clone` pending permit through
  handshake work; `AuthenticatedInboundStream` retains it until active-link
  admission succeeds. Promotion releases the permit and clears dial backoff only
  at that same authenticated gate, so a failed handshake never leaks admission.
- `ExactIoError` carries a fixed `IoErrorKind` category
  (`Closed`, `Deadline`, `Cancelled`, `Failed`); `read_exact` and
  `write_all_exact` are the only raw-socket helpers.

### `AuthenticatedLink` reader/writer supervision (`ntcp2_link.rs`)

`reader_task` (and its `writer_task` counterpart) loop: read a 2-byte length
prefix under `Ntcp2Deadline::after(deadlines.read_idle)`, decode the length,
read exactly that ciphertext, then **reserve the inbound queue accounting for
the full wire length (`2 + ciphertext_length`)**. On the frame that would
**exceed** the accounting lease, `state.reserve_inbound(wire_bytes, limits)`
returns `false` and the reader `break`s — the link closes rather than admitting
an unaccounted frame, and no partial lease is taken. Any read error, cancelled
token, or failed handoff send takes the same `break` path; the epilogue always
sets `closed` and cancels the shared link token, so the writer child is torn
down with the reader.

Every terminal path releases: `ReceivedFrameLease::Drop` releases inbound,
queued-frame drop releases outbound, and `AuthenticatedLinkSnapshot` exposes
`queue_release_underflows` so a double-release would be observable. Valid paths
leave that counter at zero (`queue_release_returns_to_baseline`).
`AuthenticatedLinkError`: `QueueFull`, `Cancelled`, `Closed`, `Frame(FrameError)`,
`Io(IoErrorKind)`, `ChildScope`. `AuthenticatedLinkStartError`: `Admission(..)`,
`QueueLimitTooLarge`, `ChildScope`. Public surface: `send_blocks`, `recv`,
`correlated_receive_oracle`, `close`, `snapshot`.

### `Ssu2RuntimeService` (`ssu2_runtime.rs`, Plans 158/159/282)

`Ssu2RuntimeService::new(config, identity)` validates without opening a socket;
`.start(&ChildScope, Ssu2SocketConfig) -> Result<Ssu2ServiceHandle, Ssu2BindError>`
binds the requested families and spawns **one loop task per family** under the
caller-owned `ChildScope`. `Ssu2BindError`: `NoSocket`, `Bind`, `State`,
`Scope`. Requested families are recorded before binding so a partial bind still
reports what the composition asked for.

Each loop task is receive classifier plus central scheduler: cheap length checks
→ side-effect-free `matches_inbound` trial → pending handshake routing →
intro-key prevalidation → admission, then **one** `tokio::time::sleep` recomputed
to the earliest handshake/ACK/RTO/idle deadline. No task per dial, no timer per
packet. Handshake resend batches always *replace* the pending deadline;
min-merging against a stale past value would burn the retry budget to
`RetriesExhausted`.

**Loopback bind strategy in tests:** `Ssu2SocketConfig { ipv4, ipv6 }` takes
explicit bind literals; the `ipv4` field is documented as "loopback tests use
`127.0.0.1:0`" and every integration suite passes exactly
`Some("127.0.0.1:0")` with `ipv6: None` so the OS assigns an ephemeral port
(`tests/ssu2_local.rs:179`, `tests/ssu2_peer_relay.rs:401`,
`tests/ssu2_controlled_peer_test.rs:150`). Raw probe/spoofer/forwarder sockets
also bind `127.0.0.1:0` (`tests/ssu2_local.rs:530,546`,
`tests/ssu2_peer_relay.rs:223`). `start_controlled_peer_test` additionally
*enforces* loopback and a non-zero port, returning
`ControlledPeerTestError::BindingMismatch` otherwise — the guard is in
production code, not only in tests.

Plan 159 path validation: every active session owns a `PathValidator` starting at
the promotion address. `handle_active_datagram` classifies the source only after
the session authenticated the datagram; unknown sources open at most one bounded
candidate (per-session quota plus the service-wide
`SSU2_MAX_PATH_CANDIDATES_GLOBAL = 256`), answered with one minimum-MTU
`PathChallenge` carrying OS-CSPRNG bytes. **Nothing migrates on source change
alone.** A `PathResponse` promotes only on an exact tracked-challenge match:
migration moves the endpoint plus per-IP/subnet accounting, resets stale
congestion state via `note_path_migrated` (unacked fragments retransmit fresh on
the new path), records a `ValidatedPath` reachability signal, and counts
`path_migrations`. Wrong/stale values count as `path_rejections` without
migration. The central scheduler expires candidates (`path_expirations`,
retaining the old path). `AddressObserved` records
`AuthenticatedPeerObservedExternalAddress` signals; `ReachabilityTracker` stays
`Unknown` until corroborated — a single observation can never publish
reachability.

Plan 282 adds `publication_material(wall_now_ms)`, building a strict direct SSU2
`RouterAddress` from the actual bound socket, runtime-owned static/intro keys,
and the live unexpired reachability snapshot. Unknown, non-direct, expired,
unbound, and closed states fail closed through
`Ssu2PublicationUnavailable` (`Closed`, `NoBoundSocket`, `StateUnavailable`,
`ReachabilityUnqualified`, `InvalidPublication`). The return value
(`Ssu2PublicationMaterial`) carries public address data and
categorical/expiry evidence only. `install_local_router_info` accepts future
handshake RouterInfo bytes only after signature and local identity checks,
preserved network id, strict endpoint/key binding to the actual socket, and a
size-bounded decode; replacement is atomic and already-established sessions are
not rewritten. Plan 283 adds the Plan 103 freshness windows mirrored locally
(dependency direction forbids importing them from `i2pr-netdb`) and the
opt-in explicit-bind corroboration flag. The bind-plus-observation pair yields
exactly `CandidateReachable`; `publication_material` still requires above-floor
`Reachable`, so static loopback-homogeneous traffic stops at the evidence
boundary by design (Plan 282 stopped; Plan 283 owns the third-class strategy and
activation).

### `Ssu2PeerRelayService` (`ssu2_peer_relay.rs`, Plan 160)

`Ssu2PeerRelayService::new(config)` owns the five Plan 160 tables
(`PeerTestTable`, `RelayRequester`, `RelayIntroducer`, `RelayTarget`,
`IntroducerTable`) plus the router-level `ReachabilityTracker`, a per-source
sliding rate limiter, and a bounded signer registry. The service holds **no
private signing keys** — outgoing blocks are signed by caller-supplied keys.

**The introducer service is disabled by default.**
`Ssu2PeerRelayConfig { pub introducer_enabled: bool }` defaults to `false`;
`issue_relay_tag` and `on_relay_request` refuse unless it is explicitly enabled
(controlled tests opt in; `introducer_stays_disabled_by_default` pins it). The
3× response budget is enforced before crypto.

Per-source rate limits: `PEER_RELAY_RATE_SOURCES = 1024` tracked sources,
`PEER_RELAY_DATAGRAMS_PER_SECOND = 8`, cheap-drop **before** parsing.
`PEER_RELAY_MAX_SIGNERS = 128`; `PEER_RELAY_RESPONSE_BUDGET_NUMERATOR = 3`.
Inbound flow per datagram: `check_admission(source)` → protocol-table ingest
with trial-commit multi-key verification (a wrong key never mutates a peer's
test) → typed outcome mirrored into reachability as family-only signals
(`Confirmed` supports; `Mismatch`/`Firewalled` contradict; `Inconclusive`/
`Rejected` neutral; relay success → `RelayFirewalledSignal`, never direct).
`poll_expired`/`next_deadline_ms` form the single central scheduler input, and
`shutdown` returns every table, tag, record, and rate window to baseline.
`Ssu2PeerRelaySnapshot` carries counts plus the conservative reachability state
— no hashes, nonces, tags, endpoints, or signatures — and `Debug` for the
service and all tables is redacted. Sealed-session carriage of in-session blocks
is proven in `i2pr-transport-ssu2/tests/peer_relay.rs`.

### NTCP2 executor, driver, observer, and the non-activation guard

`ntcp2_runtime` owns bounded NTCP2 socket/link lifecycle;
`ntcp2_driver` fulfills handshake actions with exact reads/writes, bounded
deadlines, cancellation, replay admission, clock, padding, and RouterInfo
handoff; `ntcp2_link` owns authenticated frame reader/writer children and queue
leases.

**The NTCP2 link service is composed but not activated in production.** Three
independent guards agree:

1. `crates/i2pr-daemon/src/config.rs:718` — `default_ntcp2_enabled()` returns
   `false`, so the normal daemon does not enable NTCP2.
2. `grep` finds **no** reference to `Ntcp2RuntimeService` anywhere in
   `crates/i2pr-daemon/src/`. The only non-test consumer is
   `tools/i2pr-interop/src/main.rs`, a non-production test launcher.
3. `specs/support.toml` marks the NTCP2 surfaces `status = "experimental"`,
   `advertised = false`.

Plan 101
(`plans/implementation/ntcp2-transport/101-daemon-ntcp2-activation-safety-and-router-handoff-correction.md`,
status *planned*) is the plan of record for the daemon activation-safety
corrective; the retained development result is `protocol-defect-localized` at
`noise_authenticated` (Plan 099/100, `plans/closure/ntcp2-transport/099-status.md`).
Nothing in this crate may be read as NTCP2 activation.

`ntcp2_data_oracle` and `ntcp2_handshake_observer` are **not compiled behind a
Cargo feature** — both `mod` declarations are unconditional in `lib.rs`. What
keeps them out of production is structural: neither is referenced from
`crates/i2pr-daemon/src/`, and the oracle's own module doc states it "never
weakens security validation" (a frame failing the runtime's AEAD check is
rejected before the oracle sees it).

### `observability.rs` — snapshots and redaction (ADR 0009)

The module is "an observation boundary, not an event store". `RuntimeSnapshot`
(`::try_new()`), `SupervisorSnapshot` (via `SupervisorHandle::snapshot()` /
`Supervisor::snapshot()`), `ServiceSnapshot`, `SimulationSnapshot`, and
`RouterLifecycle`. `MAX_SNAPSHOT_CHANNELS = 256`,
`MAX_SNAPSHOT_RESOURCES = 32`; `SnapshotError` is
`TooManyChannels { maximum }` / `TooManyResources { maximum }`.

The redaction policy is explicit in the module doc: snapshots contain aggregate
counters and typed categories only, and **service health detail is omitted from
the redacted projection** so a parser-controlled string cannot become a default
diagnostic field. `RuntimeSnapshot::try_new` sorts channels by name and
resources by class for deterministic diagnostics. `TaskCounters` tracks owned
service tasks, child tasks, shutdown state, and forced aborts with atomics —
this is what proves ownership and final cleanup. `event` supplies 18 stable
event-name constants (`SERVICE_REGISTERED`, `SERVICE_READY`, `SERVICE_RESTARTING`,
`SHUTDOWN_REQUESTED`, `SHUTDOWN_FORCED`, `CHANNEL_REJECTED`, `RESOURCE_DENIED`,
the five `runtime.ntcp2.*` names, and the two `simulation.*` names). This crate
emits events through `tracing`; it never installs a global subscriber.

### Bound constants (all verified in source)

| Constant | Value | Source |
| --- | --- | --- |
| `MAX_CHANNEL_CAPACITY` | 4 096 | `channel.rs:23` |
| `MAX_CHANNEL_NAME_BYTES` | 64 | `channel.rs:25` |
| `MAX_QUEUE_ITEM_BYTES` | 1 << 20 | `channel.rs:27` |
| `MAX_CHILD_TASKS` | 64 | `context.rs:20` |
| `MAX_SERVICE_COUNT` | 128 | `graph.rs:14` |
| `MAX_SERVICE_TIMEOUT` | 3 600 s | `graph.rs:16` |
| `MAX_RESTART_ATTEMPTS` | 32 | `graph.rs:24` |
| `MAX_SHUTDOWN_DEADLINE` | = `MAX_SERVICE_TIMEOUT` (3 600 s) | `supervisor.rs:31` |
| `MAX_SNAPSHOT_CHANNELS` | 256 | `observability.rs:64` |
| `MAX_SNAPSHOT_RESOURCES` | 32 | `observability.rs:66` |
| `PEER_RELAY_RATE_SOURCES` | 1 024 | `ssu2_peer_relay.rs:61` |
| `PEER_RELAY_DATAGRAMS_PER_SECOND` | 8 | `ssu2_peer_relay.rs:63` |
| `PEER_RELAY_MAX_SIGNERS` | 128 | `ssu2_peer_relay.rs:65` |
| `PEER_RELAY_RESPONSE_BUDGET_NUMERATOR` | 3 | `ssu2_peer_relay.rs:67` |
| `MAX_SSU2_RUNTIME_DURATION` | 3 600 s | `ssu2_runtime.rs:108` |
| `MAX_SSU2_PENDING_CEILING` | 1 024 | `ssu2_runtime.rs:110` |
| `MAX_SSU2_ACTIVE_CEILING` | 1 024 | `ssu2_runtime.rs:112` |
| `MAX_SSU2_STAGED_DATAGRAMS_CEILING` | 4 096 | `ssu2_runtime.rs:114` |
| `MAX_SSU2_STAGED_BYTES_CEILING` | 64 MiB | `ssu2_runtime.rs:116` |
| `MAX_SSU2_INBOUND_QUEUE_CEILING` | 1 024 | `ssu2_runtime.rs:118` |
| `SSU2_DEFAULT_MTU` | 1 280 | `ssu2_runtime.rs:120` |
| `SSU2_CONFIRMED_MTU_PAYLOAD` | 1 000 | `ssu2_runtime.rs:122` |
| `SSU2_CACHED_TOKEN_GRACE` | 1 500 ms | `ssu2_runtime.rs:139` |
| `SSU2_TOKEN_CACHE_PER_PEER` | 2 | `ssu2_runtime.rs:141` |
| `SSU2_TOKEN_CACHE_PEERS` | 128 | `ssu2_runtime.rs:143` |
| `SSU2_TOKEN_REQUEST_SOURCES` | 1 024 | `ssu2_runtime.rs:145` |
| `SSU2_TOKEN_REQUESTS_PER_SECOND` | 8 | `ssu2_runtime.rs:147` |
| `SSU2_LINKS_PER_PEER` | 8 | `ssu2_runtime.rs:149` |
| `SSU2_MANAGER_MESSAGES_PER_LINK` | 64 | `ssu2_runtime.rs:151` |
| `SSU2_MANAGER_BYTES_PER_LINK` | 256 KiB | `ssu2_runtime.rs:153` |
| `SSU2_BACKOFF_ENTRIES` | 256 | `ssu2_runtime.rs:155` |
| `SSU2_MAX_DRAIN_PER_SESSION` | 4 | `ssu2_runtime.rs:157` |
| `SSU2_MAX_PATH_CANDIDATES_GLOBAL` | 256 | `ssu2_runtime.rs:163` |
| `SSU2_SEND_QUEUE_MESSAGES` | 64 | `ssu2_runtime.rs:3019` |
| `SSU2_SEND_QUEUE_BYTES` | 256 KiB | `ssu2_runtime.rs:3021` |
| `ORACLE_MAX_FRAMES` | 16 | `ntcp2_data_oracle.rs:42` |
| `ORACLE_MAX_PLAINTEXT_BYTES` | 256 KiB | `ntcp2_data_oracle.rs:46` |
| `ORACLE_MAX_BLOCKS` | 64 | `ntcp2_data_oracle.rs:50` |
| `ORACLE_MAX_NON_TARGET_I2NP` | 16 | `ntcp2_data_oracle.rs:54` |
| `BOUNDED_STEP_BUDGET` | 5 ms | `ntcp2_data_oracle.rs:302` |
| `ORACLE_SCHEMA` | `"i2pr-ntcp2-data-oracle-v1"` | `ntcp2_data_oracle.rs:38` |

Non-exported (`pub const` in a private module, so not part of the public
surface): `DEFAULT_STARTUP_TIMEOUT` 30 s, `DEFAULT_READINESS_TIMEOUT` 30 s,
`DEFAULT_SHUTDOWN_GRACE` 5 s (`graph.rs:18-22`); `LOCAL_ROUTER_INFO_MAX_AGE_SECS`
86 400, `LOCAL_ROUTER_INFO_MAX_FUTURE_SKEW_SECS` 3 600,
`LOCAL_ROUTER_INFO_MAX_ENCODED_LEN` 16 KiB (`ssu2_runtime.rs:129-136`).

## Dependencies

`Cargo.toml:10-22` — production, all verified present, and exactly the set the
checker allowlist permits:

| Dependency | Purpose |
| --- | --- |
| `futures-util` | `FutureExt::catch_unwind` for panic classification |
| `i2pr-core` | Runtime-neutral contracts: health snapshots, lifecycle states, budgets, names |
| `i2pr-crypto` | Transport static secrets, OS randomness |
| `i2pr-proto` | Router hashes, bounded RouterInfo decode |
| `i2pr-transport` | Transport link/session contracts |
| `i2pr-transport-ntcp2` | NTCP2 handshake/frame state machines and actions |
| `i2pr-transport-ssu2` | SSU2 handshake/session/peer-test machines, introducer table, address types |
| `rand_core` (`os_rng`) | `OsRng` fulfillment for nonces and padding |
| `tokio` | **The runtime.** Sockets, timers, tasks, sync primitives |
| `tokio-util` | `CancellationToken` primitive |
| `tracing` | Structured event emission (never a global subscriber) |
| `zeroize` | Static-secret hygiene |

`[dev-dependencies]`: `flate2`, plus `i2pr-crypto`, `i2pr-proto`,
`i2pr-transport`, `i2pr-transport-ssu2` for test fixtures. No dev-dependency on
any non-test crate that the production set does not already allow.

`scripts/check-dependency-direction.sh` allowlist for this crate
(`scripts/check-dependency-direction.sh:51-54`):

```python
"i2pr-runtime": {
    "i2pr-core", "i2pr-crypto", "i2pr-proto", "i2pr-transport", "i2pr-transport-ntcp2",
    "i2pr-transport-ssu2",
},
```

Six in-workspace edges; `rand_core`, `tokio`, `tokio-util`, `tracing`, and
`zeroize` are external and checked by `cargo deny` instead. The direction is
strictly inward: this crate depends on the runtime-neutral crates and nothing
depends back on it except `i2pr-daemon` (composition root) and `i2pr-testkit`.

Workspace-level Tokio feature set (root `Cargo.toml:65`, `default-features =
false`): `io-util`, `macros`, `net`, `rt`, `signal`, `sync`, `time`,
`test-util`. `net` is the socket surface, `time` + `test-util` are what make
`#[tokio::test(start_paused = true)]` possible, and `sync` backs the bounded
channels. `tokio-util` is `default-features = false, features = ["rt"]` — only
the cancellation primitive is used.

`#![forbid(unsafe_code)]` **is** present at `lib.rs:9`, and the workspace lint
table additionally sets `unsafe_code = "deny"`. This crate owns sockets and
timers but still holds the blanket rule, so the blanket claim in `AGENTS.md`
applies to it without exception.

## Tests

82 in-crate `#[test]` / `#[tokio::test]` functions across 11 modules, plus 20
integration tests across the 4 files in `tests/` — 102 total.

### In-crate unit tests

| Module | Tests | Notable |
| --- | --- | --- |
| `cancel.rs` | 4 async | first-reason-wins, parent reason visible to child, all waiters wake |
| `channel.rs` | 11 (8 `start_paused`) | ordering + resource charge until processing finishes, `send_until` deadline/cancel, synthetic overload graph drains with no usage or task leak, request/latest-state paths |
| `context.rs` | 3 | child scope join, bounded task limit, shutdown report |
| `graph.rs` | 6 sync | lexical topological determinism, invalid graphs rejected before startup, restartable services require a policy, optional-startup and essential classification is a contradiction, startup requirement defaults to `Required` |
| `ntcp2_data_oracle.rs` | 5 | bounded step budget, every negative bound, exact target correlation |
| `ntcp2_driver.rs` | 6 (4 `start_paused`) | initiator/responder action fulfillment, padding bounds |
| `ntcp2_handshake_observer.rs` | 2 | observer records metadata only, never payload |
| `ntcp2_link.rs` | 1 | `queue_release_returns_to_baseline` (RAII accounting) |
| `ntcp2_runtime.rs` | 8 (2 `start_paused`) | `admission_is_global_ip_and_subnet_bounded_and_releases`, `replay_cache_fails_closed_and_expires_deterministically`, `loopback_listener_and_exact_io_use_supervised_scope`, `link_reader_and_writer_are_joined_after_close`, `queue_entry_drop_releases_exactly_once`, `active_link_admission_is_exact_and_raii`, `queue_and_active_link_teardown_repeat_without_underflow` |
| `ssu2_peer_relay.rs` | 7 sync | `introducer_stays_disabled_by_default`, rate-limiter cheap-drop before crypto, quotas + shutdown to baseline, relay-request replay does not re-amplify, snapshot/Debug expose no secrets, earliest-deadline tracking, unknown signer fails closed |
| `ssu2_runtime.rs` | 19 | 5 sync validation (`limits_validate_rejects_zero_ceiling_and_scope_violations`, `deadlines_validate_ordering_and_bounds`, `dial_targets_validate_before_socket_activity`, `identity_rejects_malformed_router_info`, freshness/permit checks) + 3 real-UDP path-validation tests (`legitimate_path_migration_over_real_udp`, `spoof_burst_from_new_sources_never_migrates`, `challenge_response_control_round_trip_over_real_udp`) + 11 publication / controlled-test / network-condition tests |
| `supervisor.rs` | 13 (8 `start_paused`) | `services_start_ready_and_shutdown_gracefully`, `panic_is_classified_without_payload`, `restartable_services_use_bounded_backoff`, `degradable_and_optional_failures_do_not_stop_essential_work`, `forced_shutdown_aborts_and_joins_the_owned_child_scope`, **`forced_child_cleanup_is_repeatably_joined`**, `service_child_scope_is_joined_before_manager_completion` |

`observability.rs` and `ssu2_controlled_peer_test.rs` carry no in-crate
`#[cfg(test)]` module; their behavior is covered by the integration suites.

### Integration tests (`crates/i2pr-runtime/tests/`)

| File | Tests | Pattern | Coverage |
| --- | --- | --- | --- |
| `ssu2_local.rs` | 9 async | **real sockets, real time, `127.0.0.1:0`** | `tokenless_establishment_over_real_udp`, `active_peer_hashes_names_live_session_peers`, `cached_token_establishment_with_stale_recovery`, `bidirectional_i2np_exchange_with_fragmentation`, `data_loss_recovers_with_exact_once_delivery`, `ack_loss_reorder_and_duplicate_recover_exactly_once`, `malformed_and_random_traffic_creates_no_state`, `active_session_cap_denies_with_baseline_return`, `graceful_close_abrupt_peer_and_cancel_return_to_baseline` |
| `ssu2_peer_relay.rs` | 7 async | **real sockets, `127.0.0.1:0`** | direct peer test with NAT rewrite, mismatch/inconclusive, 40-datagram flood cheap-drop, relay product path ending in a live `Ssu2RuntimeService` dial with bidirectional I2NP, introducer expiry/disabled/shutdown, concurrent isolation with crossing schedules, publication/privacy integration |
| `ssu2_controlled_peer_test.rs` | 3 async | real sockets, `127.0.0.1:0` | `controlled_exchange_promotes_loopback_to_reachable_with_direct_material`, `controlled_exchange_is_refused_without_the_permit`, `controlled_apis_fail_closed` |
| `ssu2_independent.rs` | 1, `#[ignore]` | env-gated | `ssu2_independent_ipv4_interop` — `#[ignore = "Plan 161: requires exact-pinned external i2pd environment"]`. Compiled with the workspace, skipped by ordinary libtest, selected only by `tests/integration/ssu2/run-independent.sh` with `--ignored --exact`; missing env still reaches a hard failure, never a skip |

### Posture

- **Paused time is the default for state-machine and overload tests.**
  `#[tokio::test(start_paused = true)]` appears in `channel.rs` (8),
  `ntcp2_driver.rs` (4), `ntcp2_runtime.rs` (2), `supervisor.rs` (8), and
  `cancel.rs`. No wall-clock sleeps participate in overload or state-machine
  assertions.
- **Real-socket suites are loopback-only and use OS-assigned ports.**
  Every bind literal in `tests/` is `127.0.0.1:0`; `ssu2_independent.rs` is the
  only file with fixed port numbers, and only because it dials an
  env-supplied exact-pinned i2pd endpoint.
- **Real-socket suites use real elapsed time** via explicit bounded
  `tokio::time::sleep` waits (`POLL_INTERVAL`, 300–400 ms settle windows) rather
  than paused time, because paused time cannot advance a real handshake.
  `check-runtime-boundaries.sh` bans `thread::sleep`, not `tokio::time::sleep`.
- **Bounded negative paths** are first-class: zero/oversized ceilings,
  zero deadlines, invalid prefixes, malformed RouterInfo, admission-limit
  denial with baseline return, replay-cache full (fails closed), unknown signer,
  introducer disabled, controlled-test permit absent, and every
  `*_fail_closed` / `*_refused` name.
- **Lease release on every drop path** is covered from both sides: the channel
  side by `queue_entry_drop_releases_exactly_once` and the synthetic-overload
  graph test, and the NTCP2 link side by
  `queue_release_returns_to_baseline` and
  `queue_and_active_link_teardown_repeat_without_underflow`. The
  `queue_release_underflows` counter makes a double-release observable rather
  than silent.
- **`--test-threads=1` is required** for `i2pr-runtime` and `i2pr-daemon`
  suites. These are real loopback socket tests; running them in parallel under
  Cargo causes flakes (notably `forced_child_cleanup_is_repeatably_joined`, a
  repeatability test that the routine floor runs serially). macOS CI builds each
  test executable once and then runs it with `--test-threads=1` for the same
  reason. The multi-socket suites are additionally written to be individually
  serial-safe by binding distinct ephemeral ports.

## Distinctive design choices

1. **Sole Tokio owner, enforced positively** — the checker allows `tokio`/
   `tokio-util` manifests in exactly `i2pr-runtime` and `i2pr-testkit`, so
   "who may hold the runtime" is a machine-checked fact rather than a convention.
2. **Capability-narrowed service context** — `ServiceContext` exposes exactly
   name, cancellation, readiness, health, and child scope; a service structurally
   cannot reach the supervisor.
3. **Resource charge lives with the queue entry, not the send** — dropping a
   `Received<T>` releases the charge, so a stalled consumer holds its budget
   honestly.
4. **`send_until` reserves the queue permit before taking the resource lease** —
   a blocked sender never pins budget while waiting for a queue slot, and the
   `biased` select puts cancellation ahead of the deadline.
5. **First-reason-wins cancellation that a child cannot overwrite** — an
   inherited parent reason is sticky, so a fast child cannot mask why the tree
   went down.
6. **`JoinSet` is the only task owner** — `supervisor.rs` and `context.rs` hold
   `JoinSet<..>`; the crate contains no `JoinHandle` identifier at all, which
   the boundary script bans outright.
7. **Bounded forced shutdown with a poll budget** — `0..=MAX_CHILD_TASKS`
   `yield_now()`-interleaved iterations stop a non-cooperative child from
   extending shutdown indefinitely, and `MAX_CHILD_TASKS` bounds *live* tasks
   because finished children are reaped.
8. **Accounting-lease admission, not post-hoc accounting** — a frame that would
   exceed the inbound lease breaks the reader *before* the lease is taken, so
   the link closes rather than running unaccounted, and
   `queue_release_underflows` makes double-release observable.
9. **Exact-once promotion at one authenticated gate** — the inbound permit and
   the dial-backoff clear are both released only after active-link admission
   succeeds, so a failed handshake leaks neither.
10. **SSU2 routes before it parses** — the side-effect-free `matches_inbound`
    trial means unknown traffic never creates persistent state, and the central
    scheduler recomputes exactly one sleep per loop instead of a timer per
    packet.
11. **Conservative reachability** — a single address observation can never
    publish reachability; the tracker stays `Unknown` until corroborated, and a
    relay success is recorded as `RelayFirewalledSignal`, never as direct
    reachability.
12. **Redacted-by-default observability** — snapshots carry aggregate counters
    and typed categories, and health *detail* is deliberately dropped from the
    redacted projection so a parser-controlled string cannot become a default
    diagnostic field.
13. **The crate is a bounded seam, not a protocol driver** — it executes
    transport actions and enforces runtime ownership, but claims no handshake,
    frame, manager-registration, or mixed-router completion.

## Cross-references

- [Overview](overview.md) · [Dependency graph](dependency-graph.md) ·
  [Tooling](tooling.md)
- [i2pr-core](i2pr-core.md) — the runtime-neutral contracts this crate
  specializes and re-exports.
- [i2pr-transport](i2pr-transport.md) — the link/session contract surface driven
  from supervised services.
- [i2pr-transport-ntcp2](i2pr-transport-ntcp2.md) — produces the
  `HandshakeAction` / `FrameAction` values fulfilled by `ntcp2_driver` and
  `ntcp2_link` (experimental, non-advertised).
- [i2pr-transport-ssu2](i2pr-transport-ssu2.md) — produces the SSU2
  `HandshakeAction` / `SessionAction` / `SessionEvent` values fulfilled by
  `ssu2_runtime` (Plans 158–160).
- [i2pr-daemon](i2pr-daemon.md) — the composition root that owns the listeners
  and the process runtime.
- ADRs: `docs/adr/0002-tokio-runtime-boundary.md` (Tokio at runtime-facing
  boundaries), `0003-bounded-supervised-services.md` (bounded supervised
  services and explicit cancellation), `0008-runtime-supervision-and-cancellation.md`
  (concrete runtime ownership and wakeable supervision),
  `0009-runtime-observability-and-validation.md` (privacy-aware runtime
  observability and deterministic validation).
- Plans of record: `plans/implementation/workspace-foundation/021-m2-supervision-cancellation.md`,
  `022-m2-bounded-channels-resource-governor.md`;
  `plans/implementation/ntcp2-transport/035-m3-runtime-link-manager-and-addresses.md`,
  `101-daemon-ntcp2-activation-safety-and-router-handoff-correction.md`;
  `plans/implementation/ssu2/158-m8-ssu2-udp-runtime-and-local-session-product.md`,
  `159-m8-ssu2-path-validation-publication-and-transport-selection.md`,
  `160-m8-ssu2-peer-test-and-relay-reachability.md`;
  `plans/implementation/floodfill/282-m12-runtime-publication-and-reply-delivery-contract-corrective.md`,
  `283-m12-third-class-evidence-and-controlled-activation.md`.
- Closures: `plans/closure/workspace-foundation/021-closure.md`,
  `022-closure.md`; `plans/closure/ntcp2-transport/035-closure.md`,
  `037-m3-corrective-integration-closure.md`;
  `plans/closure/ssu2/158-status.md`, `159-status.md`, `160-status.md`,
  `161-status.md`, `162-status.md`; `plans/closure/floodfill/282-status.md`,
  `283-status.md`.
