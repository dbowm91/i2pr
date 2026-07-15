# `i2pr-runtime` — Deep Dive

The **only production owner of Tokio** in the workspace. Built on top
of `i2pr-core` (contracts) and `i2pr-transport` (link contracts), it provides
the bounded socket, timer, channel, and wakeable-cancellation seam that fulfills
`i2pr-transport-ntcp2` actions. Plan 042 adds the runtime-owned handshake
executor and authenticated data-frame link; these are controlled local
composition surfaces, not mixed-router evidence. Plan 044 confirms that the
runtime-owned NTCP2 wire adapter is implemented and locally validated; mixed-
router harness composition and authorized evidence remain pending.

Path: `crates/i2pr-runtime/`

## Purpose

`i2pr-runtime` is the seam between the runtime-neutral crates and the
rest of the world. It is where:

- Supervision trees are run (topological ordering, restart policy,
  graceful/forced shutdown).
- Wakeable cancellation is implemented (with parent-chain reason
  walking).
- Bounded service channels are built (command, event, request,
  latest-state) with resource charging.
- TCP listeners and link children are owned.
- NTCP2 actions are fulfilled by `ntcp2_driver` with exact reads/writes,
  deadlines, cancellation, replay admission, clock, padding, and RouterInfo
  handoff.
- `ntcp2_link` owns authenticated frame reader/writer children and queue
  leases. Listener/dial promotion keeps pending admission attached until
  active-link admission succeeds.
- Privacy-safe runtime snapshots are produced.

The contract that protocol, transport, and storage crates stay free
of Tokio is enforced by `scripts/check-runtime-boundaries.sh`.

## Module layout

| Module | File | Lines | Responsibility | Key public types |
| --- | --- | --- | --- | --- |
| `cancel` | `src/cancel.rs` | 169 | Hierarchical wakeable cancellation with first-reason-wins semantics and parent-walk | `CancellationToken` |
| `channel` | `src/channel.rs` | 1908 | Typed bounded service channels with resource charging, overflow policies, privacy-safe counters | `ChannelSpec`, `ChannelName`, `CommunicationClass`, `OverflowPolicy`, `QueueCharge`, `*Sender*`/`*Receiver*`, `Received`, `ReceivedRequest`, `ChannelSnapshot`, all error types |
| `context` | `src/context.rs` | 585 | Per-service context bundle, readiness signals, health publication, child-task scope with bounded join and forced abort | `ServiceContext`, `Readiness`, `HealthReporter`, `HealthReceiver`, `ChildScope`, `ChildFailurePolicy`, `ChildTaskFailure`, `ChildScopeError`, `ChildShutdownReport` |
| `graph` | `src/graph.rs` | 648 | Service registration, deterministic topological ordering, full graph validation before startup | `ServiceGraph`, `ServiceGraphBuilder`, `ServiceSpec`, `ServiceFuture`, `RestartPolicy`, `RestartExhaustion`, `RestartPolicyError`, `GraphError` |
| `ntcp2_runtime` | `src/ntcp2_runtime.rs` | — | Bounded NTCP2 socket/link lifecycle, TCP listener ownership, admission control, replay cache, dial backoff, link reader/writer children, exact I/O helpers | `Ntcp2RuntimeService`, `BoundNtcp2Listener`, `ListenerHandle`, `LinkHandle`, `InboundAdmission`, `ReplayCache`, `DialAdmission`, etc.; fns `read_exact`, `write_all_exact` |
| `ntcp2_driver` | `src/ntcp2_driver.rs` | Plan 042 | Runtime-owned handshake action executor with bounded deadlines, cancellation, replay, clock, padding, and RouterInfo provision | `HandshakeDriverConfig`, `HandshakeRun`, `drive_initiator_handshake`, `drive_responder_handshake` |
| `ntcp2_link` | `src/ntcp2_link.rs` | Plan 042 | Authenticated frame reader/writer children and item/byte accounting leases | `AuthenticatedLink`, `ReceivedFrameLease`, `AuthenticatedLinkSnapshot` |
| `observability` | `src/observability.rs` | 360 | Privacy-aware runtime events (tracing), bounded aggregate snapshots, shared task counters | `RouterLifecycle`, `SupervisorSnapshot`, `ServiceSnapshot`, `RuntimeSnapshot`, `SimulationSnapshot`, `event::*` |
| `supervisor` | `src/supervisor.rs` | 1703 | Service startup sequencing, health tracking, restart with bounded exponential backoff, graceful/forced shutdown | `Supervisor`, `SupervisorHandle`, `SupervisorError`, `SupervisorConfigError`, `ShutdownReport`, `ShutdownOutcome` |

## Public surface (crate-root re-exports, `lib.rs:19-61`)

- `cancel`: `CancellationToken`
- `channel`: `ChannelConfigError`, `ChannelName`, `ChannelNameError`,
  `ChannelSnapshot`, `ChannelSpec`, `CommunicationClass`,
  `EventReceiver`, `EventSendError`, `EventSender`, `LatestState`,
  `LatestStateReceiver`, `LatestStateSender`,
  `MAX_CHANNEL_CAPACITY`, `MAX_CHANNEL_NAME_BYTES`,
  `MAX_QUEUE_ITEM_BYTES`, `OverflowPolicy`, `QueueCharge`,
  `ReceiveError`, `Received`, `ReceivedRequest`, `RequestChannelParts`,
  `RequestError`, `RequestReceiver`, `RequestSender`, `SendError`,
  `StateUpdateError`, `TryReceiveError`, `command_channel`,
  `event_channel`, `latest_state_channel`, `request_channel`
- `context`: `ChildFailurePolicy`, `ChildScope`, `ChildScopeError`,
  `ChildShutdownReport`, `ChildTaskFailure`, `HealthReceiver`,
  `HealthReporter`, `MAX_CHILD_TASKS`, `Readiness`, `ReadinessError`,
  `ServiceContext`
- `graph`: `GraphError`, `MAX_RESTART_ATTEMPTS`, `MAX_SERVICE_COUNT`,
  `MAX_SERVICE_TIMEOUT`, `RestartExhaustion`, `RestartPolicy`,
  `RestartPolicyError`, `ServiceFuture`, `ServiceGraph`,
  `ServiceGraphBuilder`, `ServiceResult`, `ServiceSpec`
- `ntcp2_runtime`: `AddressFamily`, `AdmissionDenied`,
  `AdmissionRejection`, `AdmissionSnapshot`, `BoundNtcp2Listener`,
  `DialAdmission`, `DialAttempt`, `DialBackoffConfig`,
  `DialBackoffDecision`, `DialBackoffSnapshot`, `DialKey`,
  `AdmittedInboundStream`, `ActiveLinkAdmission`, `ActiveLinkPermit`,
  `ActiveLinkSnapshot`, `DialKeyError`, `DialOutcome`, `ExactIoError`,
  `InboundAdmission`,
  `InboundChunk`, `InboundPermit`, `IoErrorKind`, `IpPrefixPolicy`,
  `LinkHandle`, `LinkId`, `LinkSendError`, `LinkSnapshot`,
  `LinkTermination`, `ListenerHandle`, `ListenerSnapshot`,
  `Ntcp2Deadline`, `Ntcp2DeadlineError`, `Ntcp2RuntimeConfig`,
  `Ntcp2RuntimeConfigError`, `Ntcp2RuntimeDeadlines`,
  `Ntcp2RuntimeLimits`, `Ntcp2RuntimeService`, `ReplayCache`,
  `ReplayCacheDecision`, `ReplayCacheSnapshot`, `RuntimeLimitKind`,
  `WriteOutcome`, `read_exact`, `write_all_exact`
- `observability`: `MAX_SNAPSHOT_CHANNELS`, `MAX_SNAPSHOT_RESOURCES`,
  `RouterLifecycle`, `RuntimeSnapshot`, `ServiceSnapshot`,
  `SimulationSnapshot`, `SnapshotError`, `SupervisorSnapshot`,
  `event`
- `supervisor`: `MAX_SHUTDOWN_DEADLINE`, `ShutdownOutcome`,
  `ShutdownReport`, `Supervisor`, `SupervisorConfigError`,
  `SupervisorError`, `SupervisorHandle`
- Plan 042 NTCP2 composition: `HandshakeClock`, `HandshakeDriverConfig`,
  `HandshakeDriverError`, `HandshakeRun`, `PaddingProfile`,
  `AuthenticatedLink`, `AuthenticatedLinkError`,
  `AuthenticatedLinkSnapshot`, `AuthenticatedLinkStartError`, and
  `ReceivedFrameLease`; helpers `run_blocking` and `bounded_timeout` keep
  Tokio ownership inside this crate.
- Re-exports from `i2pr-core`: `CancellationReason`, `DegradationCode`,
  `FailureCategory`, `HealthDetail`, `HealthSnapshot`, `HealthState`,
  `InvalidLifecycleTransition`, `LifecycleState`,
  `ServiceClassification`, `ServiceCompletion`, `ServiceFailure`,
  `ServiceFailureCategory`, `ServiceName`, `ServiceNameError`,
  `ShutdownReason`

## Key subsystems

### Supervision tree (`supervisor.rs`)
- `Supervisor::new(graph, shutdown_deadline)` →
  `Supervisor::run()` — main async loop.
- `SupervisorHandle` returned to callers.
- Spawns one manager task per service in `JoinSet<ManagerOutput>`
  (`supervisor.rs:356, 575`). Managers (`run_manager`) implement
  restart with bounded exponential backoff via `RestartPolicy`.
- Graceful shutdown races a `tokio::time::sleep` deadline; forces
  `abort_all()` on expiry (`supervisor.rs:924`), then drains child
  scopes via `force_shutdown()`.

### Service context & child tasks (`context.rs`)
- `ServiceContext` bundles per-service state (name, cancellation,
  readiness, health, children).
- `ChildScope::spawn(factory)` (`context.rs:414`),
  `shutdown()` (`context.rs:449`),
  `force_shutdown()` (`context.rs:471`).
- Bounded to `MAX_CHILD_TASKS = 64` (`context.rs:20`).
- Each child gets a child `CancellationToken`. Panics caught via
  `AssertUnwindSafe + catch_unwind` (`context.rs:437-439`).
- `force_shutdown` aborts all children and drains with a bounded
  poll budget (`context.rs:489-502`).

### Bounded channels (`channel.rs`)
- `command_channel()`, `event_channel()`, `request_channel()`,
  `latest_state_channel()`.
- Backed by `tokio::sync::mpsc` / `oneshot` / `watch` — **no
  unbounded channels** (verified by grep).
- Hard ceiling: `MAX_CHANNEL_CAPACITY = 4_096` (`channel.rs:23`).
- Every send is `try_send` or `send_until` with deadline +
  cancellation (`channel.rs:769-826`).
- Resource charging: `QueueCharge::PerItem` or `PerBytes`,
  validated against a `ResourceBudget` before admission
  (`channel.rs:479-519`).
- `Received<T>` / `ReceivedRequest` own their `ResourceLease` —
  drop releases the charge (`channel.rs:670-676`, `690-692`).

### Wakeable cancellation (`cancel.rs`)
- `CancellationToken::new()`, `.child_token()`, `.cancel(reason)`,
  `.cancelled()`, `.cancelled_reason()`.
- Wraps `tokio_util::CancellationToken`.
- First-reason-wins: only the first `.cancel()` records a reason
  (`cancel.rs:54-67`).
- Parent reason walks the chain recursively (`cancel.rs:85-99`).
- Every `tokio::select!` branch uses `cancellation.cancelled()` as
  a biased first branch.

### Resource governor / admission (`ntcp2_runtime.rs`)
- `InboundAdmission::admit(address)` — global, per-IP, per-subnet
  admission. `InboundPermit` RAII guard (`ntcp2_runtime.rs:592-610`).
- `DialAdmission::check(key)` / `.record_failure(key)` — bounded
  exponential backoff with `DialKey` (redacted `[u8; 32]`).
- `IpPrefixPolicy` supports configurable IPv4/IPv6 prefix widths.

### Health publication (`context.rs`)
- `HealthReporter::report()`, `.ready()`, `.degraded()`;
  `HealthReceiver::snapshot()`, `.changed()`.
- Backed by `tokio::sync::watch<HealthSnapshot>` — latest state
  only, no unbounded history.
- Transitions tracked with a monotonic `transition_sequence` counter.

### TCP listener & link children (`ntcp2_runtime.rs`)
- `BoundNtcp2Listener::bind()` → `.start(scope)` →
  `ListenerHandle::next()`.
- `LinkHandle::start(scope, stream, ...)` → `.send()`, `.close()`.
- `BoundNtcp2Listener` is the **only** socket-opening constructor
  (`ntcp2_runtime.rs:707-721`, uses `TcpListener::bind`).
- Accept loop spawned via `scope.spawn(move |child| async move { ... })`
  (`ntcp2_runtime.rs:731`).
- Each link spawns two supervised children (reader + writer) with
  their own `CancellationToken` (`ntcp2_runtime.rs:1194, 1219`).
- Reader uses a fixed 4096-byte buffer. Reader EOF or error
  cancels the writer via the shared token.

### Handshake and authenticated data owner (`ntcp2_driver.rs`, `ntcp2_link.rs`)
- `drive_initiator_handshake` and `drive_responder_handshake` consume the
  protocol state machines and reject ambiguous unframed bounded reads.
- `AuthenticatedInboundStream` retains the pending inbound permit through
  successful handshake; promotion releases it only after active-link
  admission succeeds. Dial backoff is cleared only at the same authenticated
  gate.
- `AuthenticatedLink::send_blocks` and `recv` expose bounded owners. Queue
  leases release on write success, failure, cancellation, receiver closure, or
  owner drop.
- `run_blocking` and `bounded_timeout` are runtime-owned helpers used by the
  isolated launcher, keeping Tokio dependencies out of tooling and
  runtime-neutral crates.

### Replay cache (`ntcp2_runtime.rs`)
- `ReplayCache::new(maximum)`,
  `.check_and_record(token, now, retention)`.
- Bounded `BTreeMap<[u8; 32], ReplayEntry>` with time-based
  expiration. Fails closed when full.

### Observability / snapshots (`observability.rs`)
- `RuntimeSnapshot::try_new()`,
  `SupervisorSnapshot` (via `SupervisorHandle::snapshot()`),
  `event::*` constants.
- `TaskCounters` tracks owned service tasks, child tasks, shutdown
  state, forced aborts using atomics — proves ownership and final
  cleanup.
- Snapshots redact payloads and diagnostic text.

## Bounded-channel rules and `tokio::spawn` ownership

Verified by grep:

- **Zero matches** for `unbounded_channel | UnboundedSender |
  UnboundedReceiver` — `check-runtime-boundaries.sh` ✅.
- All `tokio::spawn` calls are bound:
  - Test code only: `let task = tokio::spawn(...)` (multiple
    locations in `supervisor.rs`, `channel.rs`).
  - `waiters.push(tokio::spawn(...))` in `cancel.rs:160` (pushed
    into an owned `Vec`).
- `JoinSet::spawn`:
  - `Supervisor::spawn_manager` (`supervisor.rs:575`) on
    `&mut JoinSet<ManagerOutput>`.
  - `ChildScope::spawn` (`context.rs:435`) inside an `AsyncMutex`.
- `ChildScope::spawn` / `scope.spawn` delegated to the `JoinSet`
  inside `ChildScopeInner`.

Every spawn is bound to `let`, `push(`, or a `JoinSet` — passes the
runtime-boundary check.

## Dependencies

`Cargo.toml:10-16`:

| Dependency | Purpose |
| --- | --- |
| `futures-util` | `FutureExt` for `catch_unwind` |
| `i2pr-core` | Core types, resource budgets, health snapshots |
| `i2pr-transport` | Transport contracts (no Tokio) |
| `tokio` | Runtime — one of only two crates allowed this |
| `tokio-util` | `CancellationToken` primitive |
| `tracing` | Structured event emission |

`AGENTS.md` permits `tokio`/`tokio-util` only in `i2pr-runtime` and
`i2pr-testkit`. ✅

## Tests

Most async tests use `#[tokio::test(start_paused = true)]`; the two explicit
socket lifecycle tests use loopback-only sockets and are never public-network
tests. There are no wall-clock sleeps or DNS lookups. Fixed seeds are implicit
in the paused runtime.

| Module | Tests | Notable |
| --- | --- | --- |
| `cancel.rs:116-169` | 4 async | `cancellation_before_wait_is_immediate`, `parent_reason_is_visible_to_child`, `all_waiters_wake` |
| `channel.rs:1575-1908` | 10 | `commands_are_ordered_and_resource_charged_until_processing_finishes`, `synthetic_overload_graph_drains_and_shuts_down_without_usage_or_tasks`, `request_*`, `latest_state_*` |
| `graph.rs:577-648` | 3 sync | `topological_order_is_lexically_deterministic`, `invalid_graphs_are_rejected_before_startup`, `restartable_services_require_a_policy` |
| `ntcp2_runtime.rs` | 8 | `admission_is_global_ip_and_subnet_bounded_and_releases`, `replay_cache_fails_closed_and_expires_deterministically`, `loopback_listener_and_exact_io_use_supervised_scope`, queue RAII, active-link admission, and repeated teardown tests |
| `supervisor.rs:1267-1703` | 13 | **`forced_child_cleanup_is_repeatably_joined`** (100-iteration, requires `--test-threads=1`), `panic_is_classified_without_payload`, `forced_shutdown_aborts_and_joins_the_owned_child_scope`, `restartable_services_use_bounded_backoff` |

## Distinctive design choices

1. **Two-level cancellation hierarchy** — wraps
   `tokio_util::CancellationToken` with first-reason-wins + parent
   chain reason walking.
2. **Resource charging tied to queue entries, not sends** — the
   charge lives as long as the queue entry. Dropping the received
   item releases the charge.
3. **`send_until` reserves capacity before acquiring the resource
   lease** (channel.rs:734 → 752) — a blocked sender doesn't hold a
   lease while waiting for a queue slot.
4. **`DialKey` redacts its `[u8; 32]` in `Debug`** — renders as
   `DialKey(<redacted>)`.
5. **`InboundChunk` transfers an admitted stream owner** — the
   `AdmittedInboundStream` wrapper carries the non-cloneable permit through
   handshake work; dropping the wrapper releases admission exactly once.
6. **`LinkHandle` spawns reader and writer as separate supervised
   children** — each link is two tasks in the `ChildScope`; service-created
   links retain one active-link lease until the handle is dropped.
7. **Queue entries are RAII owners** — one queued frame releases its item and
   byte accounting on write, cancellation, receiver closure, or scope teardown;
   valid paths leave the underflow counter at zero.
8. **Forced child shutdown uses a bounded poll budget** —
   `for _ in 0..=MAX_CHILD_TASKS` with `yield_now()` interleaved
   prevents a non-cooperative child from extending shutdown
   indefinitely.
9. **`ServiceContext` narrows the API surface** — services receive
   only the context bundle, never a direct handle to the supervisor.
10. **`RuntimeSnapshot::try_new` sorts channels and resources** —
   by name and by class — for deterministic diagnostics.
11. **The `channel` module is the largest file** — it implements four channel
   paradigms with a shared `CommandSenderInner<T>`.
12. **The runtime is a bounded seam, not yet a protocol driver** —
   `start_link()` enforces runtime ownership after an external authenticated
   handoff; it does not claim handshake, frame, manager-registration, or
   mixed-router completion.
13. **No `async fn` in transport contracts** — this crate provides
   the async bridge via `read_exact` / `write_all_exact`.
14. **`Ntcp2RuntimeService` is `Clone`** — backed entirely by
   `Arc`-wrapped shared state.

## Cross-references

- [Overview](overview.md)
- [i2pr-core](i2pr-core.md) — provides the runtime-neutral types
  this crate specializes.
- [i2pr-transport](i2pr-transport.md) — contract surface driven
  from supervised services.
- [i2pr-transport-ntcp2](i2pr-transport-ntcp2.md) — produces
  `HandshakeAction` / `FrameAction` requests fulfilled here.
- Plan-of-record: `plans/021-m2-supervision-cancellation.md`,
  `plans/022-m2-bounded-channels-resource-governor.md`,
  `plans/035-m3-runtime-link-manager-and-addresses.md`,
  `plans/037-m3-corrective-integration-closure.md`.
- Closures: `plans/021-closure.md`, `plans/022-closure.md`,
  `plans/035-closure.md`, `plans/037-closure.md`.
