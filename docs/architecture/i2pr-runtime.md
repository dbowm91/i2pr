# `i2pr-runtime` — Deep Dive

The **only production owner of Tokio** in the workspace. Built on top
of `i2pr-core` (contracts) and `i2pr-transport` (link contracts), it provides
the bounded socket, timer, channel, and wakeable-cancellation seam that fulfills
`i2pr-transport-ntcp2` actions and `i2pr-transport-ssu2` handshake/session
actions over real UDP. Plan 042 adds the runtime-owned handshake
executor and authenticated data-frame link; these are controlled local
composition surfaces, not mixed-router evidence. Plan 044 confirms that the
runtime-owned NTCP2 wire adapter is implemented and locally validated; mixed-
router harness composition and authorized evidence remain pending.
Plan 158 adds the runtime-owned SSU2 UDP service with its central
bounded scheduler and real-loopback local session product; Plan 159
adds authenticated path validation/migration, conservative
reachability observations, and the scheduler-owned candidate expiry;
Plan 160 adds the runtime peer-test/relay coordinator with rate
limits, signer registry, validated introducer records, and the
real-UDP NAT-like acceptance.
Public advertisement and router-to-router interoperability remain
pending (Plan 161). Plan 162 adds no runtime behavior: the environment-
dependent Plan 161 external test is compiled with the workspace, ignored
by ordinary libtest execution, and explicitly selected by its dedicated
exact-pinned-i2pd lane.

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
- UDP sockets and SSU2 session children are owned (Plan 158).
- NTCP2 actions are fulfilled by `ntcp2_driver` with exact reads/writes,
  deadlines, cancellation, replay admission, clock, padding, and RouterInfo
  handoff.
- SSU2 handshake/data actions are fulfilled by `ssu2_runtime` with cheap
  receive classification, OS randomness/time, admission, a central
  scheduler, path validation/migration, reachability observations,
  and `TransportManager` promotion.
- `ntcp2_link` owns authenticated frame reader/writer children and queue
  leases. Listener/dial promotion keeps pending admission attached until
  active-link admission succeeds.
- Privacy-safe runtime snapshots are produced.
- The Plan 161 independent SSU2 test driver remains a test-only consumer of
  this surface. Its `#[ignore]` metadata keeps routine workspace and direct
  test-executable runs peer-free; the external lane opts in with
  `--ignored --exact` and retains fail-closed environment validation.

The contract that protocol, transport, and storage crates stay free
of Tokio is enforced by `scripts/check-runtime-boundaries.sh`.

## Module layout

| Module | File | Lines | Responsibility | Key public types |
| --- | --- | --- | --- | --- |
| `cancel` | `src/cancel.rs` | 169 | Hierarchical wakeable cancellation with first-reason-wins semantics and parent-walk | `CancellationToken` |
| `channel` | `src/channel.rs` | 1908 | Typed bounded service channels with resource charging, overflow policies, privacy-safe counters | `ChannelSpec`, `ChannelName`, `CommunicationClass`, `OverflowPolicy`, `QueueCharge`, `*Sender*`/`*Receiver*`, `Received`, `ReceivedRequest`, `ChannelSnapshot`, all error types |
| `context` | `src/context.rs` | 585 | Per-service context bundle, readiness signals, health publication, child-task scope with bounded join and forced abort | `ServiceContext`, `Readiness`, `HealthReporter`, `HealthReceiver`, `ChildScope`, `ChildFailurePolicy`, `ChildTaskFailure`, `ChildScopeError`, `ChildShutdownReport` |
| `graph` | `src/graph.rs` | 648 | Service registration, deterministic topological ordering, full graph validation before startup | `ServiceGraph`, `ServiceGraphBuilder`, `ServiceSpec`, `ServiceFuture`, `RestartPolicy`, `RestartExhaustion`, `RestartPolicyError`, `GraphError` |
| `ntcp2_data_oracle` | `src/ntcp2_data_oracle.rs` | 13.9 K | Local data-phase oracle used by the testkit to drive authenticated frame streams deterministically without owning a real socket | `Ntcp2DataOracle`, `Ntcp2DataOracleConfig`, oracle-side frame/error enums |
| `ntcp2_driver` | `src/ntcp2_driver.rs` | Plan 042 | Runtime-owned handshake action executor with bounded deadlines, cancellation, replay, clock, padding, and RouterInfo provision | `HandshakeDriverConfig`, `HandshakeRun`, `drive_initiator_handshake`, `drive_responder_handshake` |
| `ntcp2_handshake_observer` | `src/ntcp2_handshake_observer.rs` | 5.3 K | Compile-time-gated handshake observer used by the interop harness to record NTCP2 handshake state-machine transitions for diagnostics; never linked into the production daemon | `HandshakeObserver`, `HandshakeObserverEvent`, `ObserverSink` |
| `ntcp2_link` | `src/ntcp2_link.rs` | Plan 042 | Authenticated frame reader/writer children and item/byte accounting leases | `AuthenticatedLink`, `ReceivedFrameLease`, `AuthenticatedLinkSnapshot` |
| `ntcp2_runtime` | `src/ntcp2_runtime.rs` | — | Bounded NTCP2 socket/link lifecycle, TCP listener ownership, admission control, replay cache, dial backoff, link reader/writer children, exact I/O helpers | `Ntcp2RuntimeService`, `BoundNtcp2Listener`, `ListenerHandle`, `LinkHandle`, `InboundAdmission`, `ReplayCache`, `DialAdmission`, etc.; fns `read_exact`, `write_all_exact` |
| `ssu2_runtime` | `src/ssu2_runtime.rs` | 5094 | Plan 158 bounded SSU2 UDP socket/session lifecycle plus Plan 159 path validation/migration and reachability observations: `Ssu2RuntimeService` + `Ssu2ServiceHandle`, cheap receive classification, OS randomness/time fulfillment, pending/active admission, central scheduler, per-session `PathValidator`, `TransportManager` promotion, cached-token dials, bounded I2NP handoff | `Ssu2RuntimeService`, `Ssu2ServiceHandle`, `Ssu2RuntimeConfig`, `Ssu2RuntimeLimits`, `Ssu2RuntimeDeadlines`, `Ssu2DialTarget`, `Ssu2DialOutcome`, `Ssu2SendOutcome`, `Ssu2LinkHandle`, `Ssu2EstablishedLink`, `Ssu2InboundI2np`, `Ssu2Snapshot`, `Ssu2TestFaults`, `Ssu2BindError`, `SSU2_MAX_PATH_CANDIDATES_GLOBAL`, `Ssu2SocketConfig` |
| `ssu2_peer_relay` | `src/ssu2_peer_relay.rs` | Plan 160 | Plan 160 bounded peer-test/relay coordination: `Ssu2PeerRelayService`, table ownership (peer-test, requester, introducer, target, introducer records), per-source rate limits, bounded signer registry, central expiry scheduler, reachability mirroring, privacy-safe snapshots | `Ssu2PeerRelayService`, `Ssu2PeerRelayConfig`, `Ssu2PeerRelaySnapshot`, `PeerRelayAdmission`, `PEER_RELAY_*` bound constants |
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
- `ssu2_runtime` (Plan 158): `Ssu2RuntimeService`, `Ssu2ServiceHandle`,
  `Ssu2SocketConfig`, `Ssu2RuntimeConfig`, `Ssu2RuntimeConfigError`,
  `Ssu2RuntimeLimits`, `Ssu2RuntimeDeadlines`, `Ssu2LimitKind`,
  `Ssu2IdentityMaterial`, `Ssu2DialTarget`, `Ssu2DialTargetError`,
  `Ssu2DialOutcome`, `Ssu2SendOutcome`, `Ssu2LinkHandle`,
  `Ssu2EstablishedLink`, `Ssu2InboundI2np`, `Ssu2Snapshot`,
  `Ssu2TestFaults`, `Ssu2BindError`, `SSU2_MAX_PATH_CANDIDATES_GLOBAL`
  (Plan 159 service-wide candidate ceiling), plus `SSU2_*` bound constants
- `ssu2_peer_relay` (Plan 160): `Ssu2PeerRelayService`,
  `Ssu2PeerRelayConfig` (`introducer_enabled`, default `false`),
  `Ssu2PeerRelaySnapshot`, `PeerRelayAdmission`,
  `PEER_RELAY_RATE_SOURCES`, `PEER_RELAY_DATAGRAMS_PER_SECOND`,
  `PEER_RELAY_MAX_SIGNERS`, `PEER_RELAY_RESPONSE_BUDGET_NUMERATOR`
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
- `ntcp2_data_oracle` (interop harness + testkit): `Ntcp2DataOracle`,
  `Ntcp2DataOracleConfig`, oracle-side frame/error enums. Never linked
  into the production service graph; compile-time gated for the
  testkit and the harness only.
- `ntcp2_handshake_observer` (interop harness diagnostics): `HandshakeObserver`,
  `HandshakeObserverEvent`, `ObserverSink`. Compile-time gated; never
  compiled into the production daemon binary.
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

### SSU2 UDP service (`ssu2_runtime.rs`, Plan 158)
- `Ssu2RuntimeService::new(config, identity)` validates without
  opening a socket; `.start(scope, sockets)` binds loopback UDP
  sockets and spawns one loop task per family under the
  caller-owned `ChildScope`.
- Each loop task is receive classifier plus central scheduler: cheap
  length checks → side-effect-free `matches_inbound` trial → pending
  handshake routing → intro-key prevalidation → admission, then one
  Tokio sleep recomputed to the earliest handshake/ACK/RTO/idle
  deadline. No task per dial/packet, no timer per packet.
- TokenRequest answers and tokenless Retries are stateless (scratch
  responder, no DH); only token-bearing SessionRequests that pass
  admission create bounded pending entries keyed by local connection
  ID. Retry answers respect the 3× amplification budget.
- Outbound dials are single-flight per destination address, hold a
  peer-bound `PendingHandshake`, and promote through the owned
  `TransportManager` (`TransportKind::Ssu2`, duplicate resolution)
  only at the authenticated gate. Cached `NewToken` dials fall back
  to the tokenless path inside the same dial timeout.
- `send_i2np` admits through `delivery_capability` +
  `enqueue_on_link`, then queues into the bounded session outbound
  queue; `Ssu2InboundI2np` messages leave through a bounded channel.
- A handshake resend batch always replaces the pending deadline;
  min-merging with a stale past value burns the retry budget
  (`RetriesExhausted` — caught by the `ssu2_local` suite).
- Local acceptance lives in `tests/ssu2_local.rs` (9 tests, real
  loopback datagrams, serial-safe and parallel-safe).

The independent external acceptance driver lives in
`tests/ssu2_independent.rs`, not in production runtime modules. Its single
Plan 161 test is marked ignored with the reason that exact-pinned i2pd
environment is required. This preserves all-target compilation and ordinary
workspace/macOS executable discovery while preventing a peer-dependent test
from running in routine CI. The dedicated lane must invoke the test with
`--ignored --exact`; absent environment still reaches the driver's hard
`missing required env` failure rather than becoming a skip.

### SSU2 path validation (`ssu2_runtime.rs`, Plan 159)- Every active session owns a `PathValidator` starting at the
  promotion address. `handle_active_datagram` classifies the source
  only after the session authenticated the datagram: validated
  sources flow normally; unknown sources open at most one bounded
  candidate (per-session quotas plus the service-wide
  `SSU2_MAX_PATH_CANDIDATES_GLOBAL` ceiling) with one minimum-MTU
  `PathChallenge` carrying OS-CSPRNG bytes. Nothing migrates on
  source change alone.
- Inbound `PathChallenge` events are answered with one minimum-MTU
  `PathResponse` to the challenger (authenticated packets only, so
  no amplification to unauthenticated victims). Inbound
  `PathResponse` events promote only on an exact tracked-challenge
  match: migration moves the endpoint plus per-IP/subnet accounting,
  resets stale congestion state via `note_path_migrated`, records a
  `ValidatedPath` reachability signal (also mirrored into the manager
  ring buffer), and counts `path_migrations`. Wrong/stale values are
  counted as `path_rejections` without migration.
- `AddressObserved` events record
  `AuthenticatedPeerObservedExternalAddress` signals; the
  `ReachabilityTracker` stays `Unknown` until corroborated (a single
  observation can never publish reachability).
- The central scheduler expires candidates (`path_expirations`,
  retaining the old path) and sleeps to the earliest candidate
  deadline via `PathValidator::next_deadline_ms`.
- `Ssu2Snapshot` adds `path_challenges`, `path_responses`,
  `path_migrations`, `path_rejections`, `path_expirations`,
  `path_denied`, and the conservative `reachability` state (counts
  only, no endpoints).
- Real-UDP acceptance lives inline in `ssu2_runtime.rs`
  (`legitimate_path_migration_over_real_udp`,
  `spoof_burst_from_new_sources_never_migrates`,
  `challenge_response_control_round_trip_over_real_udp`): sealed
  bytes come from the live sessions, cross real loopback sockets
  (including a raw third socket as the new path), and prove
  exact-once migration with behavioral post-migration delivery,
  automatic return migration on genuine re-validation, spoof
  boundedness, and the no-migration control round trip.

### SSU2 peer-test/relay coordination (`ssu2_peer_relay.rs`, Plan 160)
- `Ssu2PeerRelayService::new(config)` owns the five Plan 160 tables
  (`PeerTestTable`, `RelayRequester`, `RelayIntroducer`,
  `RelayTarget`, `IntroducerTable`) plus the router-level
  `ReachabilityTracker`, a per-source sliding rate limiter (8/sec,
  1024 sources, cheap-drop before parsing), and a bounded signer
  registry (128 keys; tests register explicitly, production
  RouterInfo plumbing is Plan 161 debt). The service holds no
  private signing keys: outgoing blocks are signed by caller-supplied
  keys.
- Inbound flow per datagram: `check_admission(source)` →
  protocol-table ingest with trial-commit multi-key verification
  (wrong keys never mutate the peer's test) → typed outcome mirrored
  into reachability as family-only signals (`Confirmed` supports;
  `Mismatch`/`Firewalled` contradict; `Inconclusive`/`Rejected`
  neutral; relay success → `RelayFirewalledSignal`, never direct).
- Introducer methods (`issue_relay_tag`, `on_relay_request`) refuse
  unless `introducer_enabled` (default `false`); the 3x response
  budget is enforced before crypto. `poll_expired`/`next_deadline_ms`
  form the single central scheduler input; `shutdown` returns every
  table, tag, record, and rate window to baseline.
- `Ssu2PeerRelaySnapshot` carries counts plus the conservative
  reachability state (no hashes, nonces, tags, endpoints, or
  signatures); `Debug` for the service and all tables is redacted.
- Real-UDP NAT-like acceptance lives in
  `tests/ssu2_peer_relay.rs` (7 tests, Alice/Bob/Charlie/Target plus
  a test-only rewriting forwarder): direct PeerTest with NAT rewrite,
  mismatch/inconclusive, 40-datagram flood boundedness, the relay
  product path ending in a live `Ssu2RuntimeService` dial with
  bidirectional I2NP, introducer expiry/disabled/shutdown,
  concurrent isolation with crossing schedules, and
  publication/privacy integration. Sealed-session carriage of
  in-session blocks is proven in
  `i2pr-transport-ssu2/tests/peer_relay.rs`; the forwarder moves whole
  datagrams only, never parsed blocks.

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
| `i2pr-crypto` | Transport static secrets, OS randomness (Plan 158) |
| `i2pr-proto` | Router hashes, RouterInfo decode (Plan 158) |
| `i2pr-transport` | Transport contracts (no Tokio) |
| `i2pr-transport-ssu2` | SSU2 handshake/session machines (Plan 158) |
| `rand_core` | `OsRng` fulfillment (Plan 158) |
| `tokio` | Runtime — one of only two crates allowed this |
| `tokio-util` | `CancellationToken` primitive |
| `tracing` | Structured event emission |
| `zeroize` | Local static-secret hygiene (Plan 158) |

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
| `ssu2_runtime.rs` | 7 (4 sync + 3 async real-UDP path) | `limits_validate_rejects_zero_ceiling_and_scope_violations`, `deadlines_validate_ordering_and_bounds`, `dial_targets_validate_before_socket_activity`, `identity_rejects_malformed_router_info`, `legitimate_path_migration_over_real_udp`, `spoof_burst_from_new_sources_never_migrates`, `challenge_response_control_round_trip_over_real_udp` |
| `ssu2_peer_relay.rs` | 7 sync | `introducer_stays_disabled_by_default`, `rate_limiter_cheap_drops_floods_before_crypto`, `peer_test_quotas_and_shutdown_return_to_baseline`, `relay_request_replay_does_not_reamplify`, `snapshot_and_debug_expose_no_secrets`, `next_deadline_tracks_earliest_table`, `unknown_signer_fails_closed_without_state` |
| `tests/ssu2_local.rs` | 9 async | Real-loopback product suite: `tokenless_establishment_over_real_udp`, `cached_token_establishment_with_stale_recovery`, `bidirectional_i2np_exchange_with_fragmentation`, `data_loss_recovers_with_exact_once_delivery`, `ack_loss_reorder_and_duplicate_recover_exactly_once`, `malformed_and_random_traffic_creates_no_state`, `active_session_cap_denies_with_baseline_return`, `graceful_close_abrupt_peer_and_cancel_return_to_baseline`, `inbound_handoff_shape_is_transport_neutral` |
| `tests/ssu2_peer_relay.rs` | 7 async | Real-UDP NAT-like suite: `peer_test_direct_over_real_udp_with_nat_rewrite`, `peer_test_mismatch_and_inconclusive_over_real_udp`, `peer_test_flood_is_cheap_dropped_over_real_udp`, `relay_product_path_over_real_udp_then_normal_handshake`, `introducer_expiry_disabled_and_shutdown_over_real_udp`, `concurrent_peer_tests_stay_isolated_over_real_udp`, `publication_integration_and_privacy_over_real_udp` |
| `supervisor.rs:1267-1703` | 13 | **`forced_child_cleanup_is_repeatably_joined`** (100-iteration, requires `--test-threads=1`), `panic_is_classified_without_payload`, `forced_shutdown_aborts_and_joins_the_owned_child_scope`, `restartable_services_use_bounded_backoff` |

The `ssu2_local` suite uses real time (monotonic handshake/data
deadlines plus wall-clock skew checks) with explicit bounded waits —
never paused Tokio time — and ephemeral loopback ports only.

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
15. **SSU2 routes before it parses** — active sessions match through
   the side-effect-free `matches_inbound` trial; handshake responses
   route by source address under single-flight-per-address dials; only
   then does intro-key prevalidation run. Unknown traffic never
   creates persistent state.
16. **SSU2 resend batches replace deadlines** — a new arm batch
    supersedes the old schedule; min-merging would spin the retry
    budget to `RetriesExhausted`.
17. **SSU2 dials converge on duplicates** — simultaneous inbound and
    outbound promotions resolve through the generic hash-ordered
    duplicate policy, so both sides keep the same session.
18. **SSU2 never migrates on source change alone** — only a matching
    authenticated `PathResponse` promotes a tracked candidate; the
    old path carries traffic until then and survives expiry (Plan
    159).
19. **SSU2 migration requeues, never strands** — the runtime calls
    `note_path_migrated` so unacked fragments retransmit fresh on
    the new path under the existing ceilings (Plan 159).
20. **Peer-test correlation is by nonce, never by source** — the
    relay service keys every transition on the test nonce plus
    role/state, so NAT rewrites and crossing schedules cannot
    confuse tests (Plan 160).
21. **Trial-commit multi-key verification** — out-of-session messages
    try registered signer keys against a cloned table and commit
    only the first success, so a wrong key never mutates a peer's
    test (Plan 160).
22. **Relay success is firewalled-class** — the service mirrors relay
    completion as `RelayFirewalledSignal`, never as direct
    reachability (Plan 160).

## Cross-references

- [Overview](overview.md)
- [i2pr-core](i2pr-core.md) — provides the runtime-neutral types
  this crate specializes.
- [i2pr-transport](i2pr-transport.md) — contract surface driven
  from supervised services.
- [i2pr-transport-ntcp2](i2pr-transport-ntcp2.md) — produces
  `HandshakeAction` / `FrameAction` requests fulfilled here.
- [i2pr-transport-ssu2](i2pr-transport-ssu2.md) — produces SSU2
  `HandshakeAction` / `SessionAction` / `SessionEvent` values
  fulfilled here (Plan 158).
- Plan-of-record: `plans/021-m2-supervision-cancellation.md`,
  `plans/022-m2-bounded-channels-resource-governor.md`,
  `plans/035-m3-runtime-link-manager-and-addresses.md`,
  `plans/037-m3-corrective-integration-closure.md`,
  `plans/158-m8-ssu2-udp-runtime-and-local-session-product.md`,
  `plans/159-m8-ssu2-path-validation-publication-and-transport-selection.md`,
  `plans/160-m8-ssu2-peer-test-and-relay-reachability.md`.
- Closures: `plans/021-closure.md`, `plans/022-closure.md`,
  `plans/035-closure.md`, `plans/037-closure.md`,
  `plans/158-status.md`, `plans/159-status.md`,
  `plans/160-status.md`.
