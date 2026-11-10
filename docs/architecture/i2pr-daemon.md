# `i2pr-daemon` — Deep Dive

**Crate:** `i2pr-daemon` — **Path:** `crates/i2pr-daemon` — **Binary:** `i2pr` (`src/main.rs`)
**Size:** 77 `.rs` files, 134 855 lines of `src` (71 at the crate root + 5 under `src/sam/` + 1 under `src/tests/support/`).**Size:** 79 `.rs` files, 135 808 lines of `src` (73 at the crate root + 5 under `src/sam/` + 1 under `src/tests/support/`).
**Lints:** workspace-inherited; the workspace denies `unsafe_code`, `clippy::dbg_macro`,
`clippy::todo`, and `clippy::unimplemented`.

**One-line purpose:** the composition root and the only production owner of Tokio, sockets,
timers, channels, and **every** listener in the router.

Authority order for this document (highest first): executable tests/scripts > ADRs >
prose. `plans/closure/*/*-status.md` **wins** over `plans/registry.md`, and both win over
`specs/support.toml`. See [`plans/README.md`](../../plans/README.md).

---

## Purpose

`i2pr-daemon` sits at the top of the dependency graph and sees every crate that will
eventually participate in the running daemon. It is the process shell, not a protocol
implementation.

### What the daemon owns

- **CLI parsing** (`src/cli.rs`, 69 lines) via `clap` derives: subcommands, flags, `--help`.
- **Configuration** (`src/config.rs`, 6 786 lines): strict versioned TOML, `deny_unknown_fields`
  on all 22 `Raw*` structs, semantic validation, and normalization.
- **Identity lifecycle**: explicit generation and inspection of `<data_dir>/router.identity`.
  No auto-generated side effects on `run --dry-run`.
- **Identity -> NetDB bootstrap** (`bootstrap_daemon`, `src/lib.rs:5166`): identity load,
  cache revalidation, local RouterInfo construction, readiness, one optional bounded reseed.
- **Service-graph composition** (`src/lib.rs`): `i2pr_runtime::ServiceGraph`,
  `i2pr_runtime::Supervisor`, service lifecycles, and graceful shutdown.
- **ALL listeners.** SAM 3.1–3.3 (`sam.rs`), I2CP (`i2cp.rs`), I2PControl (`i2pcontrol.rs`,
  TLS + JSON-RPC), service-tunnel per-profile executors, the loopback router console
  (`console.rs`, EggServe), and the SSU2 `ssu2-router` service.
  Every `TcpListener` / `UdpSocket` / `tokio::spawn` in production lives here or in
  `i2pr-runtime`; the boundary is enforced by
  [`scripts/check-runtime-boundaries.sh`](../../scripts/check-runtime-boundaries.sh).
- **The router console listener and its control principal** (`console.rs`,
  `i2pcontrol_dispatch.rs`). The console *substrate* is in `i2pr-console`; the daemon
  owns the socket, the bind policy, the `ControlDispatcher` the console reads from,
  and the closed read-only allow-set that keeps the console off `TunnelManager`
  and `AddressBook`. Disabled by default, loopback-only, non-advertised.
- **All task/queue/channel ownership.** Every spawned task has explicit ownership and
  cancellation; channel and socket close are lifecycle events, never blind retries.
- **The M10 service-tunnel manager** (`service_tunnels.rs`, 9 342 lines) and every per-profile
  executor.
- **The outproxy route owner** (`outproxy_route.rs`, 976 lines) — the daemon half of Proposal
  170's I2P-routed outproxy: it opens the Streaming route and recovers the credential, while the
  policy stays in the runtime-neutral `i2pr-service-tunnels`. Reachable from tests only; no
  request path consults it.
- **M11 transit composition** (`transit_compose.rs`, `transit_owner.rs`, `transit_volume.rs`,
  `transit_composition.rs`) — the **disabled** posture by default; the owner is installed in the
  production composition behind the operator's default-off `[transit] participation` opt-in
  (Plan 502).
- **M12 floodfill** (`floodfill.rs`) — controlled-activation/withdrawal composition only.
- **Address book** (`addressbook.rs`, `addressbook_fetch.rs`, `control_sources.rs`): refresh
  tasks, timers, bounded download composition, and resolver-handle installation into SAM,
  service-tunnel, and I2PControl consumers.
- **NEWS** (`news.rs`, private module) handling.
- **Stable process exit codes** (`error.rs`) that operators and automation can rely on.

### What the daemon must NOT own

Protocol semantics live in the runtime-neutral crates and stay there:

- No I2NP/LeaseSet2/Streaming wire codecs — `i2pr-proto`.
- No crypto primitives — `i2pr-crypto` (the daemon only calls wrappers such as
  `hkdf_sha256_extract_and_expand` and `red25519::derive_public_key`).
- No tunnel-build or tunnel-data-plane algorithms — `i2pr-tunnel`.
- No RouterInfo/LeaseSet2 validation or store semantics — `i2pr-netdb` / `i2pr-netdb-persist`.
- No SAM/I2CP/I2PControl wire state machines — `i2pr-api` / `i2pr-i2pcontrol`.
- No HTTP/SOCKS5/IRC/Streamr/TLS parsing — `i2pr-service-tunnels`. That includes the whole
  runtime-neutral outproxy **policy** (dialects, endpoint/target grammar, rotation, bounded
  retry, codecs); the daemon contributes only the socket and credential halves.
- No destination lifecycle or ECIES session logic — `i2pr-client`.
- No SU3 framing — `i2pr-su3`.
- No runtime/transport/runtime-neutral contracts — `i2pr-runtime`, `i2pr-transport`.
- No console HTML, CSS, theme parsing, asset table, session store, or Argon2id
  verification — `i2pr-console`. The daemon contributes the listener and the control
  side only; it does not re-implement any console rendering or password checking.
  Equally, `i2pr-console` reaches no router state and opens no socket: every value it
  renders arrives through its `ControlClient` trait.

NTCP2 remains **DISABLED** by default: `default_ntcp2_enabled() == false`
(`src/config.rs:1018`). It is not unconditionally enableable — an enabled
NTCP2 surface must name an explicit qualification profile, and only the two
authorized profiles (`controlled_loopback`, `private_lan_qualification`) are
accepted; every other shape is refused by name. The `ntcp2-transport` service
from the transport crate is still never registered: the daemon owns its own
`ntcp2-router` service instead. `i2pr-transport-ntcp2` and
`i2pr-transport-ssu2` are **not** direct daemon dependencies.

---

## Module layout

64 files at the crate root plus `sam/` (5 files) and `tests/support/` (1 file). Line counts are from
`wc -l` on repo head. The tables below cover 63 of the crate's 70 `src/` files; seven rows are
still owed by the plans that added those modules, and the gap is recorded rather than papered over.

### CLI and configuration

**Plan 360 (2026-10-06): `i2pr run` starts the router.** `run_daemon` runs the bounded
bootstrap pipeline synchronously, builds the graph, and drives the supervisor until
SIGINT. A configured loopback listener is bound and the process stays up:

```text
cargo run --locked -p i2pr-daemon -- run --config <cfg>     # stays up until Ctrl-C
```

The previously recorded defect — `error: supervisor terminated: supervisor failed:
service lifecycle failed during startup: ReadinessTimeout`, with no listener opened —
was caused by every service body awaiting cancellation without ever reporting
readiness, so the supervisor's 30-second readiness deadline fired on `lifecycle`
(Essential) and tore the graph down before `sam-bridge` started.

**Readiness contract (Plan 360, shape b).** Readiness means **"this service is
running"** — never "some task finished" and never "we hope". `readiness_expectation()`
in `src/lib.rs` is the single source of truth: it is installed as each
`ServiceSpec::description` and quoted in the startup diagnostic.

| Service | Classification | Signals readiness when |
| --- | --- | --- |
| `lifecycle` | Essential | immediately — it owns no work; a cancellation-scoped lifetime anchor is up when scheduled |
| `netdb-bootstrap` | Essential | immediately — the bootstrap pipeline already ran in `run_daemon` before the supervisor was constructed, and a router with an empty NetDB is still running |
| `sam-bridge`, `i2cp-bridge`, `i2pcontrol`, `router-console` | Optional | **after** the loopback bind succeeds, so a ready service provably owns a bound socket |
| `addressbook-refresh`, `signed-news-refresh` | Optional | once the cadence loop is established — deliberately not gated on a remote fetch, whose failure is non-fatal |
| `ssu2-router` | Optional | after its controlled owner is up (pre-existing) |

Listener services bind under a bounded 1s deadline (`LISTENER_BIND_DEADLINE`) and then
serve **unbounded** until cancellation. A timeout wrapped around the serving phase
would abort a healthy listener rather than start one — that is the shape the pre-Plan-360
code used, and the console service already avoided it.

Every service carries explicit bounded deadlines (`SERVICE_STARTUP_DEADLINE`,
`SERVICE_READINESS_DEADLINE`, `SERVICE_SHUTDOWN_GRACE`). When readiness genuinely cannot
be established, `describe_supervisor_error()` names the service, both deadlines, the
reason, and what that service was awaiting:

```text
error: supervisor terminated: supervisor failed: service `sam-bridge` failed during startup: sam-bridge bind failed: failed to bind SAM listener on 127.0.0.1:17657: Address already in use (os error 98) (startup deadline 30s, readiness deadline 30s; readiness means loopback SAM listener bound and accepting)
```

No listener default changed: SAM/I2CP/I2PControl/service-tunnels/console remain
loopback-only, disabled by default, and non-advertised. `run --dry-run`,
`check-config`, `identity generate`, and `identity inspect` are unchanged. The
`sam_loopback_listener` example still binds an ephemeral loopback port and prints
`{"port":NNN,"pid":PPP}`, but it is no longer a workaround for the product path.
Evidence: `crates/i2pr-daemon/tests/run_lifecycle_readiness.rs` (black-box through the
CLI binary) plus four unit tests over the diagnostic formatter.

| File | Lines | Responsibility | Key public types |
| --- | --- | --- | --- |
| `src/main.rs` | 68 | Binary shell: `Cli::parse()`, dispatch through `execute()`, print results, map errors to stable exit codes via `i2pr_runtime::run_blocking` on the live `run` path | `main()`, `process_exit()`, `_command_name()` |
| `src/cli.rs` | 69 | `clap` CLI vocabulary only — no logic | `Cli`, `Command`, `IdentityCommand`, `CheckConfigArgs`, `IdentityArgs`, `RunArgs` |
| `src/config.rs` | 7 068 | Strict versioned TOML: 22 `Raw*` structs with `deny_unknown_fields`, semantic validation, normalization, `bind_socket()` / `loopback_test_profile()` helpers. Plans 356–358 add the `[console]` section, its redacting `Debug`, and `normalize_console` (loopback-only enforcement, bundled-theme validation, enabled-gated runtime ceilings, Argon2id PHC validation at parse time). The `[transport.ntcp2].qualification_profile` surface accepts `disabled`, `controlled_loopback`, and `private_lan_qualification`, each with its own per-reason refusals | `Config`, `RouterConfig`, `LoggingConfig`, `LimitsConfig`, `NetworkConfig`, `Ntcp2Config`, `Ntcp2QualificationProfile`, `TransportConfig`, `NetDbConfig`, `ReseedConfig`, `ReseedSourceConfig`, `NewsConfig`, `SamConfig`, `Ssu2Config`, `I2cpConfig`, `I2pControlConfig`, `I2pControlPassword`, `FloodfillConfig`, `ServiceTunnelsConfig`, `RouterProfile`, `LogFormat`, `ConfigError`, `CURRENT_SCHEMA_VERSION`, plus `ConsoleConfig`, `ConsolePasswordHash`, `RawConsoleConfig` |
| `src/ntcp2_activation.rs` | 307 | The profile-aware NTCP2 activation gate. A **pure** function over an already-normalized `Ntcp2Config` that returns the address scope the transport may dial, or a typed refusal. Extracted from the service closure so that every refusal branch is reachable by a unit test — the configuration layer already refuses every shape the gate refuses, so an in-closure branch could never be reached from a configuration file | `ntcp2_activation_scope()`, `Ntcp2ActivationRefusal`, `MIN_NTCP2_ACTIVATION_PORT` |
| `src/lib.rs` | 6 578| Crate root: module declarations, `pub use` re-exports, `execute()` dispatch, logging init, bootstrap, service-graph construction, `run_daemon()`. Plan 360 adds the readiness contract (`readiness_expectation`, `service_spec`, `signal_running`, `bind_then_serve`) and the startup diagnostic (`describe_startup_failure`, `describe_supervisor_error`). The NTCP2 activation gate is called from the composition root and from the `ntcp2-router` service closure, so there is one authority for the decision. **Plan 514** makes the operator's exploration switch real: `NetDbParticipation` carries `exploration_enabled`, read from `netdb.exploration_enabled`, and `drive_discovery` consults it before it starts an exploration — after the RouterInfo-kind resolution, so the routing table keeps its production reader  **Plan 515** adds the router's **own** RouterInfo refresh: the SSU2 service's loop carries a `publication_tick` beside the relay tick, and the refresh republishes the router's record on the reference's interval — it is the **router's own**, so the publication no longer depends on a service tunnel being provisioned  **Plan 516** joins the **inbound owner** to the publication: the same loop that reads the inbound stream correlates an inbound `DeliveryStatus` with the attempt it names, so the router learns that the **network holds** its record -- a different fact from the transport having accepted the store  **Plan 517** makes the publication a **cycle**: the periodic tick **begins** one, and the loop's existing `100` millisecond tick **advances** it when the awaited confirmation's deadline passes -- so the retry introduces **no new timer**  **Plan 518** makes the published record **current**: the publication tick re-dates and re-signs the router's own RouterInfo **once per cycle**, installs the fresh record so the RouterInfo handed out in `SessionConfirmed` carries the fresh date too, and publishes it -- and the relay tick, which asks the cycle's later batches, does not re-date. The **cycle's record** lives in the owner, so every rung publishes the same bytes: the relay tick does not read the installed record back  **Plan 519** moves the publication onto its **own task**: the owner, the transport capability, the identity and the acknowledgement inbox all move into `run_router_info_publication_task`, which the composition root spawns under the SSU2 service's own child scope with an explicit cancellation. The service's `select!` loop keeps **none** of the publication's state and never awaits it; the inbound `DeliveryStatus` crosses a **bounded** channel instead, and a full channel counts the acknowledgement as unmatched rather than dropping it silently | `CommandOutcome`, `IdentitySummary`, `execute()`, `initialize_logging()`, `bootstrap_daemon()`, `build_daemon_graph()`, `build_daemon_graph_with_inspection()`, `build_shared_service_manager()`, `run_daemon()` |
| `src/error.rs` | 128 | Typed error hierarchy and the stable exit-code mapping | `ExitCode` (`#[repr(u8)]`), `DaemonError` |

### Identity and bootstrap

| File | Lines | Responsibility | Key public types |
| --- | --- | --- | --- |
| `src/bootstrap.rs` | 887| Plan 106 bounded NetDB bootstrap state machine, cache revalidation, bounded offline SU3 reseed ingestion, trust-set loading. Owns no runtime, sockets, or tunnels. **Plan 513** adds `store_mut`, the narrow mutating accessor the discovery spine's RouterInfo-kind lookup writes a resolved record through | `Bootstrap`, `BootstrapState`, `BootstrapPolicy`, `BootstrapSnapshot`, `BootstrapReport`, `ReseedAttemptSummary`, `BootstrapError`, `CacheLoaderReport`, `ReseedBundleReport` |

> `bootstrap_daemon` itself lives in `src/lib.rs:5166`, not in `bootstrap.rs` — it is the
> composition-root wrapper that loads the `IdentityStore`, builds `LocalRouterInfoBuilder` and
> `RouterInfoStoreConfig`, then calls `Bootstrap::run`.

### NetDB, discovery, and routing seams

| File | Lines | Responsibility | Key public types |
| --- | --- | --- | --- |
| `src/netdb_participation.rs` | 3 479| Plan 488 peer participation driver: feeds the Plan 488 profile store with observed peer outcomes, runs the maintenance pass on the daemon's clock, and emits sanitized receipts. **Not** wired into the router's tunnel building in this phase. **Plan 489** extends the same module with the routing-table driver: it indexes the floodfills the router learns, runs the bounded exploration job, drives the optimized lookup from the routing table, and emits sanitized routing receipts. **Not** wired into the router's tunnel building in this phase either. **Plan 497** adds the `RoutingTableConsumer` — the shared, mutex-owned handle the lookup path consults, whose `candidates(...)` answers `Table(Vec<RouterHash>)` or `Fallback(RoutingTableFallback)`, and whose `RoutingTableCounters` count every fallback by reason. **Plan 513** adds the **`NetDbDiscoverySpine`** — the bounded owner of the router's DHT discovery — with its `DiscoveryOutcome`, `DiscoveryError`, `DiscoveryDelivery`, `DiscoveryDispatch` and `NetDbDiscoveryCounters`; the spine owns no task, no timer, no listener and no socket, and the daemon's existing tick drives it. **Plan 514** adds `RoutingTableConsumer::closest_floodfill` — the table's closest-first answer, used as a **target** rather than a preference — and the spine's exploratory start asks the table for it, so the peer the request is addressed to is the closest floodfill rather than the flat store's first advertiser; a table that names none ends the request, counted under `explorations_refused_no_target` | `PeerParticipationDriver`, `PeerParticipationHandle`, `ParticipationReceipt`, `ParticipationReceiptKind`, `ProjectionLoadOutcome`, `ProfileMaintenanceExit`, `run_profile_maintenance`, `RoutingTableDriver`, `RoutingTableConfig`, `RoutingReceipt`, `RoutingReceiptKind`, `RoutingDispatch`, `RoutingLookupOutcome`, `RoutingDiscovery`, `RoutingProjectionOutcome`, `RoutingTableConsumer`, `RoutingTableCandidates`, `RoutingTableFallback`, `RoutingTableCounters` |
| `src/router_info_publication.rs` | 1 396 | **Plan 515** the router's **own** RouterInfo publication: the runtime-neutral schedule (a period, a due test, the counted outcomes), the **shared** selection rule -- the version-gated function, so Plan 503's API-version floor holds for every publication -- the shared `DatabaseStore` construction, and the **one** delivery path the service product's one-shot publication and the router-level refresh both use. The schedule never reads a clock: it takes `now_ms` from its caller. **Plan 516** makes the publication **confirmable**: the store message's construction moves to the correlator, so the minted correlation token reaches the wire, and the module gains the runtime-neutral confirmation seam (`confirm_publication`) and the attempt book the owner holds. A **delivery** admission and a **network acknowledgement** are counted separately. **Plan 517** adds the **cycle**: a bounded ladder of batches, the cycle's exclusion set, the reference's `1600` millisecond confirmation deadline, and the retry that follows an unacknowledged batch. The cycle is runtime-neutral -- it takes the reading from its caller through a `PublicationClock` and never reads a clock itself. **Plan 518** adds the publication's **staleness rule**: `record_is_publishable` is runtime-neutral, takes the reading from its caller, and uses the netdb's **own** acceptance window -- so a record the network would refuse is refused and counted (`refused_stale_record`) rather than published. **Plan 518** also gives the owner the **cycle's record**: the bytes every rung of the cycle publishes, set once at the cycle's start, so a rung cannot publish a different record from the one the correlator registered  **Plan 519** adds the **established-session preference** (`prefer_established_sessions`) -- the batch prefers a peer the router is already connected to, as i2pd's `IsConnected` and Java's `isEstablished` both do -- and counts the targets it skipped for want of one (`skipped_no_session`). It adds the publication's own **bounded per-attempt delivery timeout** (`PUBLICATION_DELIVERY_TIMEOUT_MS`, the reference's 90 seconds), so a hung handshake holds neither the transport's loop nor the publication's task, and a delivery that did not finish inside it is refused under its own reason (`deliveries_timed_out`) | `RouterInfoPublicationOwner`, `RouterInfoPublicationSchedule`, `RouterInfoPublicationOutcome`, `RouterInfoPublicationCounters`, `RouterInfoPublicationPlan`, `RouterInfoPublicationCycle`, `RouterInfoPublicationStep`, `PublicationConfirmation`, `PublicationClock`, `publication_targets`, `publication_targets_excluding`, `begin_attempts`, `confirm_publication`, `publish_router_info`, `refresh_router_info`, `advance_publication_cycle` |
| `src/router_info_publication_task.rs` | 162 | **Plan 519** the publication's **bounded acknowledgement handoff**: the transport's service loop hands the inbound `DeliveryStatus`'s message id to the publication's task over a **bounded** channel and never awaits it, so the loop is never blocked by the publication. A full channel counts the acknowledgement as **unmatched** -- the refusal Plan 516 already defines -- rather than dropping it silently, and the task folds the refusals into the publication's own counters on every wake. The module also carries the task's inputs: the owner, the transport capability, the identity, the store handle and the inbox, so the transport's loop holds **none** of the publication's state | `PublicationAcknowledgementHandoff`, `PublicationAcknowledgementInbox`, `RouterInfoPublicationTask`, `publication_acknowledgement_channel`, `PUBLICATION_ACKNOWLEDGEMENT_CAPACITY`, `PUBLICATION_LADDER_TICK_MS` |
| `src/self_publication.rs` | 462 | **Plan 520** the owner of the router's **own** published SSU2 address: the operator's opt-in, the hand-off of the transport's address to the netdb's own qualified builder, the install, the **withdrawal** when the evidence lapses, and the count of every outcome under its own reason. It never builds a snapshot, never writes an address's options and never records a reachability signal of its own: the **transport's** `build_publication_snapshot` is the single authority on whether an endpoint may be published | `SelfPublicationOwner`, `SelfPublicationDecision`, `SelfPublicationOutcome`, `SelfPublicationCounters`, `SelfPublicationCounterSnapshot`, `StandDownReason`, `SelfPublicationFailure` |
| `src/netdb_seam.rs` | 1 592| Plan 106/107 runtime-facing seam over the Plan 105 lookup state machines; Plan 117 composition outcomes; Plan 122 dedicated LeaseSet2 lookup state machine and separate reply-path provider; **Plan 497** adds `begin_lookup_with_preferred`, which forwards a routing table's preferred peer list to the engine's `start_with_preferred` entry point. **Plan 513** adds the two live starts the discovery spine drives — `begin_routerinfo_lookup` (wire type `2`, the candidate list reordered by the table's preference and never narrowed) and `begin_exploratory_lookup` (wire type `3`) — plus `ingest_routerinfo_store`, `ingest_exploratory_search_reply`, `set_routerinfo_now_seconds` and `policy` | `NetDbSeam`, `NetDbSeamError`, `CompositionOutcome`, `ExploratoryPathStatus`, `LeaseSet2ResponseOutcome`, `begin_lookup_with_preferred` |
| `src/netdb_tunnels.rs` | 1 180| Plan 186 NetDB-over-exploratory-tunnels coordinator: authoritative bounded store, ordinary-path reference bootstrap, floodfill verification, tunnel-path proofs, bounded lookup/publication/search matrices, typed tunnel loss; **Plan 491** refuses a DSM whose floodfill target is below the DSM API-version floor; **Plan 497** adds `with_routing_table`, which injects the shared `RoutingTableConsumer` so `begin_tunnel_lookup` can ask the table how it would order the flat store's selection. **Plan 513** factors the composition into the free function `compose_lookup_via_outbound_tunnel`, which the coordinator's `compose_lookup_via_tunnel` now delegates to with byte-identical behaviour, so the live discovery spine reaches the canonical composition without constructing the never-composed coordinator | `NetDbTunnelCoordinator`, `NetDbTunnelError`, `NetDbTunnelCounters`, `TunnelPathProof`, `PublicationPathProof` |
| `src/outbound_lookup.rs` | 980 | Plan 117 §8/§10 outbound exploratory data-plane composition; Plan 187 `deliver_outbound_cells` for client-composed Garlic cells | `compose_outbound_lookup`, `compose_outbound_publication`, `deliver_outbound_cells`, `encode_standard_envelope`, `encode_store_envelope`, `OutboundLookupDispatch`, `OutboundLookupError`, `MAX_OUTBOUND_LOOKUP_CELLS = 8`, `MAX_OUTBOUND_PUBLICATION_CELLS = 8` |
| `src/inbound_dispatch.rs` | 290 | Plan 117 §9 inbound exploratory `TunnelData` dispatch; Plan 187 `GarlicComplete` outcome for destination carriers | `dispatch_inbound_tunnel_data`, `route_databasestore`, `route_database_search_reply`, `InboundDispatchOutcome`, `InboundResponseKind`, `InboundDispatchError`, `MAX_RECOVERED_ENVELOPE = MAX_I2NP_PAYLOAD_SIZE` |
| `src/destination_peers.rs` | 681 | Bounded projection and selection of validated Destination build peers from the validated RouterInfo store; **Plan 491** refuses a build peer below the tunnel-build API-version floor and fails closed with the reason | `DestinationPeerCandidate`, `DestinationSelectionError`, `CandidateRefusal`, `CandidateProjectionSummary`, `project_validated_store`, `select_destination_path`, `select_destination_path_os`, `select_projected_destination_path`, `select_projected_destination_path_os` |
| `src/peer_test.rs` | 417 | Plan 285 bounded I2NP peer testing: echo a `(message id, timestamp)` unchanged, match the echo against an outstanding probe, derive RTT from the supplied timestamp | `PeerTestTracker`, `PeerTestEcho`, `PeerTestOutcome`, `PeerTestError`, `MAX_OUTSTANDING_PEER_TESTS = 32`, `DEFAULT_PEER_TEST_TIMEOUT_MS = 10_000` |

### Tunnels, transit, and floodfill

| File | Lines | Responsibility | Key public types |
| --- | --- | --- | --- |
| `src/router_i2np.rs` | 3 874 | Plan 184 central authenticated router-I2NP dispatcher, narrow router delivery, daemon-owned `ssu2-router` service, controlled identity generation, reference RouterInfo verification. The dual-transport RouterInfo builders are scope-parameterized: `generate_scoped_dual_transport_router_info` carries both transports' endpoints inside one address scope, and the controlled-loopback and private-LAN forms delegate to it | `dispatch_router_i2np`, `RouterI2npOutcome`, `RouterDeliveryService`, `Ssu2DaemonService`, `Ssu2DaemonHandle`, `generate_controlled_identity`, `generate_controlled_dual_transport_router_info`, `generate_private_lan_dual_transport_router_info`, `generate_scoped_dual_transport_router_info`, `verify_reference_router_info`, `refresh_local_router_info_record` |
| `src/exploratory_build.rs` | 4 698| Plans 185/188/314 short-build owner plus Plan 390's explicit bounded mixed-role registry capacity: exploratory limits plus at most 32 service groups × 8 retained inbound roles and one transient outbound activation. **Plan 513** adds `exploratory_reply_path` (the pool's oldest valid **inbound** tunnel as the reply path a discovery request carries) and `exploratory_outbound_role` (the pool's usable **outbound** role), both read from the pool's own registrations so a destination-role slot is never returned | `ExploratoryBuildCoordinator`, `BuildRequest`, `BuildDirection`, `PeerBuildMaterial`, `BuildCoordinatorOutcome`, `BuildCoordinatorCounters`, `SubmitResult`, `InboundRouteOutcome`, `tunnel_state_at`, `next_creator_tunnel_id_value` |
| `src/tunnel_liveness.rs` | 707 | Plan 185 bounded creator-side tunnel liveness scheduler: first test, repeat interval, response timeout, and failure threshold all bounded below the two-minute idle deletion boundary; one central scheduler, no per-tunnel task or timer | `TunnelLivenessScheduler`, `LivenessConfig`, `LivenessAction`, `LivenessTestId`, `LivenessCounters`, `LivenessError`, `route_inbound_with_liveness`, `first_due_after`, `repeat_interval`, `response_timeout` |
| `src/exploratory_driver.rs` | 1 778 | **Plan 505** exploratory build driver: the router names a peer through Plan 491's version-gated selection, builds a one-hop exploratory tunnel toward it through the EXISTING `rebuild_exploratory` path, and records exactly one peer-level observation per terminal outcome -- a local condition produces none, so the router never blames a peer for a fault of its own | `PeerBuildIdentity`, `PeerMaterialError`, `PeerBuildIdentityResolver`, `ProfileStorePeerResolver`, `PeerObservation`, `observation_for_outcome`, `ExploratoryDriverCounters`, `ExploratoryDriverAction`, `SelectedPeer`, `ExploratoryDriverState`, `ExploratoryDriver` |
| `src/peer_reconcile.rs` | 933 | **Plan 506** peer-knowledge reconcile: the product-owned, bounded operation that carries the bootstrap's validated RouterInfos into the two stores the router **builds with** -- the destination coordinator's authoritative store and the peer-profile store. The three locks are taken in one order and never nested; every outcome is counted by reason and a capacity refusal is counted, never fatal; the reconcile writes peer **records**, never a selection, so Plan 491's version gate stays at selection | `PeerReconcile`, `PeerReconcileCounters`, `PeerReconcileAction`, `PeerReconcileError`, `MIN_RECONCILE_PERIOD_MS`, `MAX_RECONCILE_PERIOD_MS`, `RETENTION_WINDOW_MS` |
| `src/destination_tunnels.rs` | 2 152 | Plan 187 destination LeaseSet2/Garlic-over-tunnels coordinator; Plan 190 typed `InboundGatewayRoute` -> `i2pr_netdb::ReplyPath` adapter; Plan 201 §G sanitized boundary observation counters | `DestinationTunnelCoordinator`, `DestinationTunnelError`, `DestinationTunnelCounters`, `DestinationTunnelPathProof`, `RemoteMaterialProof`, `RemoteLeaseSummary`, `LeaseStoreIngestOutcome`, `reply_path_for_inbound_route`, `ReplyPathDerivationError`, `note_lookup_boundary` |
| `src/transit_compose.rs` | 4 231 | Plan 253 M11 transit composition: message-level STBM dispatch, role-specific STBM/OTBRM routing, `TunnelData` previous-peer forwarding, expiry/cancellation, delivery-failure rollback, creator-correlation bypass. **Disabled by default** (`TransitIngressGate::disabled()`) | `TransitBuildService`, `TransitIngressGate`, `TransitDispatch`, `TransitTunnelDataDispatch`, `TransitCounters`, `TransitServiceError` |
| `src/transit_owner.rs` | 1 671 | M11 live transit owner: build/disposition evidence types, OBEP delivery, live gateway/inbound outcomes, and the controlled-disabled probe that is the composition's **no-owner fallback** (Plan 502). Plan 508 adds the `CreatorFacts` wrapper that holds the creator-correlated facts with their expiries | `TransitOwner`, `TransitLiveOwner<R>`, `TransitOwnerError`, `TransitLiveError`, `TransitDataDisposition`, `TransitDataForwardEvidence`, `ObepDeliveryOutcome`, `LiveBuildOutcome`, `LiveGatewayOutcome`, `LiveInboundOutcome`, `TransitBuildEvidence`, `controlled_transit_disabled_probe` |
| `src/transit_composition.rs` | 668 | **Plan 502** M11 transit **product composition**: the derivation of the admission policy, hop material and participation binding from the normalized configuration, the owner's construction behind the operator's default-off `[transit] participation` opt-in, the inbound dispatch that makes the whole owner/no-owner choice internally, and the bounded peer-index reconcile against the live session set | `TransitCompositionInputs`, `TransitAdmissionDerivation`, `TransitCompositionError`, `derive_admission_policy`, `build_transit_owner`, `compose_transit_owner`, `dispatch_transit_inbound`, `reconcile_transit_peers` |
| `src/transit_volume.rs` | 450 | Plan 340 transit volume, trailing-window bandwidth, and the authoritative **participation-posture owner** behind Proposal 170's three remaining RouterInfo selectors | `TransitParticipation` (`Disabled` default / `Enabled(Arc<Mutex<TransitVolumeCounters>>)`), `TransitVolumeCounters`, `TransitVolumeSnapshot` |
| `src/floodfill.rs` | 3 089 | M12 floodfill delivery boundary and the controlled activation/withdrawal composition: resource leases through effect completion, fresh bounded I2NP envelopes, Store acks, direct/tunnel lookup replies, `run_floodfill_owner` (bounded ingress + maintenance + effect drain) | floodfill owner/coordinator types, `activate_controlled`, `withdraw_controlled` |
| `src/ssu2_introducer.rs` | 1 047 | Plan 490 SSU2 introducer (Bob) role owner: relay-tag issuance, the per-source rate limit, the target resolution through the tag, the requester's signature verification against a validated RouterInfo, the typed outcome, and the counts-only receipt. **Plan 498** adds `NetDbSignerResolver`, the production `IntroducerSignerResolver` that answers only from the NetDB's validated store. **Plan 499** adds the `Ssu2RelayEvent::Tag(_)` refusal: a tag the runtime observed is not the role owner's to act on, exactly as an `Intro` is | `Ssu2IntroducerService`, `Ssu2IntroducerConfig`, `Ssu2IntroducerOutcome`, `Ssu2IntroducerRefusal`, `Ssu2IntroducerReceipt`, `IntroducerSignerResolver`, `NetDbSignerResolver` |
| `src/ssu2_relay_drain.rs` | 215 | Plan 498 bounded drain from the runtime's relay handoff into the introducer (Bob) role owner. A **pull** seam (`RelayEventSource`), one pass of at most `MAX_RELAY_DRAIN_PER_PASS` (16) events, and a counts-only outcome. It never fails its caller and never logs a peer hash, a key, a tag, or a payload. **Plan 499** adds a **carriage** seam (`RelayCarriage`) beside the pull seam and the production `RuntimeRelayCarriage` over the runtime that owns the sessions, so a produced block is carried onto the wire instead of counted and dropped; `awaiting_carriage` becomes `carried` plus `carriage_refusals` by reason | `drain_relay_events`, `RelayEventSource`, `RelayCarriage`, `RuntimeRelayCarriage`, `RelayDrainOutcome`, `MAX_RELAY_DRAIN_PER_PASS` |
| `src/ssu2_qualification_receipts.rs` | 283 | Plan 480 private local qualification readout: the permission-gated, pure projection of one SSU2 observer epoch into the Plan 476 local-observed phase vocabulary. Carries phase labels and counts only — no key, RouterInfo, address, endpoint, payload or transient message ID — and `live_qualification` stays `"not-run"`. **Plan 476** adds the `PrivateLan` read scope, the direction-preserving session phases and `request_sent`, and the `reply_correlated` boolean | `LocalQualificationProfile`, `LocalQualificationPermit`, `LocalObservedReceipt`, `LocalReceiptCollector`, `LOCAL_PHASES` |
| `src/ssu2_qualification_export.rs` | 411 | Plan 476 sanitized local receipt export: writes the count-only projection of the local observer epoch to an operator-designated file (`[ssu2] qualification_receipt_path`, absolute path, parent directory must exist, mode `0600`). Opt-in, never a listener, never a publication permit; a collector refusal is recorded and is **sticky** for the life of the process | `LocalReceiptExporter`, `EXPORT_SCHEMA` |

### Destinations and the shared streaming pump

| File | Lines | Responsibility | Key public types |
| --- | --- | --- | --- |
| `src/destination_streaming.rs` | 708 | Plan 174 **shared** daemon Streaming byte pump (`run_stream_pump`, generic over `AsyncRead + AsyncWrite` with bounded chunk, negotiated segmentation, backpressure, sibling-isolated drain, cancel/EOF/terminal convergence). Reused by SAM and every service-tunnel executor — there is no second byte pump | shared pump types |
| `src/service_delivery.rs` | 819 | Plan 202/206 M10 production remote Destination/Streaming delivery capability: typed `RoutingDecision`, bounded `RemoteDeliveryCounters`, in-flight resolution table, and the executable `RemoteDestinationBackend` | `ServiceDestinationDelivery`, `RoutingDecision`, `RemoteDeliveryCounters`, `RemoteDestinationBackend`, `PendingRemoteResolution`, `RemoteResolutionIdAllocator`, `RemoteDeliveryError`, `classify_destination`, `destination_hash_bytes`, `destination_hash_from_slice`, `destination_hash_as_router_hash`, `tunnel_id_from_bytes` |

### Managed application router gateway

The `app_gateway` module is the narrow router-side boundary for a future
trusted managed-app runtime. It accepts a non-deserializable authorization
value produced by trusted composition, binds one immutable effective
capability set to one `AppPrincipal` launch instance, and owns separate private
SAM and I2CP client contexts and supervised byte-stream connections. Service
capability checks precede context or task allocation. SAM naming receives the
canonical `SharedAddressBook`; neither protocol path uses a loopback listener
fallback. `control_scoped` is typed unavailable until a separately gated
Proposal 170 adapter is available. Process authentication, package/grant
management, outer framing/multiplexing, launch, and sandboxing belong to the
future trusted runtime and are not provided by this gateway. The contract and
exact byte-stream mapping are specified in
[`managed-native-app-runtime-v1.md`](../../specs/references/managed-native-app-runtime-v1.md).

| File | Lines | Responsibility | Key types |
| --- | --- | --- | --- |
| `src/app_gateway.rs` | 704 | Plan 355 per-principal authorization, capability-first service admission, isolated private SAM/I2CP state, bounded supervised byte-stream ownership, and no listener fallback | `AppGatewayAuthorization`, `AppGatewayLimits`, `AppGatewaySession`, `AppGatewayConnection`, `AppGatewayConnectionEnd`, `AppGatewayError` |
| `src/app_manager_bridge.rs` | Plans 368/408 trusted AppManager bridge over an **injected** duplex stream: SAM/I2CP gateway plus administrator-granted `local_service` publication. The daemon binds only `127.0.0.1`, owns accepted sockets, and forwards exact bytes over bounded logical streams; appd receives no socket or descriptor. Listener, accepted-stream, and session teardown are cancellation-owned | `AppManagerBridge`, `AppManagerComposition`, `AppManagerBridgeError`, `ManagerTransport` |

### Trusted AppManager bridge

The `app_manager_bridge` module is the router-facing consumer of the private
manager protocol in `i2pr-app-manager-proto`. It exists because
`AppGatewaySession` is crate-private: Plan 355 froze the router-side capability
boundary but had no production caller, so nothing outside the daemon could ask
the router whether a principal may use SAM or I2CP.

Filling that gap with a loopback listener would be wrong — loopback is not a
trust boundary, because a managed application can reach it through any local
proxy or helper. Proposal 170 is also wrong: it is router *administrator*
authority, strictly larger than this bridge may hold. ADR 0035 therefore fixes the
transport as an anonymous **inherited capability** and the authority ceiling as
strictly below Proposal 170. Plan 368 chooses no concrete transport, so the
bridge takes its duplex stream by injection; Plan 369 owns the inherited binding.

Properties the module holds:

- One manager transport maps to one bridge; one accepted `create_session` maps to
  exactly one `AppGatewaySession` bound to one `AppInstanceId`.
- Manager-asserted effective grants are **re-derived** through
  `GrantedCapability::from_administrator_policy` and
  `EffectiveCapabilities::from_grants`. There is no decoder from an application
  `hello`, a `RequestedCapability`, or manifest bytes into authority, so
  application-declared capability can never be promoted.
- The capability check happens before any backend context, permit, or task
  allocation, and a refusal is side-effect free.
- Service handles are session-local, so a handle from another session is simply
  absent and fails deterministically.
- Service octets are forwarded exactly: no base64, JSON wrapping, rewriting, or
  reordering. Each stream's in-memory duplex and inbound queue are bounded, so a
  slow manager stalls the reader instead of growing memory.
- Each stream has its own backend watcher, so one backend EOF closes only that
  stream; manager transport EOF tears down every session and connection.
- `control_scoped` is unrepresentable in the protocol's service vocabulary, so
  the bridge cannot open it even if asked.

The normative contract is
[`managed-app-manager-protocol-v1.md`](../../specs/references/managed-app-manager-protocol-v1.md).
`scripts/check-managed-app-manager-boundary.py` and `scripts/check-runtime-boundaries.sh`
enforce the boundary statically. Plan 369 supplied the production caller this
module previously lacked: `app_runtime` drives it over the inherited anonymous
transport. The `#![allow(dead_code)]` remains scoped and documented, and must
not be read as evidence that the bridge is unused.

### Managed application runtime supervision (Plans 369–371 and 382–383)

`app_runtime` is the daemon's half of the managed-app process: it creates the
two anonymous pipes, resolves the manager executable, supervises the child, and
runs the `AppManagerBridge` over those pipes.

When the optional service starts, the daemon resolves `data_dir` against the
startup cwd, creates `<data_dir>/managed-apps` with owner-private permissions,
canonicalizes the directory, clears the child environment, and supplies only
`I2PR_APP_STATE_ROOT=<canonical-path>` to the sibling manager. That root is
local administrator policy input; appd still requires the inherited pipes and
cannot be given a listener or a router protocol endpoint.

**Executable resolution is distribution-owned.** The manager is the
`current_exe()` **sibling** of the daemon, with platform suffix rules applied
(`i2pr.exe` on Windows, the bare name elsewhere — "a name that would otherwise
need a shell to interpret is not a name we are willing to spawn"). There is
deliberately no configuration counterpart to that constant: a configurable
manager path is exactly the user-configurable-program hole Plan 369 closes.

**There is no discoverable endpoint.** The manager's read end is the
daemon→manager pipe's write end and vice versa; there is no listener and no
port, and the manager cannot be reached by anything that did not inherit the
pipes.

**Teardown is bounded and always escalates.** The daemon drops the pipe ends,
which is EOF, which is the manager's only shutdown signal. A manager that
ignores it must not hold the daemon open, so after `MANAGER_EXIT_GRACE` the
direct child is killed, and after a short `MANAGER_REAP_GRACE` it is reaped. A
manager that already violated the protocol gets the short reap path instead —
waiting the full graceful grace there would multiply the restart budget by an
order of magnitude.

**Failure never terminates the router.** Every `ManagerLaunchError` variant is
fail-closed: the service returns a typed failure and the supervisor's bounded
restart policy decides. The app runtime registers with
`StartupRequirement::Optional` (Plan 371), so an optional subsystem degrades
its own feature instead of aborting router startup, and it is excluded from
`SupervisorSnapshot::ready` so a usable router is not reported as unready.
Invariant 1 is the reason this is optional rather than a matter of taste.

Appd's production catalog reads this state root after the private handshake,
holds the runtime lock for the manager lifetime, loads one strict policy
generation, and re-verifies each selected package before creating authority.
No persistent launch policy belongs to the daemon or router config. See
[managed-app policy v1](../../specs/references/managed-app-policy-v1.md) and
[Plan 383 app state](i2pr-app-state.md).

stderr is drained continuously with a bounded retained snapshot
(`MAX_MANAGER_STDERR_SNAPSHOT_BYTES`) and an uncapped byte total, so a failing
manager cannot drive unbounded allocation. The snapshot exists so an operator
can see *why* a manager refused to start; it is never interpreted as protocol.

**`set_manager_path_override_for_tests` is a test seam.** It is `#[doc(hidden)]
pub` so a `#[cfg(test)]` module can point the supervisor at the fixture
manager. A definition is inert, but a *caller* would turn it into "the router
can be told which executable to start";
`scripts/check-managed-app-process-boundary.py` rule 4 separates the two shapes
and fails closed on any production caller.

### SAM (Plan 368 extension in progress)

The TCP listener is an admission adapter over one SAM connection driver. The
same driver accepts an injected bounded async byte stream and an explicit
connection profile. Listener clients retain the ordinary loopback `STREAM
FORWARD` behavior; private managed-app connections deny host-target forwarding
before registration or any host connect. The private seam binds no listener,
requires no listener config, and never uses a localhost socket pair. Raw STREAM
mode transfers the generic transport into the same supervised raw driver.

| File | Lines | Responsibility | Key public types |
| --- | --- | --- | --- |
| `src/sam.rs` | current | SAM listener composition; Plan 368 adds PRIMARY/subsessions and ordinary loopback UDP datagram bridge; private app path remains injected-stream/typed manager operation and has no host UDP endpoint authority | `SamServiceState`, `SamServiceError`, private connection seam, `StreamingPools`, `execute_session_create`, `execute_stream_connect`, `execute_stream_accept`, `set_addressbook_handle` |
| `src/sam/datagram_udp.rs` | current | Bounded SAM UDP 3.x header parser and DATAGRAM1/2/3/RAW wire formatter; host endpoint remains loopback-only | `DatagramPacket`, `parse_packet`, `encode_received` |
| `src/sam/fabric.rs` | 457 | Plan 149 localhost product fabric: OS-CSPRNG tunnel material, signed LeaseSet2, per-destination runtime-driver factory, typed delivery sweep counters | `SamLocalProductFabric`, `LocalDestinationProduct`, `LocalhostInboundTunnelFactory`, `DeliverySweepCounters`, `LocalDeliveryDegradation` |
| `src/sam/streams.rs` | 2 075 | Plans 138/143/144 SAM Streaming bridge; Plan 392 adds an identity-bound staged router projection so a committed Destination can accumulate bounded inbound/outbound roles before its first usable LS2 | `SamDestinationBridge`, `SamDestinations`, `RouterDestinationNetworkState`, `InboundReceiveProjectionError`, `bridge_to_peer`, `BridgeDiagnostics`, `SamDestinationHandle::lookup_by_peer_hash`, `receiver_streaming`, `peer_destination_hash` |
| `src/sam/raw_stream.rs` | 830 | Plan 147 dedicated raw STREAM driver over an owned async byte stream (the Plan 143 command-mode regression fix: real byte-stream <-> `StreamingManager` loop, CSPRNG CONNECT path) | `SamAsyncStream`, `SamIoStream`, raw-stream types |
| `src/sam/faults.rs` | 464 | Plan 151 §8 deterministic pre-start delivery fault seam for adversarial tests (packet-level faults a TCP SAM client can never observe) | fault types |

### I2CP

The TCP listener is an admission adapter over one I2CP connection driver. The
driver also accepts an injected bounded async byte stream, preserving the same
protocol byte, frame, timeout, inbound notification, and teardown behavior.
Private connections require no listener config or host socket and remain under
the existing daemon `ChildScope` ownership and service limits.

| File | Lines | Responsibility | Key public types |
| --- | --- | --- | --- |
| `src/i2cp.rs` | 2 616 | Plan 167 supervised I2CP listener adapter and private async-stream entry share one connection driver; Plan 168 bounded per-session message/data plane; Plan 169 reconfigure transaction handler + synchronous destroy drain; **Plan 171 explicit `stream.shutdown()` on terminal paths**; Plan 170 `ReplyAndFollowup` `RequestVariableLeaseSet` | `I2cpServiceState`, `I2cpServiceError`, `I2cpServiceSnapshot`, `I2cpSessionState`, private connection seam, `handle_connection`, `handle_connection_inner`, `install_client_lease_set2`, `reserve_client_destination`, `handle_send_message`, `handle_send_message_expires`, `handle_dest_lookup`, `derive_bandwidth_reply`, `handle_reconfigure_session`, `handle_destroy_session`, `apply_reconfigure`, `ReconfigurationOutcome`, `teardown_connection`, `drop_connection` |

### I2PControl

| File | Lines | Responsibility | Key public types |
| --- | --- | --- | --- |
| `src/i2pcontrol.rs` | 3 449 | Plan 287 supervised loopback-default I2PControl TLS + JSON-RPC listener: daemon-owned TLS (managed ephemeral self-signed for loopback, explicit PEM otherwise), in-memory token table, source-IP throttle, bounded connection/in-flight permits, sequential no-fanout batch dispatch with deferred mints, graceful `close_notify` shutdown; extended by Plans 288/289/294 with RouterInfo, ClientServicesInfo, TunnelManager, and AddressBook dispatch | `I2pControlServiceState`, `I2pControlServiceError`, `I2pControlServiceSnapshot`, `dispatch_body`, `ct_password_eq`, `build_tls_config`, `new_with_inspection`, `set_control_manager`, `set_addressbook_manager`, `process_router_info`, `process_client_services`, `process_tunnel_manager`, `process_addressbook` |
| `src/i2pcontrol_inspection.rs` | 2 687 | Plan 288 narrow inspection handles and pure response builders: static config truth plus publish-gated dynamic snapshots; select-form validation with canonical ordering; truthful service shapes; Plans 294/295 address-book getters, live log-ring/metrics/SSU2 cells, and composition attestations | `InspectionHandles`, `ServiceEndpoint`, `StartupServiceEntry`, `PublishedSnapshots` (`publish_*`), `FloodfillMode`, `PublishError`, `InspectionGap`, `SelectError`, `select_router_info`, `select_client_services`, `router_info_result`, `client_service_result`, `publish_addressbook`, `publish_log_ring`, `publish_metrics`, `publish_ssu2`, `publish_bans`, `MAX_INSPECTION_LIST = 1024`, `MAX_INSPECTION_STATE_STRING = 32`, `MAX_LOCAL_ROUTER_INFO_BYTES = 786_432`, `MAX_INSPECTION_HASH_STRING = 64` |
| `src/i2pcontrol_tunnels.rs` | 9 483 | Plan 289 TunnelManager control state over the **one** shared M10 manager (ADR 0031, Plan 337). The composition root builds that manager once in `build_shared_service_manager` and injects the same `Arc` into the control state and the destination-group product, so a control-created runtime is the runtime the product delivers and publishes through. `for_config` takes the manager rather than building one. The I2PControl service declares `depends_on("ssu2-router")`. Three shared-manager changes are load-bearing: `candidate_set` carries startup-owned specs through verbatim, `verify_agreement` scopes its "no extra runtime" rule to names this coordinator does not own, and `shutdown` reconciles back to the startup-only set instead of tearing the manager down. `sync_els2_materials` installs or drops the Plan 334 publication material at the end of a committed transaction and **fails the transaction closed** when a type-5 definition's identity record is unreadable. `rollback_state` (Plan 338) reconciles the shared manager to the rolled-back candidate; all five failure paths await it, and a rollback that cannot reconcile is logged, not swallowed | tunnel control-state, versioned generation store, transaction coordinator, twelve-family typed mapping types |
| `src/outbound_secret.rs` | 467 | **Plan 341** restart-safe outbound proxy secret owner. HKDF-SHA256 derives a ChaCha20-Poly1305 key from the router signing seed; the proxy credential is sealed under a random nonce with an associated-data frame | `OutboundSecretKey`, `RouterBoundOutboundSecrets`, `OUTBOUND_SECRET_KEY_INFO`, `OUTBOUND_SECRET_KEY_LEN = 32`, `OUTBOUND_SECRET_NONCE_LEN = 12`, `OUTBOUND_SECRET_TAG_LEN = 16` |

### Router console (Plans 356–358)

| File | Lines | Responsibility | Key public types |
| --- | --- | --- | --- |
| `src/console.rs` | 478 | **Plan 356–358** daemon half of the loopback browser console. Owns the EggServe listener lifecycle (bind under a bounded timeout, then serve until cancellation), builds the `SecurityPolicy` **after** `bind` so the authority allow-set names the resolved port, adapts `AxumRouter` through `TowerToEggserve`, and implements `ControlClient` over the in-process `ControlDispatcher`. Disabled by default; a non-loopback bind is rejected at config parse | `ConsoleServiceState`, `ConsoleControlClient`, `classify_envelope`, `security_policy`, `requestable_overview_selectors`, `VERIFIED_BASE_OVERVIEW_SELECTORS` |
| `src/i2pcontrol_dispatch.rs` | 902 | Plan 358 extraction of the two read-only Proposal 170 handlers (`RouterInfo`, `ClientServices`) out of the listener state into a transport-free `ControlDispatcher`. The console injects one of these directly, so the console needs **no** external I2PControl listener, password, or token. `LocalConsolePrincipal` carries a closed allow-set of method names; a request naming anything else is refused before the dispatcher is consulted | `ControlDispatcher`, `LocalConsolePrincipal` |

The console is deliberately **not** a second I2PControl listener. Everything
Plan 358 renders comes from `ControlDispatcher` calls in-process, and the
console's `LocalConsolePrincipal` can only ever name `RouterInfo` and
`ClientServices` — the read-only, already-published methods — never
`TunnelManager` or `AddressBook`. See [Router console](#router-console-plans-356358)
under Key contracts for the lifecycle detail.

### Address book, control sources, and NEWS

| File | Lines | Responsibility | Key public types |
| --- | --- | --- | --- |
| `src/addressbook.rs` | 1 504 | Plan 294 canonical AddressBook runtime owner (daemon half): `[addressbook]` config, current/backup/import activation rule, transactional mutations with rollback, generation persistence, shared resolver cell, bounded refresh queue + diagnostic artifact, snapshot-artifact import, cadence worker driver | `AddressBookManager`, `AddressBookSubsystemConfig`, `SharedAddressBook`, `AddressBookManagerError`, `IngestReport`, `MAX_SNAPSHOT_ARTIFACT_BYTES = 8_000_000`, `MAX_DIAGNOSTIC_ARTIFACT_BYTES = 65_536`, `DIAGNOSTIC_ARTIFACT_RETAIN_BYTES = 32_768` |
| `src/addressbook_fetch.rs` | 655 | Bounded content fetch over an **explicitly configured local HTTP proxy**. Its only network capability is a loopback eepProxy connect | (private module) |
| `src/control_sources.rs` | 672 | Plan 295 control-plane source owners: bounded redacted `LogRing` (INFO+, secret markers, 256 entries, 192 B lines) with the `tracing` layer feed, explicit `BanLedger` attesting the empty set, rolling `ControlMetrics` (O(1) tick over registered transport counters, bandwidth pair, `ssu2.*` rates, build outcomes) | `LogRing`, `LogRingLayer`, `BanLedger`, `ControlMetrics`, `MAX_LOG_RING_ENTRIES`, `MAX_LOG_LINE_BYTES`, `MAX_CONTROL_RATES` |
| `src/news.rs` | 1 401 | Signed NEWS handling (private module; not part of the public surface) | (private module) |

### Service tunnels (M10)

| File | Lines | Responsibility | Key public types |
| --- | --- | --- | --- |
| `src/service_tunnels.rs` | 9 342 | The M10 `ServiceTunnelManager` with explicit Destination-group ownership and per-service listener lifecycle. 103 public methods, including `new`, `set_addressbook_handle`, `prepare`, `reconcile`, `start_supervisors`, `shutdown`, `install_router_delivery` / `uninstall_router_delivery`, `router_service_candidates`, `routing_decision_for`, `co_owned_destination_hashes`, `reap_expired_drains`, and `generation_snapshot`. Plans 289/309/290/292/294/296/297 add generation cancellation separation, shared-group identity, composed-family dispatch, the pre-SYN access gate + idle sweeper, address-book resolution, multihoming, reply bundling, and TLS dial | `ServiceTunnelManager`, `ServiceTunnelManagerConfig`, `ServiceTunnelSnapshot`, `DestinationGroupRuntime`, `ServiceRuntime`, `register_service_tunnel_manager` |
| `src/service_product.rs` | 10 264| Plan 209 production composition helper wiring SSU2 + tunnels + LeaseSet2 + delivery backend; Plan 294 address-book install; Plan 318 normal-daemon adapter with bounded validated-RouterInfo handoff, bounded startup inbound replay, exact-three first-hop resolution, readiness gated on usable group pools; Plans 388–389 interleave bounded post-start builds and retain current-generation per-Destination stage counts; Plan 390 provisions aggregate role capacity; Plan 392 initializes identity-bound staged bridge state on committed router-backed generations. **Plan 513** adds the tick step `drive_netdb_discovery` and its two deliveries — `deliver_discovery_direct` for the exploratory kind and `deliver_discovery_via_outbound_tunnel` for the RouterInfo kind — plus the two ingestion helpers `ingest_discovery_search_reply` and `ingest_discovery_routerinfo_store`  **Plan 515** moves the creator RouterInfo publication onto the **shared** path — the same selection, the same store and the same delivery the router-level refresh uses — so the two cannot drift apart. **Plan 516** puts the one-shot publication on the **correlator**, so it begins a tracked attempt with the minted token on the wire and shares the attempt book with the router-level refresh | `ServiceProduct`, `ServiceProductSpec`, `ServiceProductError`; entry points `ServiceProduct::start` (`pub async`, `src/service_product.rs:2407``) and `ServiceProduct::start_over_existing_daemon` (`pub(crate) async`) |
| `src/service_els2.rs` | 868 | Plan 334 daemon-owned ELS2 publication owner for a control-owned service. Builds the `BlindingSchedule` in owner mode from the service's own Ed25519 signing seed, constructs `Els2AuthorizationServerConfig` (PSK or DH) from the bounded durable client list, and produces the real type-5 `DatabaseStore` at the day's **blinded storage key** with the correctly flagged `.b32.i2p` address. Adds no new file format and no new at-rest secret. Resolved by `ServiceTunnelManager::service_identity_record`, not by a re-derived store path (Plan 338) | `ServiceEls2Material`, `build_service_els2_material`, `load_service_els2_material`, `ServiceEls2Error` |
| `src/encrypted_target_credential.rs` | 452 | Plan 380 the ELS2 **consumer client credential**: an owned, zeroizing, non-`Clone`, non-`Debug`, non-`Display` credential holding a `PskClientKey` or an X25519 key pair whose public half is derived from the private one, plus the sealed stored form the manager holds (`SealedEncryptedTargetCredential`: ciphertext + the `Arc` that opens it). A closed grammar of `psk:<64 lowercase hex>` / `dh:<64 lowercase hex>` with no leniency, and an error enum whose every reason is a `&'static str` so no malformed value can reach a client reply or a log line | `EncryptedTargetCredential`, `SealedEncryptedTargetCredential`, `EncryptedTargetCredentialError`, `ELS2_CREDENTIAL_OPTION` |
| `src/service_generation.rs` | 205 | Plan 180 §3 committed-generation bookkeeping for `ServiceTunnelGeneration` | `ServiceGeneration`, generation diff types |
| `src/service_lifecycle.rs` | 360 | Bounded normal-daemon Destination-group startup and retirement policy — local phase/timing only, no secrets and no routing state (private module) | (private module) |
| `src/service_tunnels_http.rs` | 1 050 | HTTP proxy executor, one loopback listener per `http-client` spec; Plan 290 adds the strict-CONNECT client executor and the shared SOCKS version-peek negotiator; Plan 292 adds guarded-listener proxy authentication (407 challenge) | HTTP executor types |
| `src/service_tunnels_http_server.rs` | 673 | Plan 290 filtered HTTP server executor: shared SYN accept, loopback target connect, slowloris-bounded head read, server privacy filter, paced body forward, filtered response admit, remainder relay; Plan 292 adds pre-filter presentation gating | `HttpServerConnectionOutcome`, `run_http_server_connection`, `run_http_server_loop` |
| `src/service_tunnels_http_bidir.rs` | 212 | Plan 290 deprecated bidirectional HTTP server (loopback proxy listener plus Streaming SYN poll under one supervisor task, one generation, one persistent public identity) | `run_http_bidir_loop` |
| `src/service_tunnels_socks5.rs` | 1 014 | SOCKS5 executor, one loopback listener per `socks5-client` spec; Plan 292 adds guarded-listener RFC 1929 authentication (`05 02`, no-auth refused, SOCKS4a rejected without downgrade) | SOCKS5 executor types |
| `src/service_tunnels_irc_client.rs` | 521 | IRC client executor, one loopback listener per `irc-client` spec; Plan 290 extends the filtered loop with `initial_inbound` for the SOCKS-to-IRC handoff | IRC client executor types |
| `src/service_tunnels_irc_server.rs` | 729 | IRC server executor (Streaming accept loop per `irc-server` spec): waits for `Established`, captures the peer Destination hash, runs the bounded registration interceptor under a 30 s deadline, connects under 10 s, writes the rewritten prefix + leftover exactly once, then switches to the shared pump in opaque mode | IRC server executor types |
| `src/service_tunnels_socks_irc.rs` | 337 | Plan 290 SOCKS+IRC composer: shared version-peek negotiation, target selection, then the IRC filtered loop with no raw bypass | `SocksIrcConnectionOutcome`, `run_socks_irc_loop` |
| `src/service_tunnels_streamr.rs` | 739 | Plan 291 Streamr subscriber/publisher executors: loopback UDP media source/target sockets, bounded subscribe cadence with terminal unsubscribe, authenticated subscriber table with expiry sweep and raw fanout, producer-bound media forwarding; Plan 292 adds the subscriber sink redirect | `StreamrLoopOutcome`, `run_streamr_client_loop`, `run_streamr_server_loop` |
| `src/service_tunnels_tls.rs` | 428 | Plan 297 explicit local TLS identity/trust policy: provisioned PEM identity (X.509 expiry surfaced), SPKI pins and/or explicit trust roots (never ambient roots), explicit loopback opt-in, verifies-nothing rejected at load, custom pin-or-roots verifier with signature-scheme delegation, per-dial client configs, redacted secret handling | `ServiceTlsPolicy`, `TlsPolicyHandle`, `LoadedIdentity`, `PinOrRootsVerifier`, `ServiceTlsError`, `tls_connect` |
| `src/outproxy_route.rs` | 976 | **Plan 343 daemon half of the I2P-routed outproxy provider** — the *route owner*. Everything decidable without a socket lives in the runtime-neutral `i2pr-service-tunnels::outproxy`; this module adds only the two things the composition root can do: the I/O half (open a Streaming route to the selected I2P outproxy destination and speak the outproxy-facing handshake with bounded retry, a separate handshake deadline, and a bounded read) and the credential half (recover the sealed password through Plan 341's `OutboundSecretStore` and build the header). **Reachable from all four client request paths as of Plan 342** via the shared opener `open_client_route` — see **Outproxy provider** under Key contracts | `RouterOutproxyProvider` (impls `OutproxyProvider`), `OutproxySession`, `OutproxyCounters`, `classify`, `build_attempt`, `interpret_reply`, `is_complete`, `open_via_outproxy`, `OUTPROXY_HANDSHAKE_DEADLINE_MS = 15_000` |

---

## Public surface

### The actual `pub mod` / `mod` declarations (`src/lib.rs:9`–70`)

57 `pub mod` + 6 private modules + 5 `sam/` submodules:

`pub mod` — `addressbook`, `app_gateway`, `app_manager_bridge`, `app_runtime`,
`bootstrap`, `cli`, `config`, `console`, `control_sources`,
`destination_peers`, `destination_streaming`, `destination_tunnels`,
`encrypted_service_resolver`, `encrypted_target_credential`, `error`,
`exploratory_build`, `floodfill`,
`i2cp`, `i2pcontrol`, `i2pcontrol_dispatch`,
`i2pcontrol_inspection`,
`i2pcontrol_tunnels`, `inbound_dispatch`, `netdb_participation`,
`netdb_seam`, `netdb_tunnels`, `ntcp2_activation`,
`outbound_lookup`, `outbound_secret`, `outproxy_options`, `outproxy_route`,
`peer_test`, `router_i2np`,
`sam`, `service_delivery`, `service_els2`, `service_generation`, `service_product`,
`service_tunnels`, `service_tunnels_http`, `service_tunnels_http_bidir`,
`service_tunnels_http_server`, `service_tunnels_irc_client`,
`service_tunnels_irc_server`, `service_tunnels_socks5`, `service_tunnels_socks_irc`,
`service_tunnels_streamr`, `service_tunnels_tls`, `ssu2_introducer`,
`ssu2_qualification_export`, `ssu2_qualification_receipts`, `ssu2_relay_drain`,
`transit_compose`,
`transit_composition`,
`transit_owner`, `transit_participation`, `transit_volume`,
`tunnel_liveness`, `version_eligibility`.

`console` and `i2pcontrol_dispatch` were added by Plans 356–358;
`app_gateway`, `app_manager_bridge` by Plan 368 and `app_runtime` by Plan 369.
`netdb_participation` was added by Plan 488 and extended in place by Plans 489
(routing-table driver) and 497 (`RoutingTableConsumer`).
`ntcp2_activation` holds the profile-aware NTCP2 activation gate; it is a
public module so that the gate's refusal vocabulary is nameable from outside
the crate, and it holds **no** authority of its own.
`ssu2_relay_drain` was added by Plan 498; it holds the bounded drain from the
runtime's relay handoff into the introducer role owner and no authority of its
own.

Private — `mod addressbook_fetch`, `mod bandwidth_eligibility`, `mod news`,
`mod service_lifecycle`,
`mod app_runtime_qualification` (the Plan 369 WP5 `#[cfg(test)]` black-box
module; it compiles out of every real build, which is why it may name the
fixture binaries freely), and `mod tests` (in-crate test module).

### The actual `pub use` re-exports (`src/lib.rs:74`–79`)

```rust
pub use error::DaemonError;
pub use i2cp::{I2cpServiceError, I2cpServiceSnapshot, I2cpServiceState};
pub use i2pcontrol::{I2pControlServiceError, I2pControlServiceSnapshot, I2pControlServiceState};
pub use i2pcontrol_inspection::InspectionHandles;
pub use netdb_seam::{
    CompositionOutcome, ExploratoryPathStatus, LeaseSet2ResponseOutcome, NetDbSeam, NetDbSeamError,
};
pub use sam::{SamServiceError, SamServiceState, StreamingPools};
```

### Crate-root public items (`src/lib.rs`)

- `enum CommandOutcome`: `Validated { dry_run, config }`, `IdentityGenerated { path }`,
  `IdentityInspected { path, summary }`, `RunReady { config }`.
- `struct IdentitySummary { signing_algorithm: u16, encryption_algorithm: u16 }`.
- `fn execute(cli: Cli) -> Result<CommandOutcome, DaemonError>` — pure dispatch hub.
- `fn initialize_logging(config: &config::LoggingConfig)`.
- `fn bootstrap_daemon(&Config, now_seconds, offline_reseed_path) -> Result<(bootstrap::BootstrapReport, Arc<Mutex<bootstrap::Bootstrap>>), DaemonError>`.
- `fn build_daemon_graph(&Config) -> Result<i2pr_runtime::ServiceGraph, DaemonError>`.
- `fn build_daemon_graph_with_inspection(...) -> Result<(i2pr_runtime::ServiceGraph, Arc<InspectionHandles>), DaemonError>`.
- `fn build_shared_service_manager(...)` — builds the one M10 manager (Plan 337).
- `async fn run_daemon(config: Config) -> Result<(), DaemonError>`.

`ServiceGraph` is **`i2pr_runtime::ServiceGraph`** — the daemon owns no graph type of its own.

### Per-area public types

- **CLI/config** — `Cli`, `Command`, `IdentityCommand`, `CheckConfigArgs`, `IdentityArgs`,
  `RunArgs`; the `Config` family in the layout table above; `ConfigError`.
- **Errors** — `ExitCode` (`#[repr(u8)]`, `as_i32()`), `DaemonError` (with `exit_code()`).
- **NetDB/routing** — `NetDbSeam`, `NetDbSeamError`, `CompositionOutcome`,
  `ExploratoryPathStatus`, `LeaseSet2ResponseOutcome`, `NetDbTunnelCoordinator`,
  `OutboundLookupDispatch`, `OutboundLookupError`, `InboundDispatchOutcome`,
  `InboundResponseKind`, `InboundDispatchError`.
- **Tunnels/transit/floodfill** — `ExploratoryBuildCoordinator`, `TunnelLivenessScheduler`,
  `DestinationTunnelCoordinator`, `TransitOwner`, `TransitLiveOwner<R>`,
  `TransitParticipation`, `controlled_transit_disabled_probe`,
  `TransitCompositionInputs`, `TransitCompositionError`,
  `Ssu2DaemonService`, `Ssu2DaemonHandle`, `RouterDeliveryService`, `RouterI2npOutcome`.
- **SAM** — `SamServiceState`, `SamServiceError`, `StreamingPools` (re-exported).
- **I2CP** — `I2cpServiceState`, `I2cpServiceError`, `I2cpServiceSnapshot` (re-exported).
- **I2PControl** — `I2pControlServiceState`, `I2pControlServiceError`,
  `I2pControlServiceSnapshot` (re-exported), `InspectionHandles` (re-exported),
  `OutboundSecretKey`, `RouterBoundOutboundSecrets`.
- **Address book/control** — `AddressBookManager`, `SharedAddressBook`,
  `AddressBookSubsystemConfig`, `AddressBookManagerError`, `IngestReport`, `LogRing`,
  `LogRingLayer`, `BanLedger`, `ControlMetrics`.
- **Service tunnels** — `ServiceTunnelManager`, `ServiceTunnelManagerConfig`,
  `ServiceTunnelSnapshot`, `ServiceProduct`, `ServiceProductSpec`, `ServiceProductError`,
  `ServiceDestinationDelivery`, `RemoteDestinationBackend`, `RoutingDecision`,
  `ServiceEls2Material`, `ServiceGeneration`.
- **Outproxy route owner** — `RouterOutproxyProvider`, `OutproxySession`,
  `OutproxyCounters`, `classify`, `build_attempt`, `interpret_reply`, `is_complete`,
  `async open_via_outproxy`, `OUTPROXY_HANDSHAKE_DEADLINE_MS`. None is re-exported at the crate
  root. **Plan 342 changed the reachability**: `open_via_outproxy` is now reached from all four
  client request paths through the single shared opener `open_client_route`, which is itself
  reached from `connect_via_outproxy` (`service_tunnels_http.rs`),
  `forward_via_outproxy` (`service_tunnels_http.rs`), and `run_socks5_connection`
  (`service_tunnels_socks5.rs`). The Plan 343 statement that "nothing in `src/` calls
  `open_via_outproxy`" was true when written and is **false now**; it is retained in the Plan 343
  closure record, not here.

---

## Key contracts

### CLI surface (`src/cli.rs`)

```
i2pr [--version] [--help] <SUBCOMMAND>
```

About string (`src/cli.rs:12`): *"Experimental I2P router (NTCP2 disabled while support is
experimental)"*

| Subcommand | Flags | Description |
| --- | --- | --- |
| `check-config` | `--config <PATH>` (required) | Parse and semantically validate a configuration without side effects. |
| `identity generate` | `--config <PATH>` (required) | Generate and atomically persist a new router identity. |
| `identity inspect` | `--config <PATH>` (required) | Load and validate the existing router identity without displaying secrets. |
| `run` | `--config <PATH>` (required), `--dry-run` (bool) | Validate configuration and optionally start the router runtime. |

All `--config` arguments are `#[arg(long)]`. **No positional arguments and no default config
path** — operator intent is always explicit.

### Strict configuration posture (`src/config.rs`)

- `deny_unknown_fields` appears **21** times — every `Raw*` struct rejects unknown keys.
- `CURRENT_SCHEMA_VERSION: u64 = 1` (`src/config.rs:13`) and the check is **`!=`**, not `>=`:
  any other value is `ConfigError::UnsupportedSchemaVersion { actual }` (exit 11). Schema
  migration requires a binary update first.
- `Config::parse` is pure text-in/no-I/O and leaves `source_path: None`;
  `Config::load` records the path as provenance only.
- `Config` fields: `source_path`, `schema_version`, `router`, `logging`, `limits`, `network`,
  `transport`, `netdb`, `reseed`, `news`, `sam`, `ssu2`, `i2cp`, `i2pcontrol`,
  `service_tunnels`, `addressbook`, `floodfill`.
- `RouterProfile` has exactly one variant (`Balanced`); any other profile string is rejected.
  `LogFormat` has exactly one variant (`Text`).
- **Config secret hygiene (Plan 352).** `ConfigError::Parse` carries a
  `RedactedTomlError`, not a bare `toml::de::Error`. Its `Display` emits the
  line, the column, and `[source content redacted]` — and nothing else, because
  both upstream renderers leak: `toml`'s `Display` prints the whole offending
  source line, and its `message()` embeds the rejected key and value on a
  `deny_unknown_fields` failure. Line/column come from the byte span resolved by
  `position_of`, so redaction costs no diagnostic. `Error::source` still returns
  the upstream error for programmatic callers; rendering the source chain on this
  path is what `check-config-secret-hygiene.sh` forbids.
- `Config::load` additionally refuses a file that **holds a password and is
  group- or world-readable** (`InsecureConfigPermissions`, naming the path and
  the observed mode, expected `0600`), using the `& 0o077` idiom already enforced
  by `i2pr-storage`, `i2pcontrol_tunnels.rs`, and `addressbook.rs`. The gate is
  conditional on `!config.i2pcontrol.password.is_empty()` and runs after parsing,
  so a secret-free config is unaffected. The decision itself is the
  platform-independent `secret_file_permission_verdict(path, Option<u32>)`;
  `None` (no POSIX mode, e.g. Windows) is **refused** rather than silently
  passed, so a gate that cannot run never reports success.

### Defaults baked into config parsing

| Field | Default | Source |
| --- | --- | --- |
| `router.profile` | `"balanced"` | `default_profile` (686) |
| `logging.filter` | `"info"` | `default_filter` (690) |
| `logging.format` | `"text"` | `default_log_format` (694) |
| `limits.max_tasks` | `4_096` | `DEFAULT_MAX_TASKS` (15) |
| `limits.max_buffered_bytes` | `67_108_864` (64 MiB) | `DEFAULT_MAX_BUFFERED_BYTES` (17) |
| `network.bind_address` | `"0.0.0.0"` | `default_bind_address` (706) |
| `network.listen_port` | `9150` | `default_listen_port` (710) |
| `network.network_id` | `2` | `default_network_id` (714) |
| `ntcp2.enabled` | `false` | `default_ntcp2_enabled` (718) |
| `netdb.enabled` | `true` | `default_netdb_enabled` (762) |
| `netdb.max_records` | `4_096` | (766) |
| `netdb.max_encoded_bytes` | `4 MiB` | (770) |
| `netdb.min_router_infos` | `50` | (774) |
| `netdb.min_floodfill_advertisers` | `5` | (778) |
| `netdb.routing_table_enabled` | `false` | Plan 489; `default_netdb_routing_table_enabled` |
| `netdb.routing_table_max_entries` | `1_024` | Plan 489; `default_netdb_routing_table_max_entries` |
| `netdb.routing_table_max_bucket_size` | `8` | Plan 489; `default_netdb_routing_table_max_bucket_size` |
| `netdb.exploration_enabled` | `false` | Plan 489; `default_netdb_exploration_enabled` |
| `netdb.exploration_max_peers` | `8` | Plan 489; `default_netdb_exploration_max_peers` |
| `netdb.exploration_deadline_ms` | `30_000` | Plan 489; `default_netdb_exploration_deadline_ms` |
| `netdb.participation_maintenance_period_ms` | `60_000` | Plan 497; `default_netdb_participation_maintenance_period_ms` |
| `reseed.enabled` | `false` | `default_reseed_enabled` (782) |
| `reseed.max_sources` | `4` | (786) |
| `reseed.max_su3_bytes` | `8 MiB` | (790) |
| `news.enabled` | `false` | `RawNewsConfig::default` (649) |
| `news.proxy_port` | `4444` | (794) |
| `news.max_su3_bytes` | `8 MiB` | (798) |
| `news.refresh_interval_secs` | `21_600` (6 h) | (802) |
| `sam.enabled` | `false` | `default_sam_enabled` (810) |
| `sam.bind_address` | `"127.0.0.1"` | (812) |
| `sam.port` | `7656` | (814) |
| `sam.udp_port` | `7655` | Plan 368; bound on the same loopback IP as the TCP listener |
| `sam.max_clients` | `16` | (816) |
| `sam.max_sessions` | `16` | (818) |
| `sam.max_stream_sockets_per_session` | `16` | (820) |
| `sam.max_pending_accepts_per_session` | `16` | (822) |
| `sam.max_buffered_bytes_per_stream_direction` | `64 KiB` | (824) |
| `sam.hello_timeout_ms` | `10_000` | (826) |
| `sam.command_timeout_ms` | `60_000` | (828) |
| `ssu2.enabled` | `false` | (941) |
| `ssu2.bind_ipv4` | `"127.0.0.1"` | (955) |
| `ssu2.bind_ipv6` | `""` (unset) | (961) |
| `ssu2.introducer_service` | `false` | (961) |
| `i2cp.enabled` | `false` | (874) |
| `i2cp.bind_address` | `"127.0.0.1"` | (876) |
| `i2pcontrol.enabled` | `false` | (916) |
| `i2pcontrol.bind_address` | `"127.0.0.1"` | (918) |
| `service_tunnels.enabled` | `false` | (1039) |
| `service_tunnels.max_active_connections` | `128` | (1047) |
| `addressbook.enabled` | `false` | (1055) |
| `floodfill.enabled` | `false` | (1063) |

**Loopback-only and disabled-by-default posture.** `[sam]`, `[i2cp]`, `[i2pcontrol]`,
`[service_tunnels]`, `[ssu2]`, `[addressbook]`, and `[floodfill]` all default to
`enabled = false` with a loopback `bind_address`. For I2PControl, TLS covers loopback
identities only (managed ephemeral self-signed); any non-loopback bind requires explicit
operator-owned certificate and private key, and there is **no plaintext fallback**
(`src/config.rs:327–332`). `I2pControlConfig::is_loopback_bind()` (1177) is the gate.
`I2cpConfig::loopback_test_profile()` (1114) is the loopback-only integration-test profile;
non-loopback bind is rejected fail-closed.

### Configuration hard caps (`src/config.rs`)

`MAX_ALLOWED_TASKS = 1_000_000`, `MAX_ALLOWED_BUFFERED_BYTES = 1 << 40`,
`MAX_ALLOWED_DURATION_SECS = 3_600`, `MAX_ALLOWED_PREFIX_IPV4 = 32`,
`MAX_ALLOWED_PREFIX_IPV6 = 128`, `MAX_ALLOWED_NETDB_RECORDS = 65_536`,
`MAX_ALLOWED_NETDB_ENCODED_BYTES = 64 MiB`, `MAX_ALLOWED_RESEED_SOURCES = 16`,
`MAX_ALLOWED_RESEED_BYTES = 16 MiB`, `MAX_ALLOWED_BOOTSTRAP_RECORDS = 65_536`,
`MAX_ALLOWED_ROUTING_TABLE_ENTRIES = 16_384`,
`MAX_ALLOWED_ROUTING_TABLE_BUCKET_SIZE = 64`,
`MAX_ALLOWED_EXPLORATION_PEERS = 64`,
`MAX_ALLOWED_EXPLORATION_DEADLINE_MS = 3_600_000`,
`MIN_EXPLORATION_DEADLINE_MS = 1_000` (Plan 489).

### Stable exit codes (`src/error.rs`)

| Code | Name | When |
| --- | --- | --- |
| 0 | `Success` | Command completed. |
| 10 | `ConfigUnavailable` | Config file could not be read. |
| 11 | `ConfigParse` | Invalid TOML or unsupported schema version. |
| 12 | `ConfigSemantic` | Syntactically valid but semantically invalid. |
| 20 | `RuntimeNotImplemented` | Reserved runtime path not implemented. |
| 30 | `IdentityStorage` | Identity persistence failure. |
| 31 | `IdentityCrypto` | Identity generation failure. |
| 40 | `RuntimeBindFailed` | TCP listener could not bind. |
| 41 | `RuntimeIdentity` | Router identity not found/invalid. |
| 42 | `RuntimeListenerFailed` | Listener accept loop failed. |
| 43 | `RuntimeDialFailed` | Outbound connection failed. |
| 44 | `RuntimeHandshakeFailed` | Transport handshake failed. |
| 45 | `RuntimeShutdownTimeout` | Supervised shutdown exceeded deadline. |
| 46 | `RuntimeSupervisorFailed` | Child task crashed and the supervisor terminated. |
| 47 | `RuntimeBootstrap` | Bootstrap pipeline failed. |
| 70 | `Internal` | Unexpected internal failure. |

`clap`'s own usage errors produce exit code **2**. `DaemonError` variants:
`ConfigUnavailable { path, source }`, `Config(ConfigError)`, `RuntimeNotImplemented`,
`IdentityStorage(StorageError)`, `IdentityCrypto(CryptoError)`, `RuntimeIdentity(String)`,
`RuntimeBindFailed(io::Error)`, `RuntimeListenerFailed(String)`,
`RuntimeDialFailed(String)`, `RuntimeHandshakeFailed(String)`,
`RuntimeShutdownTimeout`, `RuntimeSupervisorFailed(String)`, `RuntimeBootstrap(String)`,
`Internal`. `ConfigError` variants: `Parse(toml::de::Error)`,
`UnsupportedSchemaVersion { actual }`, `Semantic { .. }`.

### The router republishes its own RouterInfo (Plan 515)

Until Plan 515 the router published its **own** RouterInfo once, from the
service-tunnel provisioning path, and never again. The local record was read
in exactly two places in the whole tree: that publication, and the SSU2
service installing the record for handshakes. A record that is never
refreshed ages out of the network's view, so no peer could find the router
however long it ran — and because the publication was a side effect of
building service tunnels, a router with **no** service tunnels never
published at all.

**What the module owns** (`src/router_info_publication.rs`):

- the **schedule** — a period, a due test, and the counted outcomes. It
  takes `now_ms` from its caller and never reads a clock, which is what
  makes the cadence provable without a wall clock, a socket, or a sleep;
- the **selection** — `publication_targets`, the version-gated function the
  destination coordinator already applies, so Plan 503's API-version floor
  holds for every publication and the two paths cannot drift apart;
- the **delivery** — `publish_router_info`, the one path that puts the
  store on the wire. The service product's one-shot publication and the
  router-level refresh both come through it.

**Where the refresh lives.** The transport is router-wide: the service
product attaches to the router's *existing* SSU2 daemon
(`ServiceProduct::start_over_existing_daemon`), and the router's own owner
reads the installed record from that daemon. The selection's store is the
router-wide **bootstrap's** store — the store the live discovery spine
already selects from — so the refresh is a router-level periodic owner and
does not depend on a service tunnel existing. The SSU2 service's loop
carries the tick; the records are read under the bootstrap's lock and the
lock is released before anything is sent.

**What is not claimed.** The reference's resend burst is not adopted, and
the divergence is recorded rather than papered over.

### The publication's acknowledgement (Plan 516)

Plan 515 made the publication **repeatable**. Plan 516 makes it
**observable**.

The correlator already minted a reply token for every attempt, recorded it
in its bounded token map, and documented that the peer would correlate the
acknowledgement with it — and then wrote `reply_token: 0` into the store
message. A zero token asks a peer for **no** acknowledgement, so the
floodfill never answered, the correlator's `correlate_delivery_status` could
never match, and the `acknowledged` counter was structurally always zero.
The tree's own live test said so in a comment: the "protocol-derived
acknowledgement" it correlated was **manufactured in-process**.

The other half of the mechanism was already here. This router's **own**
floodfill service answers a direct-peer store that carries a nonzero reply
token, and the `DeliveryStatus`'s message id **is** that token — so the
router's floodfill service was answering an acknowledgement the router's
publication never asked for.

**What changed.**

- The store message carries the correlator's **minted** token, in the
  reference's **direct-reply** form: a nonzero token, a **zero** reply
  tunnel id (the reference's *"zero tunnelID means direct reply"*), and the
  local hash as the reply gateway.
- The correlator publishes an **explicitly registered record**, so the
  store message's key, its reply gateway and its payload are provably
  **one** record. A publication can no longer store a record under the
  wrong key.
- The daemon composes **no** store message of its own any more. The
  one-shot publication and the router-level refresh share the
  correlator's builder and its attempt book, so there is one rule and one
  book rather than two.
- The **inbound owner** — the SSU2 service's own loop — joins the
  publication: an inbound `DeliveryStatus` is correlated with the attempt
  it names. The controlled NTCP2 probe path keeps its own table and its
  own behaviour; the publication's correlation is a separate match, scoped
  to the transport the publication uses.
- A **delivery** admission and a **network acknowledgement** are counted
  separately. A store the transport accepted is not a record the network
  holds, and neither is allowed to stand in for the other.
- An acknowledgement that matches **no** live attempt is **refused** and
  counted under its own reason. It never confirms anything and is never
  dropped silently.

**What is not claimed.** That a **stock** floodfill answers is still
blocked on Plan 493's authorized two-host topology, and no live claim is
made. Plan 515's finding **F1** — the reference's resend burst — stays
open: the reference retries on a drop and stops on a confirmation, so a
faithful burst needs this confirmation and is a successor's subject.

### The publication's retry (Plan 517)

Plan 515 made the publication **repeatable**. Plan 516 made it **observable**.
Plan 517 makes it **retry**.

The reference does not publish once and wait 39 minutes. It publishes, and while
the answer has not arrived it asks the **next closest** floodfill
(`libi2pd/RouterContext.cpp:1492`, `:1502`), excluding the ones it has already
asked (`:1540`), repeating every `ROUTER_INFO_CONFIRMATION_TIMEOUT` milliseconds
(`:37`, `:1560`) until one acknowledges (`:1321-1328`) or the exclusion set
reaches its bound of fifteen (`:38`).

**What the module owns** (`src/router_info_publication.rs`):

- the **cycle** — a bounded ladder of **batches**. A batch is the closest
  eligible targets the cycle has not already asked, so the **exclusion set**
  means a target is never asked twice in one cycle;
- the **deadline** — the reference's `1600` milliseconds, measured from the
  moment the batch went **out**. The module takes the reading from its caller
  through a `PublicationClock` and never reads a clock itself, so the whole
  ladder is provable without a wall clock, a socket, or a sleep;
- the **retry** — `advance_publication_cycle`, which abandons the awaited batch
  and asks the next one.

**Where the retry lives.** The periodic tick **begins** a cycle, and the
loop's existing `100` millisecond relay tick **advances** it when the deadline
passes. No new timer is introduced and the loop's wake-up cadence does not
change. A **confirmation** ends the cycle — the reference clears its exclusion
set and schedules the next publish at the full interval — and a delivery
**refusal** advances at once rather than waiting out the timeout.

**The cycle cannot get stuck.** The publication decides what the outcome means
for the cycle in **one** place: a batch that went out is awaited, a refused
delivery is abandoned at once, and anything else **halts** the cycle. A cycle
that is running with nothing to await would never advance, and it would hold
the next period's cycle out.

**Two divergences are recorded rather than imitated.** The tree's batch is
Plan 515's fan-out of **three** rather than the reference's one target at a
time, and a cycle that reaches the reference's bound of fifteen **ends** and
lets the next period start a fresh one, where the reference clears its
exclusion set and publishes again inside the same period.

**What is not claimed.** That a **stock** floodfill answers, and that the
retry therefore reaches one, is still blocked on Plan 493's authorized
two-host topology. The retry shares the SSU2 service's loop with the
publication's own delivery, so a slow dial delays the loop's other work; that
is the same characteristic Plan 515's refresh already has, and separating the
publication onto its own task is a successor's subject.

### The publication's freshness (Plan 518)

Plan 515 made the publication periodic. Plan 516 made it confirmable. Plan 517
made it retry. All three re-send the **same bytes** — and those bytes carry the
publication date the router signed at **startup**, so the record ages out of the
network's view.

The netdb's acceptance window is `DEFAULT_MAX_AGE`, 24 hours
(`crates/i2pr-netdb/src/router_info.rs:20`), and the reference's own netdb
expires routers between one and a half and twenty-seven hours
(`libi2pd/NetDb.hpp:49-51`). After a day of uptime the router keeps
republishing a record the network will not accept.

**The reference re-dates and re-signs its record before each publish**
(`libi2pd/RouterInfo.cpp:1218`, `libi2pd/RouterContext.cpp:1474`,
`libi2pd/RouterContext.h:33`). This tree now does the same:

- `src/router_i2np.rs` — `refresh_local_router_info_record` re-dates and
  re-signs the installed record with the router's own key, **once per cycle**,
  and installs the fresh record so the RouterInfo the router hands out in
  `SessionConfirmed` carries the fresh date too;
- `src/router_info_publication.rs` — `record_is_publishable` is the
  publication's **staleness rule**. It is runtime-neutral, it takes the reading
  from its caller, and it uses the netdb's **own** window. A record the network
  would refuse is refused and counted (`refused_stale_record`), never published;
- `src/lib.rs` — the publication tick re-dates the cycle's record and hands
  it to the owner; the relay tick, which asks the cycle's later batches, does
  not re-date and does not re-read the installed copy.

**One record per cycle.** The cycle's record lives in the owner, and the
owner is the only source of it. The first batch and every rung therefore
publish the same bytes — not because the runtime's installed copy happens to
agree, but because the record is decided once, at the cycle's start, and
nothing can change it under the cycle.

**Only the date and the signature change.** The signed region is re-encoded
from the record's own decoded parts, so the addresses, the peers and the options
are preserved, and the router's **hash** does not change: a RouterInfo's key is
derived from the router identity, not from the signed bytes.

**What is not claimed.** That a **stock** floodfill accepts the re-dated record
in the live network is still blocked on Plan 493's authorized two-host topology.

### The publication's own task (Plan 519)

Plan 515 made the publication periodic. Plan 516 made it confirmable. Plan 517
made it retry. Plan 518 made the record current. All four ran the publication
**inside** the SSU2 service's `select!` loop.

That loop's publication arm awaited a session establishment. While that await
was pending the loop could not run its other arms: the relay drain and the
inbound acknowledgement owner both waited. The transport's **socket** I/O was
not the victim — `i2pr-runtime` owns the sockets, the timers and the handshake
machinery, so a handshake completed while the daemon's service loop was
stalled. What was lost was the **service layer's** other work. This was the
**Medium** finding `M1` that Plan 517's and Plan 518's closure records both
carry open.

**Neither reference blocks its transport's loop on a handshake.** i2pd 2.61.0's
`Transports::SendMessage` posts the send and the publisher **ignores** the
returned future (`libi2pd/Transports.cpp:483-496`). Java I2P 2.13.0 runs the
publication as a **job on its own pool** (`PublishLocalRouterInfoJob`,
`JobQueue.java:163`), hands the store to a `FloodfillStoreJob` with its own
`PUBLISH_TIMEOUT = 90 s` (`FloodfillNetworkDatabaseFacade.java:174`,
`:246-257`), and delivers it through `commSystem().processMessage(m)`, so the
transport owns the dial (`StoreJob.java:415-441`).

**What changed:**

- `src/lib.rs` — `run_router_info_publication_task` holds the owner (the
  schedule, the cycle, the correlator and the cycle's record), the transport
  capability, the identity, the store handle and the acknowledgement inbox.
  The composition root spawns it under the SSU2 service's own **child scope**
  with an explicit cancellation, so it is owned, cancelled and joined with the
  service: no ownerless task and no detached one. The service's `select!` loop
  keeps **none** of the publication's state and never awaits it;
- `src/router_info_publication_task.rs` — the **bounded** acknowledgement
  handoff. The loop's hand-over never waits for room; a full channel counts the
  acknowledgement as **unmatched** and the task folds the refusals into the
  publication's own counters;
- `src/router_info_publication.rs` — the **established-session preference**
  and the publication's own **bounded delivery timeout**;
- `src/router_i2np.rs` — `Ssu2PublicationHandle`, the narrow cloneable
  capability the task holds. The service's own handle owns the runtime's
  inbound handoff and is not cloneable; the capability carries the same shared
  runtime state and grants nothing the daemon did not already hold.

**The ticks moved with the publication.** The `39`-minute period tick and the
ladder's `100`-millisecond tick belong to the publication's task, so the
transport's service loop gains **no** timer and its own wake-up cadence does
not change.

**The preference is a preference, not a gate.** The batch prefers a peer the
router is already connected to, because that is the peer whose send needs no
handshake, and the targets it drops for want of one are counted. When the
batch holds **no** session, it is asked anyway and the **direct route**
establishes the session under the publication's own timeout — i2pr installs no
exploratory tunnel, so the direct route is the only one it has.

**What is not claimed.** That a **stock** floodfill accepts the router's record
in the live network is still blocked on Plan 493's authorized two-host topology.
The **live-dial** half is not re-measured either: the lane drives the
publication's public behaviour without a socket, so what stands between the
publication and the transport's loop is the structural guard, which reads the
production files.

### The router's own published address (Plan 520)

The router publishes its own RouterInfo, and that record has carried one SSU2
address since the tree began: `generate_identity_with_keys` builds it from the
**bound** endpoint with the options `host`, `port`, `v`, `s` and `i`. The
address was **unqualified** — it omitted the `caps` and `mtu` options the
specification's SSU2 form carries and the netdb's own
`is_qualified_ssu2_address` demands — so the record named an endpoint no
peer could qualify. The reference publishes both
(`libi2pd/RouterInfo.cpp:1434-1449`, `:281`).

**What changed:**

- `src/self_publication.rs` — the owner. It asks the runtime for the
  material, hands the transport's address to the netdb's own qualified
  builder, installs the record, withdraws it when the evidence lapses, and
  counts every outcome under its own reason. It starts no task, no timer and
  no listener, and it records **no** reachability signal of its own;
- `crates/i2pr-netdb/src/local.rs` —
  `LocalRouterInfoBuilder::build_with_ssu2_address`, the **non-floodfill**
  addressed builder. It validates the address with the netdb's own
  `is_qualified_ssu2_address`, takes **no** floodfill permit, and therefore
  cannot mint `caps=f`;
- `src/config.rs` — `ssu2.advertise` stops being a parse-time error and
  becomes an explicit opt-in that is **off** by default, refused unless the
  SSU2 service itself runs and refused outside the controlled loopback
  profile;
- `src/router_i2np.rs` — the socket and the service boundaries refuse the
  opt-in by **profile** rather than refusing it outright;
- `src/lib.rs` — the composition root builds the owner from the operator's
  opt-in and refreshes the record inside the publication's cycle, **before**
  the cycle re-dates and publishes it;
- `crates/i2pr-runtime/src/ssu2_runtime.rs` — the install gate no longer
  *requires* an SSU2 address. It still binds every address the record carries
  to the router's own bound endpoint with the router's own keys, and still
  refuses two conflicting ones, but the transport's **endpoint-less** form is
  now installable — which is what makes the withdrawal possible.

**The decision has one owner.** The transport's `build_publication_snapshot`
renders the direct form — host, port, the family capability and the
transport's MTU — only when the router's own reachability evidence is
`Reachable`. The daemon never re-implements that rule: the owner asks the
runtime for the material and takes the transport's answer, and the guard
proves the daemon's own copy of the rule does not exist.

**The withdrawal is the expiry.** When the evidence lapses the published
record returns to the netdb's ordinary **zero-address** form, so the router
does not keep publishing an endpoint it can no longer justify. The withdrawal
is counted under its own reason, exactly like every other refusal.

### The router's own peer test (Plan 521)

Plan 520 made the published SSU2 address **qualified** and gated its
publication on the router's own reachability evidence, and it closed with
the finding that the ordinary record could not be earned in production: the
direct form needs **three** distinct corroboration classes, and the third —
a confirmed peer test — was recorded in production only by the controlled
floodfill activation. Plan 521 closes that gap at its source.

**What changed:**

- `src/peer_test.rs` — the bounded owner. The outstanding-probe table is
  keyed by the probed identifier **and** the probed timestamp, so an echo
  matches the probe that produced it, and a probe completes only when the
  answer came from the peer the probe was sent to. One probe at a time per
  peer; a second attempt is refused, not queued. The table is a ceiling that
  refuses rather than evicts. A second bounded table remembers the keys the
  router has already **answered**, so the reply traffic a peer can provoke
  is bounded by the router's own table and not by the peer's patience;
- `src/service_product.rs` — the responder sits on the **live inbound path**
  and reads the dispatcher's **classified** outcome, so the inbound message
  is decoded exactly once and the responder runs no decoder of its own. An
  echo of the router's own probe completes the probe and is owed no answer;
  anything else is owed an answer that repeats the probed fields unchanged.
  The owner is driven by the product's **existing** tick: it starts no task,
  no timer and no listener;
- `src/config.rs` — `ssu2.peer_test` is an explicit opt-in, **off** by
  default, and refused unless the SSU2 service itself runs;
- `crates/i2pr-runtime/src/ssu2_runtime.rs` — the production recorder. It is
  distinct from the controlled mirror, it records only a confirmed outcome,
  and it is refused after shutdown. The caller names the **link** the
  exchange travelled on and the runtime derives the address family from
  that session's own address, so the daemon can neither assert a family nor
  record an unmatched exchange;
- `crates/i2pr-transport/src/reachability.rs` — `ConfirmedPeerTest`, a
  **sealed** proof of a matched round trip. It has no constructor that takes
  a boolean or a claim, so the unmatched and timed-out paths cannot mint
  one.

**The transport's tracker remains the single authority.** The daemon never
computes a reachability state, never reads one, and never writes a state into
a published record; the transport's corroboration policy is untouched.

**The signal is about this router.** The recorded evidence belongs to the
router's **own** reachability and is never written into a peer's profile.

### `bootstrap_daemon` (`src/lib.rs:5166`) — ordered startup

```
bootstrap_daemon(&Config, now_seconds, offline_reseed_path)
│
├─ IdentityStore::in_data_dir(config.router.data_dir).load()      // 1. identity
│    └─ fail closed -> DaemonError::RuntimeIdentity
├─ LocalRouterInfoBuilder::new(&bundle)
├─ RouterInfoStoreConfig::new(netdb.max_records, netdb.max_encoded_bytes)
├─ Bootstrap::new(store_config, config.reseed.clone())
│    [+.with_offline_reseed_path(path) when supplied]
├─ BootstrapPolicy::from_config(&config)
└─ Bootstrap::run(data_dir, &builder, policy, now_seconds)        // 2..5
     ├─ revalidate the persistent RouterInfo cache
     ├─ construct + self-validate the local RouterInfo
     ├─ recompute bootstrap readiness
     └─ at most ONE bounded reseed attempt when reseed.enabled
          └─ fail closed -> DaemonError::RuntimeBootstrap
```

The function **never opens sockets, never performs DNS, and never contacts I2P peers**. The
offline SU3 bundle is the only allowlisted acquisition path; HTTPS reseed is deferred. It
returns both the sanitized `BootstrapReport` and an `Arc<Mutex<Bootstrap>>` so long-lived
runtime adapters can observe the in-memory store without re-running a pipeline stage.
`BootstrapState` is a bounded seven-variant enum: `Empty`, `CacheSufficient`,
`ReseedRequired`, `Reseeding`, `ReadyForNetworkIntegration`, `DegradedInsufficientPeers`,
`Failed`.

### Composition (`src/lib.rs`)

`execute(cli)` is the pure dispatch hub: `CheckConfig` -> `Config::load` -> `Validated` (no
side effects); `Identity::Generate` -> `OsRng` -> `RouterIdentityBundle::generate` ->
`store.save_new` (atomic); `Identity::Inspect` -> `store.load` -> `IdentityInspected` (no
secrets); `Run` -> `Config::load` -> `RunReady` (or `Validated { dry_run: true }`).

`run_daemon(config)` (`src/lib.rs:5345`) is the real daemon path:

1. Compute wall-clock seconds.
2. Run `bootstrap_daemon(&config, now_seconds, None)`.
3. Build one `InspectionHandles` from config and one `ServiceLifecycleController`.
4. Build the service graph via `build_daemon_graph_inner` (Plan 288 shares one
   `InspectionHandles` across the SAM/I2CP/I2PControl factories; Plan 295 installs the log
   ring, metrics, ban ledger, and static attestations; the SSU2 factory publishes its runtime
   service). **Plan 497** registers the `netdb-participation` service in the same graph
   (`src/lib.rs:958``). It is `Optional` in **both** `ServiceClassification` and
   `StartupRequirement`, so a broken participation owner degrades that one feature and
   nothing else, and it does not gate router readiness.
5. Publish the local RouterInfo base64 through `MAX_LOCAL_ROUTER_INFO_BYTES`.
6. Create the `Supervisor` with the graph, run it alongside the signal owner, then keep the
   normal SSU2 owner active through the bounded service-group drain before cancelling the
   supervisor and joining its scopes.

`main()` (`src/main.rs`) is the outermost shell: `Cli::parse()`, `execute(cli)`, and on
`RunReady` `i2pr_runtime::run_blocking(i2pr_daemon::run_daemon(config))` — the binary is
**not** synchronous; it hands an `async fn` to the runtime owner.

`ServiceProduct` has exactly **one** `pub` entry point, `ServiceProduct::start`
(`src/service_product.rs:2407``); `ServiceProduct::start_over_existing_daemon` is
`pub(crate)`, the normal-daemon adapter that reuses an already-running SSU2 owner. Build the
one shared `ServiceTunnelManager` with `build_shared_service_manager` (Plan 337) and inject
the same `Arc` into both the control state and the destination-group product.

### NTCP2 qualification profiles and the activation gate

`[transport.ntcp2].qualification_profile` selects one of three values. The
key has no default that silently activates anything: the default profile is
`disabled`, and an **enabled** NTCP2 surface must name a profile explicitly.

| `enabled` | `qualification_profile` | Outcome |
| --- | --- | --- |
| `false` | `"disabled"` | accepted; the transport stays off |
| `false` | `"controlled_loopback"` | refused — `controlled_loopback requires enabled = true` |
| `false` | `"private_lan_qualification"` | refused — `private_lan_qualification requires enabled = true` |
| `true` | `"disabled"` | refused — `enabled NTCP2 requires the explicit controlled_loopback or private_lan_qualification profile` |
| `true` | `"controlled_loopback"` | accepted; the bind must be loopback |
| `true` | `"private_lan_qualification"` | accepted; the bind must be an RFC1918 IPv4 literal |
| `true` | any other value | refused — `enabled NTCP2 requires the explicit controlled_loopback or private_lan_qualification profile` |

**The private-LAN profile is the operator's, and it is narrow.** It requires
all of: `enabled = true`; exactly one RFC1918 IPv4 `bind_address` literal
(`10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`); an explicit nonzero
`listen_port` of at least 1024; a network id representable by the handshake
width; and **family coherence with SSU2** — the `[ssu2]` surface must itself
be in `private_lan_qualification` mode and bind an RFC1918 IPv4 address,
because one RouterInfo carries both transports' endpoints.

Every refusal is a `ConfigError::Semantic` that names the offending field and
states the requirement. The bind-address shapes are refused separately:

| Field | Reason |
| --- | --- |
| `transport.ntcp2.bind_address` | `private_lan_qualification requires an explicit RFC1918 IPv4 address literal; an empty bind address is not a private-LAN endpoint` |
| `transport.ntcp2.bind_address` | `private_lan_qualification requires an explicit RFC1918 IPv4 address literal` |
| `transport.ntcp2.bind_address` | `private_lan_qualification requires an RFC1918 IPv4 address; loopback is not a private-LAN address` |
| `transport.ntcp2.bind_address` | `private_lan_qualification requires an RFC1918 IPv4 address; the unspecified address is not a private-LAN address` |
| `transport.ntcp2.bind_address` | `private_lan_qualification requires an RFC1918 IPv4 address (10.0.0.0/8, 172.16.0.0/12, or 192.168.0.0/16)` |
| `transport.ntcp2.bind_address` | `private_lan_qualification requires an RFC1918 IPv4 address; IPv6 literals are not supported` |
| `transport.ntcp2.listen_port` | `private_lan_qualification requires an explicit nonzero port` |
| `transport.ntcp2.listen_port` | `private_lan_qualification requires a port of at least 1024` |
| `network.network_id` | `private_lan_qualification requires a network ID representable by the handshake` |
| `transport.ntcp2.qualification_profile` | `private_lan_qualification requires the ssu2 surface to use the private_lan_qualification profile` |
| `transport.ntcp2.qualification_profile` | `private_lan_qualification requires the ssu2 surface to bind an RFC1918 IPv4 address` |

Note that a public IPv4 literal and TEST-NET `192.0.2.0/24` share the
"requires an RFC1918 IPv4 address (10.0.0.0/8, 172.16.0.0/12, or
192.168.0.0/16)" reason, because both fail the same `is_private()` test; the
loopback, unspecified, and IPv6 shapes each carry their own reason.

#### The gate (`src/ntcp2_activation.rs`)

`ntcp2_activation_scope(&Ntcp2Config) -> Result<Ntcp2DialScope, Ntcp2ActivationRefusal>`
is **pure**. It takes an already-normalized configuration and returns either
the address scope the transport may dial or a typed refusal. The purity is
deliberate: the configuration layer already refuses every shape the gate
refuses, so a gate branch inside the service closure could never be reached
from a configuration file. Extracting it makes every branch reachable by a
unit test.

The refusal vocabulary is five variants, each with its own reason text:
`Disabled` ("NTCP2 is disabled"), `NoExplicitProfile` ("NTCP2 requires an
explicit qualification profile"), `NonLoopbackBind` ("the NTCP2
controlled-loopback profile requires a fixed loopback bind address"),
`NonPrivateLanBind` ("the NTCP2 private-LAN profile requires an RFC1918 IPv4
bind address"), and `PrivilegedPort` ("NTCP2 requires an explicit
non-privileged listen port").

The gate is called from two places, so there is **one authority** for the
decision:

- the composition root in `src/lib.rs`, which fails daemon startup with the
  refusal's reason; and
- the `ntcp2-router` service closure, which applies the same gate as defense
  in depth. That service is registered with
  `StartupRequirement::Optional`, so a refusal there degrades that one
  service and the rest of the router keeps running.

The gate never widens a scope. `controlled_loopback` yields
`Ntcp2DialScope::Loopback`; `private_lan_qualification` yields
`Ntcp2DialScope::PrivateLan`. The scope is always the caller's and is never
widened inside the runtime.

#### What the profile does not do

- **It publishes nothing.** The operator profile adds no publication path.
  The NetDB publication path still requires an `Active` floodfill role
  **and** an in-memory `FloodfillAdvertisementPermit`, and neither is
  reachable from this profile. `[ssu2] advertise = true` remains refused at
  parse time (`SSU2 address publication is unavailable in this milestone`).
- **It advertises no capability.** The signed dual-transport record carries
  **no capability letter** — no `caps=f`. The support inventory is unchanged.
- **It mints no reachability permit.**
- **It is not a public transport activation.** Public transport activation
  remains unclaimed: it requires external two-family evidence, which needs an
  authorized two-host topology this profile does not supply. What this profile
  delivers is the source-side activation substrate, not an activation.
- **The two families do not share one link registry.** SSU2 owns a private
  `i2pr_transport::TransportManager`; NTCP2 owns its own admission
  structures. What they share is one delivery registry
  (`Ntcp2RouterDeliveryRegistry`), one bandwidth governor, one authenticated
  I2NP dispatcher, and one local-RouterInfo channel.

The dual-transport RouterInfo builders are scope-parameterized:
`generate_scoped_dual_transport_router_info` copies both endpoints from the
live owners' validated configuration and identity-bound stores and requires
both to fall inside the scope, with matching address families and unprivileged
ports. `generate_controlled_dual_transport_router_info` (loopback) and
`generate_private_lan_dual_transport_router_info` (RFC1918) both delegate to
it. The private-LAN record carries the operator's two socket-derived private
endpoints and is **held locally only**: the operator hands it to the lab peer
out of band.

### `NetDbSeam` and the reply-path flip (`src/netdb_seam.rs`)

`path_status()` returns `ExploratoryPathStatus::Available` when the injected
`i2pr_netdb::ReplyPathProvider` reports at least one valid inbound tunnel, and
`BlockedExploratoryTunnelUnavailable` otherwise. `set_reply_path_provider` accepts any
`Box<dyn ReplyPathProvider>`; the production wiring is the
`i2pr_tunnel::ExploratoryPoolReplyPathProvider` adapter installed when a registered inbound
exploratory tunnel activates. **A peer transport link is not a complete reply path.**

`composition_outcome_with_registry` derives the readiness outcome from the real
`DataPlaneRegistry` state at the supplied deterministic time; the legacy caller-set readiness
bits are deprecated. `CompositionOutcome` is the bounded vocabulary
(`NeedInboundExploratory`, `NeedOutboundExploratory`, `LookupReadyForTunnelDispatch`,
`NoEligibleCandidates`).

Plan 122 adds a **separate** LeaseSet2 reply-path provider so Plan 117 router-side
exploration is not consulted for destination lookups: `begin_lease_set2_lookup`,
`advance_lease_set2_after_path`, `ingest_lease_set2_response`, `ingest_lease_set2_store`,
`lease_set2_delivery_outcome`, `cancel_lease_set2_lookup`, `active_lease_set2_lookup`.

### Peer participation driver (`src/netdb_participation.rs`)

Plan 488's daemon-side owner of the `i2pr-netdb` peer profile store. It
does three things and nothing else:

- **feeds** the store with the outcomes the router observes — answered,
  answered without the record, refused, a reply that does not match the
  requested key, not answered, unreachable;
- **runs the maintenance pass** on the daemon's clock, so an aged failure is
  demoted and then dropped instead of lingering forever;
- **emits receipts** that make the observed state visible without disclosing
  anything about a peer.

The driver keeps an **explicit clock**: `advance_time(now_ms)` and
`tick(now_ms)` are the only ways time enters it, so every observation,
maintenance pass and selection is reproducible. `run_profile_maintenance`
is the owned periodic future the daemon's service graph runs under the
runtime's child scope; it owns its cancellation, refuses a maintenance
period outside `[1 ms, 1 h]`, and returns a typed exit reason. There is no
ownerless spawn.

`ParticipationReceipt` carries counts, stable tier and state labels, and a
SHA-256 digest over those counts. It never carries a peer's hash, address,
key material, session state, or payload — the private-mesh lane asserts
that a receipt rendered in full carries no peer hash.

`ProjectionLoadOutcome` is the persistence contract: a projection that
round-trips is `Restored { restored, orphaned, fresh }`; a corrupt or
oversized projection is `Discarded { reason }` and is **counted**, never a
startup failure.

**The selection policy is not wired into the router's tunnel building in
this phase.** Plan 488 delivers the policy and its inputs; the wiring is the
consumer's change and belongs to the participation phases that follow.

### Routing table driver (Plan 489, `src/netdb_participation.rs`)

Plan 489's daemon-side owner of the `i2pr-netdb` floodfill-indexed routing
table. It extends the Plan 488 module rather than adding a second one, so the
two drivers share one explicit-clock convention and one receipt discipline.

It does four things and nothing else:

- **indexes** the floodfills the router learns about. A peer whose own signed
  `RouterInfo` does not advertise `f` is refused and counted;
- **runs the exploration job**: it issues an exploratory `DatabaseLookup` —
  wire lookup type `3` — with a random search key, and the peers the answer
  names enter the table on a verified floodfill's evidence;
- **drives the optimized lookup**: the candidates come from the routing table,
  closest to the requested routing key first, split into the peers the router
  may ask directly (*queryable*) and the peers whose `RouterInfo` must be
  resolved first (*pending*);
- **emits receipts** that make the table's state visible without disclosing
  anything about a peer.

The table is anchored at the router's own position in today's keyspace
(`daily_routing_key(local_router_hash, date)`), so the partition rotates with
the UTC day. The driver keeps an **explicit clock** like its Plan 488 sibling:
`advance_time(now_ms)` is the only way time enters it, so every exploration,
rotation and lookup is reproducible.

A lookup without a reply path is **not dispatchable**: `begin_lookup` and
`begin_exploration` return `NeedsReplyPath` until the runtime supplies a
`ReplyPath`. A direct peer link is not a reply path, and the state machine
refuses to emit a `DatabaseLookup` without one.

A peer the table knows but whose `RouterInfo` is unresolved is **never asked**:
`begin_lookup` returns `NeedsPeerResolution { pending }` instead, because a
peer without an address cannot be reached.

`RoutingReceipt` carries counts — the table size, the occupied buckets, the
signed and suggested evidence counts, the queryable/pending/refused split — and
a SHA-256 digest over them. It never carries a peer's hash, address, key
material, session state, or payload; the private-mesh lane asserts that a
receipt rendered in full carries no peer hash.

**Plan 500 adds the terminal-state counts.** A `RoutingReceipt` now also
carries `terminal_counts`: the number of lookups that terminated in each
final state, keyed by the state's stable label. The labels come from
`i2pr_netdb::LookupFinalState::label()`, so the same final state always lands
in the same bucket — and, in particular, a lookup that spent its per-lookup
peer budget (`peer-budget-exhausted`) is counted **separately** from a lookup
that ran out of candidates (`peer-exhausted`). The two are different events
with different causes and different remedies, and folding them into one
bucket would make the peer budget invisible in the diagnostics. The counts
are cumulative over the driver's lifetime and carry no peer identity.

`RoutingProjectionOutcome` is the persistence contract: a projection that
round-trips is `Restored { peers }`; a corrupt, oversized or expired
projection is `Discarded { reason }` and is **counted**, never a startup
failure.

**The routing table and the optimized lookup are not wired into the router's
tunnel building in this phase.** Plan 489 delivers the table and the policy
that draws from it; the wiring is the consumer's change and belongs to the
participation phases that follow. The `[netdb]` configuration section exposes
the routing-table and exploration keys, and **both default to off**; the
exploration cannot be enabled while the routing table is off, because an
exploration with nowhere to put the peers it discovers is refused at parse
time. Plan 497 consumes the table's *ordering* — see the next section — but
the table still never answers a lookup on its own.

### The routing-table consumer and the `netdb-participation` service (Plan 497)

Plan 497 turns the Plan 489 routing table from a driver with no production
caller into a consumer seam the daemon owns.

**`RoutingTableConsumer`** (`src/netdb_participation.rs:652``) is that seam. It
owns an `Arc<Mutex<RoutingTableConsumerState>>` holding the Plan 489 driver and the
operator's `enabled` switch, plus four counters held **outside** the lock.
`candidates(store, routing_key, excluded)` (`src/netdb_participation.rs:818``)
returns either `RoutingTableCandidates::Table(Vec<RouterHash>)` — the table's
closest-first verified floodfills for the key — or
`RoutingTableCandidates::Fallback(RoutingTableFallback)`, where the reason is
`Disabled` (the table is off), `NoCandidate` (the table holds no queryable
candidate for the key), or `Unavailable` (the table's lock is poisoned, so the
table cannot be reached at all). **Every fallback is counted by reason**:
`RoutingTableCounters { answered, fell_back_disabled, fell_back_no_candidate,
fell_back_unavailable }` (`src/netdb_participation.rs:680`), with `fell_back()` the
saturating sum of the three fallback counters
(`src/netdb_participation.rs:695``).

The counters live outside the lock on purpose. A poisoned table cannot answer, and
that fallback must still be counted: a fallback that is not counted is a **silent**
fallback, which is exactly what the plan forbids. `Unavailable` is a distinct
reason so an operator can tell "the table had no peer for this key" apart from
"the table is broken".

The table is a **cache of derived state**; the flat peer map stays the
**authority**. A table that cannot answer a key never fails the lookup — the
caller falls back to the flat store's selection, and the reason is counted
either way.

**The daemon service.** `NetDbParticipation` (`src/lib.rs:1864`) is registered in
the service graph under the name `netdb-participation`
(`NETDB_PARTICIPATION_SERVICE_NAME`, `src/lib.rs:1546``) with
`ServiceClassification::Optional` (`src/lib.rs:2365``) **and**
`.startup_requirement(i2pr_core::StartupRequirement::Optional)`
(`src/lib.rs:2398``). It owns the peer-profile driver and the shared
`RoutingTableConsumer`, runs `run_profile_maintenance` on a bounded cadence, and
owns a routing-table owner that populates the table from the daemon's own NetDB
floodfill advertisers and persists the projection.

**The startup sequence** (`src/lib.rs:1897``). When the service starts it loads
the projection, advances the routing table's clock so today's date is used, and
populates the table from the daemon's own NetDB. Two owned tasks then run on the
configured cadence under the service's child scope: the profile maintenance
owner (`run_profile_maintenance`) and the routing-table owner, which advances the
clock, refreshes the table, and persists the projection. Every peer the table
holds is indexed with its **own signed evidence** (`src/netdb_participation.rs:2344``); a peer's
*suggested* evidence now has a source, because **Plan 513** composes Plan 489's
exploration into the service graph: the live discovery spine names peers and
the table accepts them as suggestions.

**Inert by default.** `netdb.routing_table_enabled` defaults to `false`
(`default_netdb_routing_table_enabled`, `src/config.rs:1119`). When it is false the
service is **inert**: no population, no tick, no persistence — the service body
only signals readiness and then awaits cancellation (`src/lib.rs:2393``). Readiness
means "this service is running" under Plan 360's contract; it deliberately does
**not** wait for the first maintenance pass, the first table population, or the
projection load, because each is bounded, local and failure-tolerant.

**The additive configuration key.** `netdb.participation_maintenance_period_ms`
defaults to `60_000` ms (`default_netdb_participation_maintenance_period_ms`,
`src/config.rs:1148`) and is bounded on both sides:
`MIN_PARTICIPATION_MAINTENANCE_PERIOD_MS = 1_000` (`src/config.rs:53`) and
`MAX_ALLOWED_PARTICIPATION_MAINTENANCE_PERIOD_MS = 3_600_000`
(`src/config.rs:46`). A value outside that range is refused at parse time with
`ConfigError::Semantic` naming the field (`src/config.rs:3830`,
`src/config.rs:3836`). The key is **additive** — it lives inside the existing
`deny_unknown_fields` `[netdb]` section and changes no other key's meaning.

**The startup projection** — this discharges Plan 488's finding F4 ("the
projection is not written to disk"). The daemon reads one persisted peer-profile
projection from `<data_dir>/peer-profiles/` at startup. The directory name is
`PROFILE_PROJECTION_DIRECTORY` (`src/lib.rs:1809`); the file name is
`profile-projection-<yyyyMMdd>.txt` (`PROFILE_PROJECTION_FILE_PREFIX` and
`PROFILE_PROJECTION_FILE_SUFFIX`, `src/lib.rs:1812`, `src/lib.rs:1815`); and the
startup scan examines at most `PROFILE_PROJECTION_SCAN_LIMIT = 64` directory
entries (`src/lib.rs:1822`), so a hostile or accidental directory cannot turn the
router's startup into an unbounded walk.

**"Current" is carried by the file name, not by the format.** `ProfileProjection`
carries **no UTC-day field**, so a projection from another UTC day is discarded,
counted, and removed (`src/lib.rs:2217``). The projection **bytes** are unchanged
from Plan 488's format: the daemon reads them with the Plan 488 loader
(`PeerParticipationDriver::load_projection`, `src/netdb_participation.rs:455`),
whose `ProjectionLoadOutcome::Restored` / `Discarded { reason }` contract already
makes a discarded projection a counted outcome rather than a startup failure.

**The table-driven lookup selection.** `begin_tunnel_lookup`
(`src/netdb_tunnels.rs:503``) always computes the flat store's selection first —
that selection is the authority — and only then asks the routing table how it
would order it (`src/netdb_tunnels.rs:533``). The table's answer may only
**reorder** that selection: it can never remove a candidate the flat store could
serve, and it can never add one the flat store would not have served. A table
that cannot answer falls back to the flat store's selection, and the consumer
counts the fallback by reason. The preferred list reaches the engine through
`NetDbSeam::begin_lookup_with_preferred` (`src/netdb_seam.rs:674``).

**The coordinator is not composed into the daemon.** `NetDbTunnelCoordinator` is
**not** created by the daemon today: every `NetDbTunnelCoordinator::new` call in
the tree is in test code — the module's own `#[cfg(test)] mod tests`
(`src/netdb_tunnels.rs:996``) and the integration suites. Plan 433 owns that
composition and is registered `blocked` on Plans 431/432. Plan 497 delivers the
**seam** and the coordinator-level integration
(`NetDbTunnelCoordinator::with_routing_table`, `src/netdb_tunnels.rs:377``); the
daemon-level service composition remains Plan 433's open work. Until Plan 433
lands, the shared consumer is reachable only from this crate.

**No advertisement and no capability change.** The routing table is **internal
state**. `specs/support.toml` is unchanged by this plan — 98 surfaces (92
experimental, 5 deferred, 1 ready), 0 advertised — the published RouterInfo and
its `caps` are unchanged, and `router.version` stays `0.9.62`. The evidence for
this phase is a **private controlled mesh**, not a public-network measurement:
Plan 489 §11.1's stock-peer NetDB exploration lane and Plan 494's `caps=f`
advertisement remain unlanded, and Plan 494 stays gated on Plan 493's authorized
two-host topology.

### The live DSM version gate and the live peer-eligibility receipt (Plan 503, `src/version_eligibility.rs`)

Plan 491 attached two duties to the API version this router declares: minimum
peers a tunnel may be built through, and minimum floodfill peers a DSM may be
sent to. The **first** was enforced on the live path. The **second** was
enforced **nowhere in production**.

The router's two live `DatabaseStore` sends selected floodfills by XOR distance
alone and never consulted `dsm_target_eligibility`:

- the **creator RouterInfo** publication (`src/service_product.rs:7910``,
  reached from the live composition through the product's own tick,
  `src/service_product.rs:8082``) sorted the
  floodfill advertisers itself and truncated to three;
- the **service LeaseSet2** publication (`src/service_product.rs:9631``,
  reached from `src/service_product.rs:3486`/`:4960`/`:9035`) went through
  `DestinationTunnelCoordinator::candidate_hashes` →
  `i2pr_netdb::lookup_policy::select_floodfill_candidates`, a selection that
  filters on nothing but distance and exclusion.

The typed, counted DSM refusal Plan 491 built
(`src/netdb_tunnels.rs:835`` `begin_publication`) has **zero production
callers**, because `NetDbTunnelCoordinator` is never composed — see the
section above. The router therefore published its RouterInfo and its LeaseSet2
to floodfills that publish no version, an uncomparable version, or a version
below `0.9.62`: the exact under-compliance Plan 491 exists to prevent.

**Both live paths now consult the shared, version-gated selection** in
`i2pr-netdb` (`lookup_policy::select_version_gated_floodfills`), through two
coordinator methods:

- `DestinationTunnelCoordinator::version_gated_candidate_hashes` — the
  LeaseSet2 publication's selection, with the same routing key, the same
  target exclusion and the same three-floodfill bound as before, with the
  floor applied;
- `DestinationTunnelCoordinator::version_gated_publication_floodfills` — the
  creator RouterInfo publication's selection, with our own router hash as both
  the routing key and the exclusion.

The candidates are now considered within the policy's ceiling and the
**eligible** ones are kept, so a below-floor floodfill can no longer consume
one of the three publication slots. An empty eligible set fails closed through
the paths that already existed: the creator publication is skipped with a
debug transition, and the LeaseSet2 publication releases its reserved intent
and returns the existing typed request-construction failure. A refused
floodfill is a **refusal**, not a delivery failure, and it is counted by
reason.

**The receipt is now a live product.** `src/version_eligibility.rs` owns the
peer-eligibility decision surface: `VersionDecisionKind`,
`VersionEligibilityReceipt`, the digest, the two constructors, and
`VersionEligibilityOwner` — a bounded log (`VecDeque`, an explicit cap, the
oldest entry evicted at the cap) behind an internal `std::sync::Mutex`, so
the shared handle is a plain `Arc`. The receipt carries **counts and stable
reason labels only**: no peer hash, no address, no key, no version string, no
payload.

The coordinator records one receipt per live decision — the tunnel-build gate
(`destination_peer_candidates`) and the two DSM gates. A coordinator without an
owner records nothing, so the controlled helper and the integration lanes are
unaffected. The owner reaches the coordinator without a signature change: the
service product creates it, hands it to the coordinator, keeps it, and exposes
it through `ServiceProduct::version_eligibility`; the composition publishes it
into `InspectionHandles` (`publish_version_eligibility` /
`version_eligibility`), so the running router's decisions are observable
through the plane that already publishes the transit posture.

**The identity `examined == accepted + refused` is exact for the DSM gate**,
whose only two outcomes are eligible and refused-by-version-reason. It does
**not** hold for the build gate, whose `examined` is the whole scanned
population: the build projection has two further refusal reasons —
`unqualified` (the record lacks build metadata) and `capped_participation`
(the peer reached its participation cap) — and **neither is a version
refusal**, so neither appears in the receipt. They stay counted in
`CandidateProjectionSummary`.

**The lookup-target selection is deliberately not gated.** A `DatabaseLookup`
is not a DSM and is not one of Plan 491's two obligations; extending the floor
to the lookup path would change which floodfills the router can ask at all,
which is a product decision beyond this plan. That gap is recorded as an open
finding, not silently taken.

**No advertisement and no capability change.** `router.version` stays
`0.9.62`, `caps` and the published RouterInfo are unchanged, the support
inventory's advertised count stays **zero**, and `Cargo.toml` and
`Cargo.lock` are byte-identical. Plan 503 is **not** a stock-peer
qualification and claims no interop evidence.

### The tunnel liveness scheduler in the product composition (Plan 504, `src/tunnel_liveness.rs`)

Plan 185 delivered a tunnel-liveness scheduler for the creator side: pair
each active **outbound** exploratory tunnel with an active **inbound**
exploratory tunnel, probe through the outbound path, and correlate the
answer that comes back. It is complete, bounded and unit-tested — and it
had **never been constructed by the production daemon**. An exhaustive audit
of every `pub mod` in `src/lib.rs` — in-crate production references against
out-of-crate test references — returned exactly one module with no
production consumer: `tunnel_liveness` (prod = 0, tests = 11). The live
inbound path called the coordinator's dispatcher directly
(`src/service_product.rs:5576``, `:7571`, `:8630`, `:8860`), never the
existing `route_inbound_with_liveness` seam, so every inbound
`DeliveryStatus` echo was **discarded**: nothing correlated it, because
there was no pending test.

Plan 185's closure record says the opposite —
`plans/closure/mixed-router-interop/185-status.md:71-73` states that
`route_inbound_with_liveness` is *the single helper the daemon pump calls*.
That is false of the tree it describes. Plan 185's record is **not**
rewritten; the correction lives in Plan 504's closure record and in the
registry's row.

**The feasibility question was answered by running code, not by prose.**
Plan 504's first work package was an executable artefact that could stop
the plan, and it drives the **real** composition: it installs a real
outbound and inbound exploratory tunnel through the coordinator's own pool
and registry, composes the probe through the coordinator's real
`OutboundGatewayRole`, relays the resulting cell through the peer's real
tunnel roles exactly as a peer's answer arrives, and hands it to the
daemon's real inbound dispatch. The answer was **yes** on all three
questions — and the third one changed the design: the answer arrives
**tunneled**, through the inbound-tunnel path, not through the direct path.
Correlating only the direct path would have meant the live router never saw
its own answer, so the live correlation is wired at **both** seams.

**The correlation is now authenticated.** The dispatcher used to keep the
*transport* message id and discard `DeliveryStatusMessage::message_id` — the
id of the message being acknowledged, which is the id that identifies the
test — so the scheduler could only match "any `DeliveryStatus`, when
exactly one test is pending", as its own doc comment said.
`RouterI2npOutcome::RouterControl` now carries
`acknowledged_message_id: Option<u32>` (`Some` for `DeliveryStatus`, `None`
for the other three control bodies; the transport id is untouched and no
decoder is weakened), and the scheduler matches on it.
`record_authenticated_response` credits exactly the probe that carried the
id; an id that matches nothing is counted as an **unauthenticated response**
and credited to nothing.

**The safety gate is the point of the whole plan.** A pair becomes
**proven** only when it has produced an authenticated response, and only a
proven pair may ever be marked unhealthy. Until the router has seen that
peer answer, a missing response is indistinguishable from *this peer does
not answer*, and the router must not act on a signal it cannot distinguish
from its negation. A missing response for an unproven pair is still
counted, the withheld teardown is counted once per pair, and both counters
are published — so an operator sees that the liveness signal is not working
rather than inferring it from silence. The threshold stays two consecutive
failures, the response timeout stays 60 s (below the two-minute
idle-deletion boundary), and only **exploratory** slots are ever removed: a
destination tunnel is never removed by this plan.

**The probe reuses the canonical path.** `compose_liveness_probe` calls the
existing `compose_outbound_build_deliveries` — the same
`registry.outbound(slot)` role, the same cell composition, the same
`RouterDeliveryService` delivery as the build reply. The only change to
that shared function is that the delivery instruction became a parameter;
the build reply still passes `DeliveryInstruction::Router`. No private
parallel pipeline. The probe's payload is a `DeliveryStatusMessage` whose
acknowledged-message id **is** the probe id.

**An identifier-space collision, found and avoided.**
`DeliveryStatusMessage::message_id` is already the NetDB LS2 publication
reply token (Plan 487) — a monotonic counter from 1. A liveness id from
that space could be claimed by the publication path, so the liveness ids
live in the **high half** of the `u32` range
(`LIVENESS_PROBE_MESSAGE_ID_BASE`), disjoint from the publication token
space.

**The exploratory/destination distinction is structural, not inferred.**
`exploratory_liveness_pairs` reads only the exploratory pool's own
registration lists, so a destination-role slot — which lives in the shared
`DataPlaneRegistry` but never in the pool's lists — cannot appear in it.
No slot-number test, no heuristic.

**The measured limitation, stated plainly.** The production daemon installs
**no exploratory tunnels**: `ExploratoryBuildCoordinator::submit` is called
only from the out-of-crate lanes, while the daemon submits only
*destination* builds. So `exploratory_liveness_pairs()` returns empty in
production today and the scheduler tracks zero pairs — it is **installed and
inert**, not broken, and the invariant-9 gate is what makes that safe:
nothing is torn down. Wiring an exploratory build driver into the product
was a separate plan; **Plan 505 is that plan** (see below), and Plan 504
adds no second build mechanism.

**No advertisement and no capability change.** `router.version` stays
`0.9.62`; `caps` and the published RouterInfo are unchanged; the support
inventory's advertised count stays **zero**; `Cargo.toml` and
`Cargo.lock` are byte-identical. No new task, no new timer, no new
listener, no new socket: the scheduler runs on the **existing** 250 ms
product tick.

### The exploratory build driver in the product composition (Plan 505, `src/exploratory_driver.rs`)

Plan 504 installed the tunnel-liveness scheduler and left one thing open: the
**population** the scheduler watches did not exist.
`ExploratoryBuildCoordinator::submit` had **zero production callers** — every
call site was an out-of-crate integration lane (`tests/exploratory_build_live.rs`,
`tests/streaming_tunnel_external.rs`, `tests/netdb_tunnel_external.rs`,
`tests/destination_tunnel_external.rs`,
`tests/service_tunnels_application_genuine_remote_qualification.rs`) — so the
daemon installed no exploratory tunnels, `exploratory_liveness_pairs()` returned
empty, and the scheduler tracked **zero pairs**. The production rebuild path
(`src/service_product.rs` → `coordinator.rebuild_exploratory(...)`) was complete
but **unreachable**, because no pair existed to be removed.

The peer-profile store was in the same state: constructed in production and
**never fed**. `learn` (`src/netdb_participation.rs`) had one caller and it was
inside `#[cfg(test)]`; `select_tunnel_build_peers` — Plan 491's version-gated
selection — had no production caller at all; and the four `observe_*` calls had
none either. The store was empty in a live router.

**The mechanism was complete; only its inputs were missing.**
`rebuild_exploratory` (`src/exploratory_build.rs`) is already a complete
first-build driver with the peer-selection step removed — it allocates the three
tunnel ids from the coordinator's monotonic pool, mints two distinct message
ids, builds the `BridgeHeader::ShortTransport`, constructs the `BuildRequest`
with the direction-appropriate role and identity, and submits. `install_established`
already installs an exploratory build into the pool (`register_outbound_with_material`
/ `register_inbound_with_material`), marks it established, and activates it in the
`DataPlaneRegistry`. **One new thing was needed: a peer.**

**The peer selection is Plan 491's version-gated selection.**
`select_tunnel_build_peers` is called with `require_floodfill: false` — an
exploratory tunnel is built toward an ordinary peer, not necessarily a
floodfill — and the gate's refusals (unknown version, uncomparable version,
below floor) are added to the driver's counters **by reason**. A below-floor
peer is **refused**, never silently skipped.

**The peer material is the identity key, not the SSU2 key.**
`PeerBuildIdentityResolver` is the seam; the production resolver,
`ProfileStorePeerResolver`, reads the peer-profile store's own validated
records, so the selection and the resolver can never disagree about which peer
exists. The static encryption key is
`router_info().router_identity().public_key()` — the same derivation the
production rebuild path uses — and **not** the SSU2 address `s` option that
Plan 185's external lane found to be the wrong key.

**There is exactly one exploratory build path.** The driver submits through the
existing `rebuild_exploratory`, the same path the liveness rebuild uses, so the
first build and the rebuild cannot drift apart. The driver module contains no
second, private build path.

**The outcome map is total over all fourteen `BuildCoordinatorOutcome`
variants, and it separates a peer fault from a local condition.**

| Outcome | Observation |
| --- | --- |
| `Installed` | answered |
| `TimedOut` | unanswered |
| `HopRejected` | refused |
| `DeliveryFailed` | unreachable |
| `InvalidReply` | misbehaving |
| `Cancelled`, `PoolRejected`, `InvalidRegistration`, `InvalidPath`, `CoordinatorRejected` | **none** — this router's own condition |
| the four destination-scoped variants | never attributed to the driver |

The five local conditions are counted by the driver's own counters and produce
**no** peer-profile write. Writing any of them into a peer's profile would
blame the peer for a fault of ours and would degrade a good peer's profile
score. The contrast — a local condition writes nothing, a genuine peer fault
**does** — is asserted in one test, so the omission can never be a broken
recorder. An outcome with no in-flight attempt, a wrong direction or a
mismatched attempt id is an **orphan**: counted, and credited to nothing.

**The peer-profile store is now fed in production.**
`NetDbParticipation::learn_bootstrap_peers` learns the router's **real**
validated peers from the bootstrap snapshot — the same population the routing
table is populated from. The batch is the bootstrap's own bounded snapshot and
is learned with **one** receipt, so a large NetDB cannot churn the bounded
receipt log; the snapshot is taken under the bootstrap's lock while the batch is
learned outside it, so the two locks are never held at once. A learned peer does
**not** reset its restored observation state: `ProfileStore::insert` only
re-derives the version, the capacity class and the floodfill flag for a peer it
already knows.

**The posture is off by default.** A new `[exploratory]` configuration section
(`src/config.rs`) carries one switch, `builds`, following the `[transit]
participation` precedent, and it is `deny_unknown_fields`, so a key that looks
like an advertisement is refused rather than ignored. There is deliberately **no**
count knob: the number of exploratory pairs is bounded by the coordinator's own
pool and by Plan 504's `MAX_LIVENESS_PAIRS`, and a configuration key that could
disagree with those ceilings would be a second source of truth.

**Off by default for a concrete reason, not out of caution.** The daemon's
transport can only dial a peer it may legally reach: `Ssu2DialTarget::new` is
loopback-only and `Ssu2DialTarget::new_private_lan` accepts only RFC1918 IPv4.
A public deployment's peer population is therefore **not dialable at all**, and a
default-on driver would be a router that tries and fails. Enabling the posture
is an explicit operator decision that this router should build exploratory
tunnels inside the mesh it can actually reach.

**No new task, no new timer, no new listener, no new socket.** The driver runs
on the **existing** 250 ms group tick, beside Plan 504's liveness driver — the
scheduler it feeds. The terminal outcomes reach it on **both** the expiry sweep
and the inbound build-reply path. The counters are published through the
existing inspection seam (`publish_exploratory_driver_counters`,
`exploratory_driver_counters`, `exploratory_driver_counters_snapshot`); an
unpublished or poisoned slot is a gap, never a fabricated zero. The counters
carry counts and stable reason labels only: no peer, address, key or payload.

**The feasibility artefact is not vacuous, and that is measured.** It drives a
controlled mesh of two real SSU2 daemons against the real coordinator, profile
store and delivery seam, and asserts that the tunnel is installed in the pool
and the registry, that the liveness scheduler sees the pair, that the profile
store received the observation, and that the counters moved. The negative
control: the responder answers with the peer's **identity** encryption key,
which is what the production derivation uses. Swapping that for the SSU2 static
key — the shortcut the pre-existing `tests/exploratory_build_live.rs` harness
uses on **both** sides — makes the build fail to install.

**What Plan 505 does not close.** The validated store is still not refreshed
from live NetDB lookups, so the driver's population is the bootstrap snapshot.
Plan 497 limitation 5's **read** half stays open: the driver becomes a live
reader of the profile store, but the routing table's only reader remains Plan
433's never-constructed `NetDbTunnelCoordinator`. Plan 487 F3's
destination-inbound half still needs tunnel rekeying, which the tree does not
implement at all. **No stock-peer evidence is claimed**: whether an ordinary
public router accepts a driver-initiated one-hop exploratory build, and whether
it relays a liveness probe back, is **unproven**.

**Superseded in part by Plan 506** (see below). The first sentence above is
wrong in a way that mattered: the router does **not** learn peers from a NetDB
lookup at all -- a `DatabaseSearchReply` carries peer **hashes only** and the
production router runs **no** RouterInfo-kind lookup. The live-learned
RouterInfos are exactly the unsolicited `DatabaseStore`s, and Plan 506
installs the reconcile that carries them into the two stores the driver and
the destination coordinator build with. The driver's population is therefore no
longer the startup snapshot.

**No advertisement and no capability change.** `router.version` stays
`0.9.62`; `caps` and the published RouterInfo are unchanged; the support
inventory's advertised count stays **zero**; `Cargo.toml` and `Cargo.lock` are
byte-identical.

### Outbound/inbound composition roles

`OutboundGatewayRole` and `LocalInboundEndpointRole` are **not** daemon types — they are
defined in `i2pr_tunnel::roles`. The daemon composes them:
`outbound_lookup.rs` drives `OutboundGatewayRole::forward_cells` against a `Router`-delivery
`TunnelPayloadHeader` and packages the resulting `TunnelData` cells as complete
short-transport I2NP messages addressed to the outbound first hop;
`inbound_dispatch.rs` routes one `TunnelDataMessage` by `tunnel_id` to the activated
`LocalInboundEndpointRole` in the `i2pr_tunnel::DataPlaneRegistry`, decodes the recovered
standard I2NP envelope exactly once through `i2pr-proto`, and supports only `DatabaseStore`,
`DatabaseSearchReply`, and `DeliveryStatus` bodies. Unknown tunnel ids fail closed without
allocating role state.

### SSU2 introducer role owner (Plan 490, `src/ssu2_introducer.rs`)

Plan 490's daemon-side owner of the SSU2 **introducer (Bob) role**. The
runtime owns the socket and the bounded tables; the daemon owns the policy
and the receipts.

The role is what the specification calls a NAT-traversal helper: a firewalled
peer (Charlie) asks this router for a relay tag and publishes it, and a
requester (Alice) that can reach this router presents that tag to name the
target. The role owner:

- **issues** the relay tag that identifies the target;
- **admits** an inbound Relay Request: the per-source rate limit comes
  first, then the tag is resolved to the target, then the requester's own
  signing key (resolved from a *validated* RouterInfo through the
  `IntroducerSignerResolver` trait) is used to verify the requester's
  signature over the specification's preimage;
- **answers** with a Relay Intro addressed to the target, and forwards the
  target's Relay Response to the requester once the target's signature
  verifies;
- **refuses** every other path with a typed reason
  (`Ssu2IntroducerRefusal`), counted in the receipt, leaving no state
  behind.

The dispatch order is **service first, transit second, unknown fail-closed**:
the decoded block reaches the service owner through the daemon's live inbound
loop (Plan 498 — see the next section); a Relay Intro belongs to the target
(transit) role and is refused by the service owner; the runtime never surfaces
a block it could not classify.

`Ssu2DaemonHandle::next_relay_dispatched` still exists and keeps its contract,
but it has **no production caller** in the tree. Plan 498 **supersedes** it as
the delivery seam: the live loop reaches the role owner through
`try_next_relay_event`. Do not read `next_relay_dispatched` as the live path.

The receipt carries **counts only** — `requests_admitted`, `intros_produced`,
`responses_forwarded`, `tags_issued`, and refusals by typed reason. It never
carries a peer hash, an address, a tag value, a key, or a payload; the
in-crate test asserts that.

The role is **opt-in and non-advertised**: `ssu2.introducer_service` is
available behind an explicit switch and defaults to off. When it is off the
role does not start and the configuration status says so truthfully
(`SSU2_INTRODUCER_DISABLED_REASON` — "available but not enabled", not the
older and now false "unavailable in this milestone").

### The relay handoff drain in the live inbound loop (Plan 498, `src/ssu2_relay_drain.rs`)

Plan 490 built the role owner and the runtime's relay handoff, and recorded
that the daemon's ordinary inbound pump never drained the handoff: a Relay
Request that a peer really sent was decoded, authenticated, and then dropped.
Plan 498 drains it **on the live path**, and it corrects Plan 490's
prescription on the way.

**The correction is the point.** Plan 490's finding prescribed adding the
drain to `ServiceProduct::poll_inbound`. The daemon's own source says
otherwise (`src/service_product.rs:5039`-5040`): *"advance runs via the daemon
loop; poll_inbound is not on the live path"*. The live inbound pump is the
daemon loop in `src/lib.rs`, whose `tokio::select!` awaits the SSU2 I2NP
handoff and the NTCP2 ingress and arms the `group_tick` and `relay_tick`
cadences. A fix confined to `poll_inbound` would have closed the finding on
paper while a live router still dropped every relay block. **The drain is in
the daemon loop**, in the `relay_tick` arm beside `group_tick`.

**There is exactly one owner.** `NormalSsu2Owner` (`src/lib.rs:2494`-2484`) is
either `Router(Ssu2DaemonHandle)` — no destination groups configured — or
`Groups(Box<ServiceProduct>)`. The live loop drains through whichever holds
the handle: `Ssu2DaemonHandle::try_next_relay_event`
(`src/router_i2np.rs:2225`) or `ServiceProduct::try_next_relay_event`
(`src/service_product.rs:3602``), and `NormalSsu2Owner` is itself the drain's
`RelayEventSource` (`src/lib.rs:2581`-2567`). The product exposes the **pull
only**; it does not own a role owner. Two owners would mean two sets of
counters and an ambiguity about which one is authoritative.

**The drain is bounded and never fails.** One pass pulls at most
`MAX_RELAY_DRAIN_PER_PASS` (16) events — the runtime relay channel's own
capacity (`max_inbound_relay_queue`, default 16, ceiling
`MAX_SSU2_INBOUND_QUEUE_CEILING` = 1024) — and stops at the first empty pull.
A refusal, a resolver miss, and a block that could not be carried are
**counted outcomes, not errors**: `RelayDrainOutcome` carries `drained`,
`admitted`, `refused`, `carried`, and `carriage_refusals` (a
`BTreeMap<&'static str, u64>` keyed by the runtime's stable reason labels),
and carries counts only. The drain never logs a peer hash, a key, a tag, or a
payload.

**The resolver answers only from the validated store.** `NetDbSignerResolver`
(`src/ssu2_introducer.rs:246-265`) implements `IntroducerSignerResolver` over
the shared `Arc<Mutex<Bootstrap>>` and answers through
`Bootstrap::validated_signing_key` (`src/bootstrap.rs:308`-322`), a narrow
read-only accessor over the NetDB's validated `RouterInfoStore`. A peer the
NetDB does not hold yields `None`, and the role owner refuses the block with
`UnknownRequester` rather than guessing; a poisoned bootstrap lock is
fail-closed the same way. The bootstrap's lock is a `std::sync::Mutex` and the
critical section is one map lookup that never awaits.

**The role owner the live loop constructs is the one above.** It is built only
when `ssu2.introducer_service` is on (`src/lib.rs:3965-4000`): the resolver
first, then `Ssu2IntroducerService::new(...)` over the runtime service handle
and the daemon's own router hash. The same key is **default off**
(`src/config.rs:1402-1404`) and is refused at parse time unless
`ssu2.enabled = true` (`src/config.rs:3353-3358`).

**A produced block is carried, not counted (Plan 499).** Plan 498's drain could
only *pull*: it received a relay block the role owner had produced and had no
way to put it on a session, so it counted it as `awaiting_carriage` and
dropped it. Plan 490's **Medium #2** — the runtime's missing outbound relay
queueing API — is now **closed** on the runtime side
(`crates/i2pr-runtime/src/ssu2_runtime.rs:2107`, `:2128`, `:2139`), and the
daemon owns the matching seam.

The drain now carries **two** seams:

- `RelayEventSource` — the **pull** seam, unchanged: where the relay events
  come from (`src/ssu2_relay_drain.rs:57-60`).
- `RelayCarriage` — the seam that puts a **produced** block on the wire
  (`src/ssu2_relay_drain.rs:74-89`). It is told **which peer** the block is
  for, because the block itself carries no destination — the session is the
  destination. It returns a typed refusal. The trait's own doc comment calls
  it a "pull-shaped seam beside `RelayEventSource`": it exists so the drain's
  behaviour can be exercised without binding a socket, and it cannot enqueue
  into the runtime's relay channel or weaken the role owner's verification,
  because it runs only after the role owner admitted and verified the block.

`RuntimeRelayCarriage` (`src/ssu2_relay_drain.rs:95-130`) is the production
implementation. It holds the **same runtime service handle** that owns the
live sessions — the one the role owner already holds — and forwards to the
runtime's four peer-facing carriage methods, which resolve the peer to
**exactly one** live session and queue the block on that session's bounded
control queue. The carriage adds **no** new send path, **no** new socket and
**no** new authority: it is the runtime's own bounded control queue, reached
through the runtime's own API.

The live inbound loop constructs the carriage and hands it to the drain
(`src/lib.rs:4419`-4377`), and the log line now reports `carried` and
`carriage_refused` instead of `awaiting_carriage` (`src/lib.rs:5189`-5011`).

**What the carriage does not do.** It does not publish anything. The
transport's own documentation says the target "asks the introducer for a tag
**and publishes it**" (`crates/i2pr-transport-ssu2/src/relay.rs:789-793`);
**publishing the tag is a NetDB/RouterInfo mechanism, not an SSU2 relay-block
mechanism**, and Plan 499 publishes nothing. The tag is asked for and
answered over the wire; nothing about it reaches a RouterInfo.

**The daemon's anti-amplification check never fires on this path.** The role
owner passes the constant `MAX_INTRODUCER_REQUEST_BYTES` (128,
`src/ssu2_introducer.rs:60`, `:390`) rather than the real request length, so
the transport's `request_bytes * 3 < 128` test
(`crates/i2pr-transport-ssu2/src/relay.rs:864`) is **always** satisfied —
`128 * 3 = 384`. This is **pre-existing** behaviour, not something Plan 499
added or fixed; it is recorded here as a **known limitation**, and the
transport's amplification budget is therefore **not** enforced by the daemon
path.

**The role owner refuses the tag event.** `Ssu2RelayEvent::Tag(_)` reaches
the role owner on the same handoff as an `Intro`, and the owner refuses it as
`NotOurRole` (`src/ssu2_introducer.rs:346-349`) — a tag the runtime observed
is not the role owner's to act on, exactly as an `Intro` is
(`src/ssu2_introducer.rs:345`).

**What the evidence proves, and what it does not.** The drain's *behaviour* is
proved in-process and socket-free by
`crates/i2pr-daemon/tests/ssu2_introducer_handoff.rs`: the **discriminator** (a
relay block reaches the role owner and the receipt moves 0→1), the **control**
(an empty handoff delivers nothing and counts nothing), the **refusal row**
(an unresolvable requester is refused and counted by reason, and no tracked
state is left behind), the **bound row** (one pass stops at the per-pass
bound), and the two **Plan 499 carriage rows** (the drain carries the
produced intro to the tag's target, and a carriage refusal is counted by
reason). The *live wiring* is proved by a **static guard** in the same file:
the daemon's live loop calls the drain, the live owner is the drain's event
source, and the live role owner answers requester keys from the validated
store.

Plan 499 adds the first **live** evidence, in
`crates/i2pr-daemon/tests/ssu2_introducer_live_mesh.rs` — but it is evidence
about **this router against other live routers**, not about a stock peer. The
role owner in that mesh is exercised against a **test signer resolver**
(`MeshSigners`), **not** the production `NetDbSignerResolver`; the production
resolver needs a `Bootstrap` holding validated RouterInfos and has its own
unit rows in `src/ssu2_introducer.rs`. There is still **no** stock-peer or
public-network evidence, and the mesh needed **no external environment**.

The role stays **opt-in and non-advertised**: `ssu2.introducer_service`
defaults to off, `ssu2.advertise` stays refused, `router.version` stays
`0.9.62`, and `specs/support.toml` is unchanged.

### The live three-party UDP mesh (Plan 499, `tests/ssu2_introducer_live_mesh.rs`)

Plan 499's mesh is the first evidence that the introducer role **completes on
the wire**. Three **live routers over real loopback UDP** — Alice (the
requester), Bob (**this router**, acting as the introducer) and Charlie (the
target):

1. **Charlie asks Bob for a tag.** Charlie sends a `RelayTagRequest` on the
   live Bob↔Charlie session. Bob's runtime observes it, checks the
   introducer gate, mints a nonzero tag from the OS CSPRNG, and binds it to
   **Charlie's** router hash — the hash of the session peer, not anything
   Charlie supplied. Bob answers with a `RelayTag` **on the same session**.
2. **Alice presents the tag.** Alice sends a `RelayRequest` carrying the tag.
   Bob's role owner resolves the tag to its owner (Charlie), verifies
   **Alice's** signature against the resolver's key, and produces a Relay
   Intro.
3. **Bob carries the intro to Charlie.** The daemon's drain now *carries* the
   produced block instead of counting it, and the runtime queues it on the
   live Bob↔Charlie session. Charlie receives it, checks the nonce against
   the request it made, and signs a Relay Response.
4. **Bob carries the response to Alice.** Bob verifies the response — the
   **nonce must equal the request's nonce** and the **sender must be the
   target** — and the drain carries it onto the live Bob↔Alice session.
5. **Alice verifies the response** under **Charlie's public key** over the
   transport's exact preimage.

Two further rows pin the boundaries. With the introducer **off**, Bob
observes the request, issues **no** tag, and the requester observes none. And
with **two peers each asking Bob for a tag** (the test names them Charlie and
Dave), each issued tag resolves to **that peer's own** router hash and to
**no other**, and an unknown tag resolves to nothing at all.

**What the mesh does not prove.** It is three instances of this router. It is
**not** a stock-peer lane: a lane against the pinned i2pd needs the Plan 493
topology, which does not exist yet. It exercises the role owner against a
**test** signer resolver, not the production `NetDbSignerResolver`. It
publishes **nothing**: the transport's documentation says the target "asks
the introducer for a tag **and publishes it**"
(`crates/i2pr-transport-ssu2/src/relay.rs:789-793`), and publishing a tag in
the target's RouterInfo is a **NetDB/RouterInfo mechanism**, not an SSU2
relay-block mechanism — Plan 499 delivers the ask and the answer and
publishes **nothing**.

**The protocol facts, stated precisely.** The relay tag names the
**target**, not the requester. The requester's own router hash is **not** in
the request's signature preimage — `relay_request_preimage` binds **Bob's**
and **Charlie's** hashes
(`crates/i2pr-transport-ssu2/src/relay.rs:144-166`), and the requester's
identity comes from the session and only selects **which key verifies** the
signature. (The *Intro* block does carry the requester's hash —
`intro.alice_hash()` is Alice's — but that is Bob's **output**, not a field
Alice signed.) The transport's own parameter is *named* `alice_hash` while
carrying the **tag owner's** hash in this path; the comment above the
binding check (`crates/i2pr-transport-ssu2/src/relay.rs:877`) describes the
parameter's **name**, not its
binding, and is misleading here. See
[i2pr-transport-ssu2](i2pr-transport-ssu2.md) for the full reading.

### API-version floors and peer eligibility (Plan 491)

Plan 491 gives the daemon two **typed refusal** points and the receipts that
record them.

**The tunnel-build enforcement point** (`src/destination_peers.rs`).
`DestinationPeerCandidate::from_validated` now returns
`Result<Self, CandidateRefusal>` instead of an `Option`. The refusal
vocabulary is closed: `UnknownVersion` (the peer published no API version),
`UncomparableVersion` (it published one that cannot be compared),
`BelowVersionFloor` (it published one below the tunnel-build floor), and
`Unqualified` (it lacks the build metadata this profile requires). The
version decision is taken **before** any other fact about the record, so a
peer that cannot be compared is refused without being inspected further.

`CandidateProjectionSummary` counts each refusal by reason.
`select_projected_destination_path` turns an empty projection into the
*reason* it is empty: when the floor refused every scanned record the
terminal is `DestinationSelectionError::NoEligibleVersionPeers`, not a bare
`NoCandidates`. The build fails closed either way — the selector never loops
looking for a peer that is not allowed.

**The DSM enforcement point** (`src/netdb_tunnels.rs`).
`NetDbTunnelCoordinator::begin_publication` refuses a floodfill that does
not satisfy the DSM minimum-version floor, with
`NetDbTunnelError::FloodfillRefusedByVersionFloor(reason)`; the reason is
one of the stable labels `unknown-version`, `uncomparable-version`, or
`below-floor`. The refusal is counted in
`NetDbTunnelCounters::publications_refused_by_version_floor`, and the
publication *target* selection (`nearest_eligible_floodfills`) excludes the
same peers and reports the same counts.

**The receipts** (`src/netdb_participation.rs`).
`VersionEligibilityReceipt` is a separate, bounded log from the Plan 488
participation receipts, because it describes a *decision* rather than the
store's observed state. It carries the gate, the examined population, the
accepted count, the refusal counts by reason, and a digest over those
counts — and **no peer identity of any kind**. The two driver methods
`select_tunnel_build_peers` and `select_dsm_targets` make the decisions and
record them, so a refusal is never a silent skip.

The driver is still **not** wired into the router's tunnel-building loop;
Plan 488's gap is unchanged. The live product path reaches the same floors
through the two enforcement points above, which refuse at the point of use.

### `router_i2np.rs` — the controlled `ssu2-router` service

`Ssu2DaemonService` runs under `SSU2_SERVICE_NAME = "ssu2-router"` supervision
(`src/lib.rs:1506``) and is the **only** inbound entry point for authenticated router I2NP.
`dispatch_router_i2np(inbound: &i2pr_runtime::Ssu2InboundI2np, now_ms: u64)`
(`src/router_i2np.rs:354`) is the single dispatch seam; it takes its input from the runtime
SSU2 session owner, so an inbound body only reaches it after the runtime has authenticated
the peer session. The dispatcher returns one of exactly four `RouterI2npOutcome` variants —
`TunnelBuildReserved { .. }`, `PeerTest { .. }`, `TunnelData { .. }`,
`RouterControl { .. }` — or `Unsupported { .. }` for a body kind it will not touch, and
fails closed on `RouterI2npError::Empty` / `TooLarge` before any decode
(`src/router_i2np.rs:471`, `474`). `dispatch_router_i2np_with_transit_bodies` (387) is the
same seam plus the bounded `TransitInboundBodies` extraction.
`controlled_transit_disabled_probe` is the no-owner fallback the composition's
`dispatch_transit_inbound` uses when the operator did not opt in; the
installed owner is consulted first, and the two can never run beside each
other (Plan 502). `RouterDeliveryService`
is the narrow outbound seam over the existing `send_i2np` path.

The service **is** registered in the production graph: `src/lib.rs:3905` constructs
`Ssu2DaemonService::new_with_bandwidth(&ssu2_config, identity, bandwidth)` when `[ssu2] enabled = true`, and the whole
block fails closed (`ServiceResult::Failed` with `InvalidState`) when the SSU2 service has no
loopback bind. Activation is therefore opt-in through configuration, **default off**
(`default_ssu2_enabled() == false`), and publication stays pq-free and non-advertised.
Java `pq=4,3` remains parser-tolerance only.

### M11 transit (Plan 268) — do not over-claim

Plan 268 passed a **ONE-FAMILY experimental qualification** with exact-head CI
(`plans/closure/transit-tunnels/268-status.md`:
`passed-m11-receipt-family-three-completions-zero-semantic-failures-path-divergence-falsified-exact-head-ci-green`).
**Public transit participation remains DISABLED, NON-ADVERTISED, and UNCLAIMED** (ADR
[`0026`](../../docs/adr/0026-staged-interoperability-progression-and-java-debt.md)).
`TransitParticipation::Disabled` is the enforced product posture and owns no counters;
`TransitParticipation::Enabled(Arc<Mutex<TransitVolumeCounters>>)` exists only for the
controlled qualification lane. The honest product baseline for the three Proposal 170
transit selectors is `0` / `0` / `0.0`. Plan 340 closed ownership of
`total.transit.bytes`, `bw.transit.15s`, and `tunnels.shareratio` as a
participation-posture change, not as missing snapshots.

### Exploratory build posture (Plan 505, `src/config.rs`)

Plan 505 added the **operator-opted posture** that says whether this router
builds its own exploratory tunnels. It is one configuration section and one
switch:

```text
[exploratory]
builds = false                 # the single switch; OFF by default
```

`config.rs` owns the surface (`ExploratoryConfig`, `RawExploratoryConfig` with
`deny_unknown_fields`). The section is strict, so a key that looks like an
advertisement — `advertise = true` — is **refused** at parse time rather than
ignored.

**There is deliberately no count knob.** The number of exploratory pairs the
router maintains is bounded by the coordinator's own pool and by Plan 504's
`MAX_LIVENESS_PAIRS`. A configuration key that could disagree with those
ceilings would be a second source of truth rather than a limit, so a
`max_pairs` key is refused by the parser and a test asserts that refusal.

**Off by default for a concrete reason, not out of caution.** The daemon's
transport can only dial a peer it may legally reach: `Ssu2DialTarget::new` is
loopback-only and `Ssu2DialTarget::new_private_lan` accepts only RFC1918 IPv4.
A public deployment's peer population is therefore **not dialable at all**, and
a default-on driver would be a router that tries and fails. Enabling the
posture is an explicit operator decision that this router should build
exploratory tunnels inside the mesh it can actually reach.

When the posture is off, the composition builds **no** driver at all, the
product owns none, and the tick's exploratory step costs nothing — a router that
has never set the key behaves exactly as it does today.

### The peer-knowledge reconcile (Plan 506, `src/peer_reconcile.rs`)

Plan 505 closed the exploratory build driver and left the **population** it
draws from open. Plan 506 closes it.

The router accepts a peer's RouterInfo on a **live** session — three
production sites ingest an authenticated unsolicited `DatabaseStore` into the
bootstrap NetDB — but the two stores the router actually **builds with** were
seeded **once at startup** and never again. `insert_validated_router_info` had
exactly one production caller, the startup seed at
`crates/i2pr-daemon/src/service_product.rs`, and `learn_bootstrap_peers` had
exactly one production caller, also at startup. A peer learned after startup
was invisible to both, so the population never grew.

**The framing is narrower than it looks, and Plan 506 corrected it.** The
router does **not** learn peers from a NetDB lookup: a `DatabaseStore` carries
exactly one record, a `DatabaseSearchReply` carries peer **hashes only**, and
the production router runs **no** RouterInfo-kind lookup — the only
`begin_lookup` caller is Plan 433's never-constructed `NetDbTunnelCoordinator`.
The live-learned RouterInfos are exactly the unsolicited `DatabaseStore`s, so
this surface is the path that carries them into the build-with stores. Adding a
RouterInfo-kind lookup would be a protocol-composition change with its own
plan-of-record.

**One bounded operation, owned by the product.** `PeerReconcile::run` is a
**step**, not a task: it holds no timer, owns no cancellation, and is driven
by the product's existing tick beside Plan 505's exploratory driver.

1. **Snapshot** the bootstrap's validated RouterInfos under the bootstrap's
   lock, and **release** that lock.
2. **Insert** each record into the destination store through the canonical
   `insert_validated_router_info`.
3. **Learn** the same records into the peer-profile store through the
   canonical `learn_all`.

One snapshot, one cadence, no skew between the two stores.

**The three locks are taken in one order and are never nested.** The
bootstrap's lock is released before the destination coordinator's lock is
taken, and the destination coordinator's lock is released before the
peer-profile store's lock is taken. The peer-profile closure never awaits, so
no lock is held across an await. `crates/i2pr-daemon/tests/peer_reconcile_boundary.rs`
asserts this **structurally**: the source order of the three steps is the
release order, and each step's body may not name another step's lock.

**Every outcome is counted by reason, and a refusal is never fatal.** A
capacity refusal, a conflict and a stale replacement are counted and the batch
continues — deliberately unlike the startup seed, which treats a capacity
refusal as fatal. One bad record never aborts the reconcile.

**The reconcile writes peer records, never a selection.** Plan 491's version
gate stays at **selection**, so a refreshed peer below the floor is still
refused there and counted. The reconcile feeds the gate; it does not weaken it.

**The cadence is the existing NetDB participation maintenance period.** The
composition root copies `config.netdb.participation_maintenance_period_ms` out
of the configuration and hands it to the product. There is **no second
configuration knob**: `config.rs` is untouched and no new key is added. The
reconcile's own bound is the same range the configuration parser already
enforces, so the reconcile's refusal cannot fire on a configured router.

**A real defect was fixed, not just recorded.** The unsolicited store was
ingested **twice** for every message the router did not serve as a floodfill:
the second site's condition — a `DatabaseStore` body with a bootstrap store —
was a strict **subset** of the first site's, so the second write re-validated
and re-applied the record and produced `InsertOutcome::Idempotent`. The two
remaining ingest sites sit in **mutually exclusive** branches — the
`!ssu2_config.enabled` branch and the SSU2-enabled path — so exactly one can
run for any message. The static guard asserts that structurally: one ingest
inside the branch, one outside it, and no more.

**The counters are published through the inspection seam.** The product
refreshes a counter cell on its tick and the composition root publishes the
same `Arc`. The counters carry counts and stable reason labels only — no peer,
address, key or payload — and an unpublished or poisoned slot is a **gap**,
never a fabricated zero.

**The feasibility artefact is not vacuous, and that is measured.** It drives
the **real** `Bootstrap`, the **real** `DestinationTunnelCoordinator` and the
**real** `PeerParticipationDriver`, and the peer arrives through the production
unsolicited-store path. Removing the `learn_all` call fails four tests, the
first with "the peer must be selectable after the reconcile"; removing the
`insert_validated_router_info` call fails five, the first with "the peer must
be insertable into the destination store by the reconcile"; raising the
below-floor peer's published version to the floor fails the refusal assertion.

**The selection set is recorded as a finding.** The destination projection is
a hash-ordered **256-prefix**
(`crates/i2pr-daemon/src/destination_peers.rs`), so adding peers does not
merely add candidates — it can change **which** peers the build path sees.
Changing the ordering is out of scope; recording the behaviour is not.

**What Plan 506 does not close.** A RouterInfo-kind NetDB lookup — the router
performs none in production. Plan 497 limitation 5's **read** half: the
routing table's only reader remains Plan 433's never-constructed
`NetDbTunnelCoordinator`. The destination store's **capacity policy** — it
refuses at capacity, and this plan adds no eviction. Plan 487 F3's
destination-inbound half still needs tunnel rekeying, which the tree does not
implement at all. Plan 491 F2 and F4, Plan 490 Medium #3, Plan 502 M1 and
Plan 503's lookup-target API-version gate all stay open. **No stock-peer
evidence is claimed**: whether an ordinary public router sends unsolicited
stores this router will accept is **unproven**.

### The lookup path's API-version posture (Plan 509, `src/destination_tunnels.rs`)

**The decision: the lookup target is NOT version-gated.** The router asks
the floodfills its own routing policy selects, and every answer is validated
by the router before it is trusted.

The decision is a **product decision**, and Plan 503 recorded it as an open
finding because it changes which floodfills the router can ask at all. The
reasons it is the right decision:

- **No floor is derivable.** The specification's API-version table attaches
  exactly two duties to the `0.9.62` row — *minimum peers will build tunnels
  through* and *minimum floodfill peers will send DSM to*. A `DatabaseLookup`
  is neither. `i2pr_proto::MinimumPeerObligation` carries only
  `constrains_tunnel_peers` and `constrains_dsm_targets`, and
  `i2pr_proto::VersionFloors` carries only `min_peer_for_tunnel_build` and
  `min_floodfill_for_dsm`. A lookup floor would have to be **invented**, and
  a floor is derived, never hand-maintained.
- **The gate vocabulary already says so.** `i2pr_netdb::VersionGate` has
  exactly two variants — `TunnelBuildPeer` and `DsmTarget` — because the
  specification imposes exactly two duties. A lookup gate cannot be expressed
  in the tree's own vocabulary without a third duty that does not exist.
- **The two floors encode peer-side duties that do not transfer.** Each floor
  says a peer **will not** do something, so the router's refusal is exactly
  *do not ask a peer for something it will not do*. The specification imposes
  no such duty for a lookup.
- **The responder's version buys no safety.** The answer is a `DatabaseStore`
  the router validates itself (`handle_database_store_lease_set2` runs it
  through `ValidatedLeaseSet2::from_lease_set2`), so a below-floor peer that
  answers correctly is answered to and one that answers incorrectly is
  refused by the validation.
- **The rotation absorbs an unhelpful candidate.** The state machine excludes
  already-queried peers and re-selects from the store, so a silent candidate
  costs one round trip and the lookup moves on.
- **The two references disagree.** i2pd refuses to *file* a floodfill below
  `0.9.62`; Java's `0.9.62` is a peer-exclusion floor and its floodfill
  predicate has no version floor at all. Neither gates a lookup, so the
  router does not pick a side.

**The selection sites.** The lookup path's selections are the **ungated**
nearest-floodfill selection: `src/destination_tunnels.rs`'s
`begin_lease_lookup_inner`, and the state machine's `start`,
`start_with_preferred` and `advance_active` in `i2pr-netdb`'s
`lookup_engine.rs`. The version-gated selection appears only on the two DSM
paths, which Plan 503 put there.

**The guard.** `crates/i2pr-daemon/tests/lookup_target_version_boundary.rs`
makes the decision structural, and
`crates/i2pr-netdb/tests/lookup_target_version_decision.rs` proves it.

### The comparator's component ceiling (Plan 511)

The I2PControl and NetDb eligibility gates refuse a peer whose published
`router.version` cannot be compared. The comparator's component bound is
**derived** from the specification's API-version table's widest row, which
`i2pr-proto` holds as data (`SPECIFICATION_WIDEST_VERSION_ROW`), so a wider
row in the specification's table widens the comparator by construction rather
than leaving a version uncomparable behind a literal. The comparator's
ordering stays **exact** — a version it accepts is stored in full — and the
refusal stays **typed** (`UncomparableVersion`), **counted** by reason
(`uncomparable-version`), and **fail-closed**: it can only exclude a peer
that both pinned references would admit, never admit one below a floor. The
references' own component handling, derived from their pinned source, is in
`specs/QUALIFICATION-GATES.md` §10.

### The managed-tunnel staging path's reason classification (Plan 510, `src/i2pcontrol_tunnels.rs`)

Plan 491's F4 was an intermittent managed-tunnel staging failure that
never reproduced again, and it was undiagnosable: `static_manager_reason`
collapsed **every** `ServiceTunnelError::InvalidConfig` into the single
label *"manager configuration rejected"*. Plan 510 makes the rejection
name its class and puts the path under a load-sensitive lane.

**The classification.** The daemon's `InvalidConfig` detail is mapped to a
**value-free class** through `MANAGER_CONFIG_CLASSES`, a pinned list of
forty (marker, class) pairs.

- The **marker** is a stable phrase that appears *inside* the detail. The
  service id is embedded at the front of most messages — *"canonical
  use_ssl requires a daemon TLS policy"* — so a prefix match would have to
  know the id, and a class that depends on the id is not a class. The
  marker carries no value: it is a constant phrase in the daemon's own
  error construction.
- The **class** is a short label of letters and spaces: *"server TLS policy
  missing"*, *"candidate bind collision"*, *"destination registry
  poisoned"*. It never names an id, a socket, a port, a key or a payload.
- The order matters: a longer marker that *contains* a shorter one comes
  first, so the more specific class wins. The guard asserts that no marker
  is shadowed by an earlier one.
- The mapping is **total**. A detail that matches no marker keeps
  `MANAGER_CONFIG_GENERIC` — today's label, unchanged — so a new rejection
  can never make the operator's view worse, and a new marker can be added
  deliberately with its own guard entry.
- **No value can reach the wire.** `manager_config_class` and
  `static_manager_reason` both return `&'static str`, so the type system
  forbids a detail from escaping. The reason template is fixed:
  `runtime transition failed: {reason}` — a class plus a prefix, nothing
  else.

**The load lane.** `crates/i2pr-daemon/tests/i2pcontrol_tunnels_load.rs`
drives eight **concurrent** managed-tunnel creates against one real daemon,
each with its own name, loopback port and target destination, and each
generating a destination identity, building a lease set and binding a
listener. The load is real work, never a sleep, and every wait is an
explicit bounded deadline. The same lane exercises the **reject** path — a
server kind with `UseSSL` and no daemon TLS policy, refused while the
shared manager stages the runtime — and asserts that the reason names a
class and carries no value, and that the shared manager still stages a
runtime afterwards.

**The guard.** `crates/i2pr-daemon/tests/managed_tunnel_reason_boundary.rs`
pins the table, the classification's totality, the agreement between the
pinned table and the production function, the value-freeness of every
class, the liveness of every marker in `src/service_tunnels.rs`, the
`&'static str` signatures, the fixed reason template, and the lane's
boundedness, concurrency and freedom from sleeps. Every rule carries an
inverted probe.

### The transit creator-correlation retention window (Plan 508, `src/transit_owner.rs`)

Plan 502 closed the **population** the transit owner must bypass; Plan 508
closes its **shelf life**.

**The gap.** The transit owner's `creator_builds`, `creator_data` and
`creator_gateways` facts were `BTreeSet`s with only `insert`, and the
production wiring installs them on **every** pump tick, so each build the
router performs left one entry behind for ever. The code's own comment
said so: *the number of facts is bounded by the coordinator's
pending-build and tunnel-retention ceilings times the number of builds
the router performs* — a bound **per build**, not over time. Plan 502
recorded it as Finding M1 and deferred the fix as *a protocol decision*.

**The decision, made on the record.** A creator-correlated fact is
installed while the coordinator's build is **pending**; the coordinator's
own memory of that build's identifiers starts when the build is
**retired**. The window is therefore the build deadline plus the
coordinator's own longest identifier retention —
`BUILD_DEADLINE_MS + BUILD_TUNNEL_ID_RETENTION_MS`, 46 minutes — derived
through a constant from bounds the tree already enforces. There is **one**
source of truth and **no new configuration key**.

**The expiry is checked at use time.** That is the **safe** direction: an
expired fact is declined, and a live one never is, so the router's own
traffic is never claimed as transit work — the invariant the facts exist
for. The **sweep** rides the pump tick that already installs the facts,
so the owner gains no second timer and no second task.

**Every comparison rounds in the safe direction.** Two of the four use
sites (`dispatch_router_garlic` and `dispatch_build_body`) carry seconds
and two (`handle_tunnel_data_inner` and `handle_gateway_inner`) carry
milliseconds. The window is stored in seconds with a **ceiling** at
install time and a **floor** at comparison time, so a fact is never
declined before it has expired.

**The expiry is owned by one wrapper.** The three fact sets are held by
a `CreatorFacts<K>` wrapper whose membership test is **clock-aware**, so a
use site cannot reach a fact without reaching its expiry: the comparison
lives in exactly one place, and the use sites call `contains` or
`contains_ms` rather than re-implementing the test. The wrapper also owns
the millisecond-to-second floor, so the two use sites that hold
milliseconds cannot round in the unsafe direction by accident.

**The wrapper is spelled `contains` for a reason.** Plan 492's
dispatch-order guard anchors on the literal `creator_builds.contains`
inside `fn dispatch_build_body`, so that guard and the shape of the
creator-correlation check are coupled in the source. The first Plan 508
implementation replaced the check with a raw map lookup and broke that
anchor; the wrapper keeps the literal, so **Plan 492's guard is left
byte-identical** and stays green. Plan 508's own guard asserts the same
anchor (rule 11), so a change that would break Plan 492's guard is
reported by Plan 508's guard first, with the reason named.

**The coordinator's own fact set is untouched.** It was already bounded
and already expired; only the owner's **copy** was unbounded.

**The controls are not vacuous, and that is measured.** Four mutations of
the real tree, each applied, run against the real lanes, and reverted: a
use site that stops consulting the expiry, a sweep that compares against
nothing, a window that collapses to zero, and an install that drops the
expiry. They fail six, eleven, eleven and eight tests respectively, and
the tree was restored byte-for-byte after each. Two of them were found
to be weaker than they looked and were fixed — see the closure record.

**The closing floor found a real regression, and it was fixed in the
design rather than in another plan's guard.** The first implementation
replaced the creator-correlation check with a raw map lookup, which
removed the literal Plan 492's dispatch-order guard anchors on, and the
floor failed on Plan 492's guard. Two fixes were available: rewrite
Plan 492's anchor, or make the replacement keep the anchor. The second
was chosen, because another plan's guard is not this plan's to edit.

**A documentation drift is recorded, not fixed.** The module table's
per-file line counts above are stale for **33** of the 63 rows that carry
one. Plan 508 measured 36, corrected the three it changed, and left the
rest alone.

### The NetDB retention policy (Plan 507, `src/peer_reconcile.rs`)

Plan 506 closed the **population** the router learns. Plan 507 closes the
**shelf life** of what it learned.

**The gap.** `RouterInfoStore::prune(now, max_age_ms)` -- the tree's own
explicit prune, the one Plan 103 §3.4 preferred over silent eviction -- had
**exactly one caller**: its own unit test at
`crates/i2pr-netdb/src/store.rs`. No production path pruned. The store
**refuses** at its quota and **never evicts**, so a NetDB that filled up
refused every new peer, permanently, and kept records the router's own
validator would refuse to accept today: the router refused a RouterInfo
older than `DEFAULT_MAX_AGE` (24 h) at validation and then retained one
indefinitely. The acceptance rule and the retention rule disagreed.

**The retention window is the router's own acceptance window.**
`peer_reconcile::RETENTION_WINDOW_MS` is derived from
`i2pr_netdb::ValidationPolicy::default_const().max_age` through a `const
fn`, so the router retains exactly what it accepts and the constant cannot
drift from the policy without a compile error. A second value would be a
second source of truth, which is the defect this plan removes. There is
**no new configuration key**: `config.rs` is untouched, and the static guard
refuses a `retention` or `max_age` key there.

**The prune rides the reconcile's existing cadence.** It is folded into
Plan 506's `PeerReconcile::run`, so the prune and the reconcile can never
drift apart in time. The router gains no second knob, no second task and no
second timer.

**The order is prune the bootstrap, prune the destination store, snapshot,
insert, learn.** The prune runs **before** the reconcile's snapshot, so a
record the prune removed is never carried into the destination store. The
prune calls the tree's own `RouterInfoStore::prune` through a bounded
method on `Bootstrap` and on `DestinationTunnelCoordinator`;
**`i2pr-netdb` is not changed** -- the plan adds a *caller*, not a
mechanism.

**The locks are one at a time and never nested.** The bootstrap's lock is
taken and released, then the destination coordinator's lock is tried and
released, and the step takes **no await at all**, so no lock is ever held
across an await. The destination store's lock is **tried, never waited
for**: a store another task is using is left alone for that pass, the
condition is **counted** under the stable reason label `prune-unavailable`,
and the reconcile still runs. The store turns over on the next pass.

**Every prune is counted.** `pruned-bootstrap`, `pruned-destination` and
`prune-unavailable` carry counts and stable reason labels only -- no peer,
address, key or payload -- and they ride the **same** inspection seam Plan
506 established, so an unpublished or poisoned slot stays a **gap**, never a
fabricated zero.

**Three things the prune deliberately does not do.** It does not prune the
**peer-profile store**, which has its own eviction policy and self-corrects
through the observation tiers. It does not reach the router's **own
RouterInfo**, which lives beside the store rather than inside it. And it
does not **select**: it removes peer *records*, so Plan 491's version gate
stays at selection.

**The controls are not vacuous, and that is measured.** Four mutations of
the real tree, each applied, run against the real lanes, and reverted:
removing the prune call from `fn run` fails nine tests; moving the prune
call after the reconcile's snapshot fails seven; collapsing the retention
window to zero fails seven; and dropping the destination-store prune count
fails five. The tree was restored byte-for-byte after each control. The
WP1 feasibility artefact carries two more controls of its own, recorded in
its module comment.

**The consequence of the policy is recorded, not designed away.** Pruning a
stale floodfill can leave the router with no lookup target. Protecting
floodfills would mean retaining records the router would refuse to accept
-- the exact inconsistency this plan removes.

**What Plan 507 does not close.** The destination store's **quota values**
are unchanged (4 096 records and 4 MiB by default); the plan changes what
happens *at* the quota, not the quota. The **lookup-target API-version
gate** (Plan 503) stays open: the specification's table constrains neither a
lookup target nor a DSM target beyond the two gates already in place, so
the floor there is a product decision and not a derived one. Plan 491 F2 and
F4, Plan 490 Medium #3, Plan 502 M1 and Plan 487 F3's destination-inbound
half stay open. **No stock-peer evidence is claimed.**

**A documentation drift is recorded, not fixed.** The module table's
per-file line counts above are stale for **36** of the 63 rows that carry
one. Plan 507 measured 40 stale rows, corrected the four it changed, and
left the rest alone: the drift is a separate concern with its own
plan-of-record, and correcting thirty-six unrelated rows here would bury
the change.

### Transit participation posture (Plan 492, `src/transit_participation.rs`)

Plan 492 added the **operator-opted posture** that the M11 transit machinery
had no answer for. The transit state machine and the transit ingress owner
were already closed surfaces; what was missing was a policy that says *whether*
this router relays other routers' tunnel traffic, *how much*, and *for how
long*.

The posture is one configuration section and one owner:

```text
[transit]
participation = false          # the single switch; OFF by default
share_limit = 25               # percent of the enforced share, 1..=100
participating_tunnels = 4      # 1..=MAX_TRANSIT_PARTICIPATING_TUNNELS
peer_participation_cap = 2      # 1..=MAX_TRANSIT_PEER_PARTICIPATION_CAP
```

`config.rs` owns the surface (`TransitConfig`, `RawTransitConfig` with
`deny_unknown_fields`) and the named bounds
(`MAX_TRANSIT_SHARE_LIMIT_PERCENT`, `MAX_TRANSIT_PARTICIPATING_TUNNELS`,
`MAX_TRANSIT_PEER_PARTICIPATION_CAP`). `normalize_transit` rejects an
out-of-range limit, a per-peer cap larger than the router-wide count, and an
opt-in **without an enabled bandwidth policy** — the share limit is enforced
against the governor's permitted share, so without a governor it would be
advisory, and an advisory limit is a configuration error rather than a silent
no-op.

`transit_participation.rs` owns the policy:

- `TransitParticipationPolicy` — the bounded limits, validated by one
  constructor that the configuration path *and* the hand-built path both go
  through.
- `TransitParticipationOwner` — the accounting surface. It owns no socket, no
  timer, and no task; the runtime owner supplies the clock.
- `TransitParticipationBinding` — the owner plus the live governor, so the
  permit stays healthy while the owner is installed and the owner has the
  governor to revalidate against.

**The counts are the data plane's own.** `admit` takes the transit service's
authoritative registration counts (`active_count`, `active_for_peer`), so the
owner never keeps a second, diverging ledger of live tunnels: an expired, a
closed, and a rolled-back registration are reflected immediately. The
participating-tunnel limit and the per-peer cap are measured against the
registry, not against a counter that could drift.

**The share is enforced, not advisory.** The owner holds the governor's
`BandwidthEnforcementPermit` and revalidates it at the point of use
(`revalidate`): a permit issued by a different governor instance is *stale*
(`PermitWithdrawn`), and a dropped issuing governor is a *health loss*
(`HealthLost`). The relay is charged against the permitted share **before**
the cell leaves the router, and exceeding the share refuses the relay rather
than growing it. Without an enforced permit there is no share, so every
request is refused with `NoEnforcedPermit`.

**The dispatch order is unchanged.** `TransitLiveOwner::dispatch_build_body`
still reads *creator/service first, transit second, unknown fail-closed*, and
the posture is consulted **after** the creator correlation check and **before**
the transit dispatch — so a refused request never allocates a registration.
`handle_tunnel_data_inner` charges the share before the forward. Both
orderings are static-guarded by
`crates/i2pr-daemon/tests/transit_participation_boundary.rs`, which carries an
inverted probe for each rule.

**The receipts are bounded and carry counts only.** A receipt holds the event
kind, an optional typed refusal reason, the participating-tunnel count, the
admitted-peer count, the relayed bytes, the refusal counters, and the stop
count. It never holds a peer hash, an address, a tunnel id, a key, or a
payload — and the guard proves that by inspecting the receipt *type*, not
merely its values.

**The ordinary build-hop selection now satisfies Plan 488's diversity rules.**
`destination_peers.rs` gained `HopDiversityPolicy` (the /16 rule and the
per-peer participation cap, taken from `i2pr-netdb` rather than restated),
the candidate carries its `/16` prefix, and the bounded search honours
`max_per_prefix16` without ever relaxing the rule to reach the hop count.
`DestinationTunnelCoordinator` owns the bounded per-peer participation ledger
and `record_hop_selection` records every selection, so the cap is measured
against real selections rather than being advisory.

> **Deviation from the plan document (recorded).** The plan named
> `i2pr-client` for the selection diversity. The router's own build-hop
> selection is the **daemon's** `destination_peers.rs`; `i2pr-client` owns
> destination tunnel *lifecycles* and consumes peers that were already chosen
> there (Plan 491 established the same fact for the version floors). The rule
> therefore landed in the daemon, consuming Plan 488's policy from
> `i2pr-netdb`, and `i2pr-client` is unchanged.

### M12 floodfill (Plans 270–283, 302–303, 306)

`floodfill.rs` retains coordinator resource leases through effect completion, constructs
fresh bounded I2NP envelopes, and routes Store acknowledgements and direct/tunnel lookup
replies using the supplied reply route. Tunnel replies are exactly one Garlic body nested
in one `TunnelGateway`. Direct replication sends a zero-token `DatabaseStore` only to its
selected peer. `run_floodfill_owner` supplies one bounded ingress, maintenance, and
effect-drain future with per-effect outcome accounting, a bounded cancel-drain, and a stats
return. `activate_controlled` / `withdraw_controlled` are the only composition sites the M12
guard permits: explicit-bind recording -> publication material -> eligibility -> activation
-> `caps=f` install -> publish, with failure rollback to the previous non-`f` bytes and
health-loss withdrawal (Draining -> same-address non-`f` reinstall -> bounded drain ->
Disabled). The production SSU2 service graph does not start `run_floodfill_owner`.
**Daemon role lifecycle, qualification, and all floodfill advertisement remain
unimplemented/unclaimed** ([ADR
`0027`](../../docs/adr/0027-floodfill-role-provenance-and-advertisement.md)).

> **Boundary-checker note (resolved by Plan 364).** `scripts/check-m12-floodfill-boundaries.sh`
> previously **exited 1 on repo head.** It grepped `crates/i2pr-netdb` for
> `DatabaseStoreData::EncryptedLeaseSet|ValidatedEncryptedLeaseSet|ServerEncryptedLeaseSet`
> — the Plan 281 type-5 deferral guard — and that pattern now legitimately matches, because
> Plans 332/333/334 populated the type-5 Encrypted LeaseSet2 floor in NetDB storage
> (`i2pr-netdb/src/els2.rs`, `lookup_engine.rs:523`, `floodfill_service.rs:739`,
> `server_store.rs:31`, `store_message.rs:84`). It sat in **neither** `AGENTS.md`'s
> routine floor **nor** `.github/workflows/ci.yml`, so the failure was invisible to CI.
> **Plan 364** replaced the stale rule with 9 positive assertions traced to the
> Plans 332/333/334/346 closure records (`--self-test`: 11/11 deliberate breaks
> caught). The script now exits 0 and is in both the floor and `ci.yml`. The script
> was never weakened and type-5 was never removed from NetDB.

### SAM and I2CP listeners

Both are loopback listeners with supervised admission and explicit per-connection ceilings
(`sam.max_clients = 16`, `sam.max_sessions = 16`, `sam.max_stream_sockets_per_session = 16`,
`sam.max_pending_accepts_per_session = 16`; `MAX_I2CP_CLIENTS = 256`,
`MAX_I2CP_SESSIONS_PER_CONNECTION = 16`, `MAX_I2CP_SESSIONS_ROUTER = 256`,
`MAX_I2CP_BUFFERED_BYTES_PER_CONNECTION = 1 MiB`,
`MAX_I2CP_PENDING_WRITES_PER_CONNECTION = 4_096`). Both reuse the **shared**
`destination_streaming` byte pump; there is no second pump.

**I2CP pre-session reject path ordering (verified).** `handle_connection`
(`src/i2cp.rs:1109`) performs the terminal close in this exact order:

```rust
let _ = stream.shutdown().await;      // src/i2cp.rs:1136
state.teardown_connection(connection_id); // src/i2cp.rs:1137
state.drop_connection(connection_id);     // src/i2cp.rs:1138
```

`stream.shutdown()` is awaited **before** any bookkeeping, so every terminal pre-session
rejection terminates TCP deterministically instead of relying on `TcpStream` drop timing. A
shutdown failure never blocks cleanup (`let _ =`), and no frame is written for an invalid
first byte. The `wrong_protocol_byte_is_closed` row is **strict** — a timeout is a failure,
not a pass — and it has a non-paused `wrong_protocol_byte_is_closed_real_time` companion with
24-iteration baselines. Never revert to drop-timing.

### I2PControl listener stack

`i2pcontrol.rs` owns the TLS + JSON-RPC listener with an in-memory token table, a
source-IP throttle (`THROTTLE_CAPACITY = 1_024`, `THROTTLE_WINDOW_MS = 60_000`,
`THROTTLE_FREE_FAILURES = 4`, `THROTTLE_DELAY_STEP_MS = 50`, `THROTTLE_DELAY_MAX_MS = 2_000`),
`MAX_HTTP_HEAD_BYTES = 16_384`, `MAX_HEADER_TOKEN_BYTES = 512`, and
`INFLIGHT_REQUESTS = i2pr_i2pcontrol::MAX_INFLIGHT_REQUESTS`. `i2pcontrol_tunnels.rs` is
the TunnelManager control state; `i2pcontrol_inspection.rs` holds the narrow
publish-gated inspection handles and the pure response builders.

**Plan 341 restart-safe outbound proxy secret owner** (`src/outbound_secret.rs`, the most
recent daemon commit `11842388`): `OutboundSecretKey(Zeroizing<[u8; OUTBOUND_SECRET_KEY_LEN]>)`
— **deliberately not `Clone` and not `Debug`-derived** — holds an HKDF-SHA256-derived key
from the router signing seed, labelled by the constant
`OUTBOUND_SECRET_KEY_INFO = b"i2pr:outproxy:secret-box:v1"`. The proxy credential is sealed
with ChaCha20-Poly1305 under a fresh random nonce with an associated-data frame; the
derived key is separable from the signing key, so compromising one does not yield the other.
`RouterBoundOutboundSecrets` re-derives the key on restart, which is what makes the owner
restart-safe without persisting the key. The frame ceiling is
`OUTBOUND_SECRET_NONCE_LEN + MAX_OUTBOUND_SECRET_LEN + OUTBOUND_SECRET_TAG_LEN`, and the
success type is proven not to be `Debug` by an in-crate test.

`ServiceEls2Material` (`src/service_els2.rs:124`) is likewise **not `Clone`** — two copies
of a service's scalar is exactly what must not exist — and its hand-written
`impl fmt::Debug` prints `<redacted>` for the authorization plus presence/counts only. The
lookup secret is borrowed from the manager's record, never copied.

### Router console (Plans 356–358)

`console.rs` owns the loopback listener and nothing else. Three decisions in it
are load-bearing:

1. **The router is built after `bind`, not before.** The authority policy is an
   allow-list of exact `Host` values (`localhost:<port>`, `127.0.0.1:<port>`,
   `[::1]:<port>`), and the tests bind `port = 0`. Building the `AxumRouter`
   before the socket exists would freeze the allow-list against a port nobody
   has yet, so `ConsoleServiceState` binds first and only then calls
   `security_policy()` with the resolved port. A request whose `Host` does not
   match is a `403`, including the bare `localhost` with no port.
2. **The listener is bound under a bounded timeout and then served to
   cancellation — it is not wrapped end-to-end in one timeout.** This differs
   deliberately from the SAM pattern. A healthy long-lived listener that is
   cancelled by the supervisor looks identical, under a whole-lifetime
   timeout, to one that failed to start; binding is bounded so a failure is
   reported promptly, and serving is then unbounded by design.
3. **The console reaches router state through `ControlDispatcher`, never
   through the network.** Plan 358 moved `process_router_info` and
   `process_client_services` out of `I2pControlServiceState` into a
   transport-free `ControlDispatcher`; the listener state now delegates to it.
   `ConsoleControlClient` holds one `Arc<ControlDispatcher>` and answers the
   console's `ControlClient` trait from it. The consequence is that enabling the
   console does **not** require an I2PControl listener, a control password, or a
   token — and the console cannot accidentally acquire one.

`LocalConsolePrincipal` is the whole authorization story: a closed allow-set of
`RouterInfo` and `ClientServices`. A request naming `TunnelManager`,
`AddressBook`, or anything else is refused before the dispatcher is consulted.
`i2pcontrol_dispatch.rs` carries a test proving the console principal's answers
for those two methods are byte-identical to the external-wire answers for the
same input, so the in-process path is not a second, drifting implementation.

There is deliberately **no** `Server` response header. EggServe's default was
empty rather than absent, and the project's posture is to advertise nothing
beyond the tested subset, so the header is dropped entirely rather than
populated.

### Address book, control sources, and NEWS

`addressbook.rs` owns the manager, `[addressbook]` config, transactional mutations with
rollback, generation persistence, the bounded refresh queue, the diagnostic artifact, and
the cadence worker driver. `addressbook_fetch.rs` is a **private** module whose only network
capability is a loopback eepProxy connect over an explicitly configured local HTTP proxy.
`control_sources.rs` owns the bounded redacted `LogRing` (INFO+, secret markers, 256 entries,
192 B lines) with the `tracing` layer feed, the explicit `BanLedger` attesting the empty set,
and rolling `ControlMetrics`. Resolver-handle installation is explicit at each consumer:
`SamServiceState::set_addressbook_handle` (`src/sam.rs:353`) and
`ServiceTunnelManager::set_addressbook_handle` (`src/service_tunnels.rs:662`), wired from
`src/sam.rs:3179` and `src/service_tunnels.rs:6540`. `news.rs` is a **private** module.

### Service tunnels

`ServiceTunnelManager` (`src/service_tunnels.rs`) is the manager: explicit Destination-group
ownership, one persistent/ephemeral identity and Streaming bridge per shared group
(Plan 309), per-service listeners, and server-port dispatch. `reconcile` is a whole-set
transactional replace, so startup-owned specs must be carried through `candidate_set`
verbatim. The manager installs router delivery through `install_router_delivery` /
`uninstall_router_delivery` and exposes `router_service_candidates`,
`routing_decision_for`, and `co_owned_destination_hashes` as the typed remote-routing seams.
`RemoteDestinationBackend` (`src/service_delivery.rs:242`) is the executable backend that
owns the shared `DestinationTunnelCoordinator` plus the authenticated router delivery
service; `ServiceDestinationDelivery::new()` / `with_backend` / `has_backend` select it.
`service_generation.rs` holds the committed-generation bookkeeping;
`service_lifecycle.rs` is a **private** module holding only local Destination-group
phase/timing policy — no secrets, no routing state.

The product driver reconciles committed control generations before advancing the
Destination pools. Replacement builds remain in the one bounded coordinator, and the
per-group replenisher interleaves inbound-first and outbound submissions so one direction
cannot occupy the entire configured build window. Its provisioning snapshot exposes only
profile quantities, current registrations/pending counts, and coarse per-Destination
submission/completion/failure/cancellation/registration counters. Those counters are
pruned when their Destination leaves the committed generation; they expose no peer or
attempt identifiers. A present pool snapshot is not publication evidence: server LS2
installation and DatabaseStore delivery admission remain separate gates.

### Outproxy provider (Proposal 170) — reachable from all four request paths, loopback-evidenced only

Read this before writing "outproxy supported" anywhere.

**Superseded as of Plan 342.** Plan
[`343`](../../plans/closure/i2pcontrol-proposal-170/343-status.md) recorded
`passed-outproxy-provider-policy-and-route-owner-with-no-reachable-request-path`: *"No option
can set an outproxy yet, and no request path consults the provider."* That was accurate for
Plan 343 and is retained in its own record; it is no longer accurate for this tree.

What Plan [`342`](../../plans/closure/i2pcontrol-proposal-170/342-status.md)
(`passed-loopback-wire-lane-landed-live-failover-rotation-unproven`) changed:

- all seven canonical option fields are admitted as one **all-or-none** block on the four proxy
  client kinds, with `OutproxyPassword` sealed into Plan 341's stored form;
- the provider is installed by the real control-plane reconciliation into the manager's registry
  and reached from **all four** client request paths — HTTP forward, HTTP `CONNECT`, the strict
  `CONNECT` adapter, and SOCKS5 — through the single classifier `classify_client_target`;
- there is still **no direct clearnet fallback** and **no direct clearnet capability anywhere in
  the design**, and there is still **no direct-clearnet arm to remove later, because there is
  never a fallback**.

So "outproxy supported" is still **wrong** — but for a different reason than it was in Plan 343.
It is wrong because the only evidence is the self-composed **loopback** lane
(`crates/i2pr-daemon/tests/outproxy_loopback_wire.rs`, 8/8 rows), the live failover rotation and
a live restart are **unproven**, and **no Java I2P or i2pd outproxy has ever been exercised**.
Plan 327 remains `blocked` and **no outproxy capability is claimed**.

One caveat that is specific to this tree and easy to get wrong: the request-target grammars
only admit a clearnet authority when the tunnel has a provider installed. `TargetPolicy` (in
`i2pr-service-tunnels`) is the policy value, and `ServiceTunnelManager::target_policy` derives it
from the **provider registry** so the parser and the classifier cannot disagree. A reader who
changes the parser without changing that derivation will silently make every clearnet request
fail at the *parser* instead of at the route, with no compiler signal.

**The seam, which is the whole architecture of this repository in miniature.** Everything
decidable without a socket is runtime-neutral and lives in
[`i2pr-service-tunnels/src/outproxy.rs`](../../crates/i2pr-service-tunnels/src/outproxy.rs)
(1 724 lines): the closed `OutproxyType` dialect vocabulary, `OutproxyEndpoint`, `OutproxyList`,
the separate `OutproxyTarget` grammar, `OutproxyPolicy`, `OutproxyConfig::route` /
`route_attempt` / `permits_tunnelled`, the `OutproxyRoute` decision, the `OutproxyFailure`
refusal vocabulary, the `OutproxyProvider` trait, `NoOutproxyProvider`, and the
request/reply codecs. That crate opens no socket. The daemon's `outproxy_route.rs` is the
**route owner**: it supplies only the two things the composition root alone can — the I/O half
and the credential half — and it implements the runtime-neutral `OutproxyProvider` trait rather
than redefining the policy. `RouterOutproxyProvider::new` calls `config.validate()?` and
returns `Result<Self, OutproxyError>`, so an invalid configuration is refused at construction
instead of at first use.

**Single-route-kind invariant.** The only route this crate ever opens for an outproxy is an
I2P Streaming connection to an I2P destination, and there is no branch that would open a
clearnet socket after a failure. Three independent mechanisms enforce it:

| Layer | Mechanism | Where |
| --- | --- | --- |
| Structural | `OutproxyEndpoint::parse` refuses any list entry that is not an I2P destination — a clearnet host, an IP literal, a `host:port` authority, a `user@host`, or a non-`.i2p` label is `NotAnI2pDestination` | `i2pr-service-tunnels/src/outproxy.rs:183` |
| Behavioural | A clearnet target with no provider is a typed refusal; the `.i2p` bypass is re-evaluated **inside** `open_via_outproxy` per attempt rather than trusted from the caller, and an I2P target reaching the opener is refused as `NotPermitted` rather than quietly routed | `src/outproxy_route.rs:396–407` |
| Static | Rules 9–11 of `scripts/check-service-tunnel-boundaries.sh` scan **both** outproxy files for socket, resolver, TLS-client, plugin, and process spellings, with a positive control | see Boundary checkers below |

The honest test of this invariant is asserting the typed failure, not searching for a socket:
`open_via_outproxy` has **no caller** in `src/`, and the only socket-shaped operation in the
module is `bridge.streaming_mut().connect(…)` against the `DestinationRef` that
`OutproxyEndpoint::parse` already validated. Bounded by construction: `OutproxyPolicy::attempts`
(`MAX_OUTPROXY_ATTEMPTS = 4`, floor 1), `OUTPROXY_HANDSHAKE_DEADLINE_MS = 15_000` as a ceiling
separate from the connect timeout (a Streaming connection can establish and then never answer —
bounding only the connect would wait forever), `MAX_OUTPROXY_HANDSHAKE_BYTES = 8 * 1024` on the
staging buffer so a hostile or broken outproxy cannot grow it without bound, and a 5 ms poll
interval. Every close is a lifecycle event: `terminate` reads the port tuple from the live
connection rather than assuming it, so a mismatch fails closed inside the streaming manager
instead of closing the wrong stream.

**Diagnostic preservation at exhaustion.** `open_via_outproxy` runs a bounded attempt loop and
tracks the last attempt's reason in `last`. When the loop is spent it calls `note_exhausted`
(`src/outproxy_route.rs:466`), which records **both** facts — the terminal reason of the last
attempt *and* `attempts_exhausted` — instead of discarding the reason. This is the fix landed
with Plan 343's record commit (`5d3088ba`): the code previously did `let _ = last`, so "the
outproxy rejected the credential N times" was lost and the only surviving signal was
`attempts_exhausted`. The row `an_exhausted_request_records_both_its_last_reason_and_the_exhaustion`
asserts exactly that composition (`authentication_rejected == 1` **and** `attempts_exhausted == 1`),
so a discarded failure reason — the one diagnosis an operator needs to tell a wrong password
from a dead outproxy — cannot come back silently. `note_exhausted` is split out of the loop so
the composition is testable without a live route.

`OutproxyCounters` is counts-only and `Clone + Copy + Debug + Default`: no hostname, username,
header, or error string reaches it, so it is safe to project verbatim into a status or
`REPORT_STATUS` surface. `classify` is **total** over `OutproxyError` — every variant has an arm,
so a new codec error cannot silently become "some other failure" and be reported as a dead
outproxy. `AuthenticationRejected` and `TargetUnreachable` are the only retryable failures
(`is_retryable`); a missing provider or a missing secret owner is a configuration fact, and
retrying it would just be a loop.

**Secret handling on this path (verified).** `RouterOutproxyProvider` holds the Plan 341 **sealed**
stored form, never plaintext, and the plaintext's lifetime is the header construction inside
`auth_header`. It fails closed at every step: no sealed form, an owner that cannot open, a form
that does not authenticate, and a missing username each produce an error rather than an
unauthenticated upstream request, and the underlying store error is deliberately dropped so no
caller surfaces a value. Nothing in the module formats the username, the password, or the
header, and **no error variant carries an operator value** — `OutproxyError::MalformedCredential`
reasons are fixed strings. `build_attempt` refuses to attach an HTTP Basic credential to a SOCKS
outproxy instead of silently dropping it, and `OutproxyTarget`'s clearnet arm is a narrower
grammar than a resolver accepts (IP literals, a trailing root dot, empty labels, and
`user@host` are all refused), so i2pr cannot be used as a port-scan primitive against an
outproxy's network.

---

## Dependencies

### Production (`crates/i2pr-daemon/Cargo.toml`)

**40 production dependencies: 17 workspace path crates + 23 external.** The path crates are
the full composition edge set, matching the allowlist in
[`scripts/check-dependency-direction.sh`](../../scripts/check-dependency-direction.sh):

`i2pr-addressbook`, `i2pr-api`, `i2pr-app-proto`, `i2pr-client`, `i2pr-console`, `i2pr-core`,
`i2pr-crypto`, `i2pr-i2pcontrol`, `i2pr-netdb`, `i2pr-netdb-persist`, `i2pr-proto`,
`i2pr-runtime`, `i2pr-service-tunnels`, `i2pr-storage`, `i2pr-su3`, `i2pr-transport`,
`i2pr-tunnel`.

**External crates:** `chacha20poly1305`, `clap`, `eggserve-server`, `flate2`, `quick-xml`,
`rand_chacha`,
`rand_core`, `rcgen`, `rustix`, `rustls`, `rustls-pki-types`, `serde`, `serde_json`, `subtle`,
`thiserror`, `tokio`, `tokio-rustls`, `toml`, `tracing`, `tracing-subscriber`,
`webpki-roots`, `x509-parser`, `zeroize`.

**Binaries/examples:** `[[bin]] name = "i2pr"`, `path = "src/main.rs"`;
`[[example]] name = "sam_loopback_listener"` (plus `i2cp_loopback_listener` and
`service_tunnels_loopback_listener`).

### Dev

`tempfile`, `futures-executor` (a `block_on` for driving the synchronous
EggServe tower service in tests), and the workspace dev-dependency set, used for
filesystem and loopback test isolation.

### Notable dependency facts

- `i2pr-transport-ntcp2` and `i2pr-transport-ssu2` are **not** direct dependencies. NTCP2 is
  disabled by default and gated on an explicit qualification profile; SSU2 contracts are reached
  through the composition-owned path, not a direct crate edge.
- `i2pr-core` is declared and is the only path crate the daemon does not yet reference from
  `src/`; keep it declared (contracts/budgets/health are its reason) but do not invent usage.
- `zeroize` and `chacha20poly1305` are present specifically for the Plan 341
  `outbound_secret.rs` owner; `rustls`/`tokio-rustls`/`rcgen`/`webpki-roots`/
  `x509-parser`/`rustls-pki-types` serve I2PControl TLS and `service_tunnels_tls.rs`.
- **Plan 343 added no dependency edge.** The outproxy route owner reaches the runtime-neutral
  policy through the already-declared `i2pr-service-tunnels` and `i2pr-client` edges, and
  `rand_core`/`OsRng` for the Streaming SYN. The outproxy path needs no new HTTP or SOCKS client
  crate, which is the dependency-level statement of the no-direct-clearnet invariant: there is
  no `reqwest`/`hyper` to reach for.
- **Plans 356–358 added `i2pr-console` and `eggserve-server`, and that is all.** The console
  substrate is where `axum`, `argon2`, `serde`/`toml`, and `zeroize` live; the daemon
  deliberately does **not** depend on `axum` or `argon2` itself. `eggserve-server` is built
  with `default-features = false, features = ["tower"]`, so the `axum` integration and any
  default server header are not pulled into the daemon.
- **The MSRV floor moved `1.88 → 1.89` for the console lane.** Every published
  `eggserve-server` (0.2.0–0.4.0) and `eggserve-primitives 0.2.2` declares
  `rust-version = "1.89"`. The previous locked graph topped out at exactly 1.88.0, so
  `cargo check` hard-failed on the old floor once EggServe entered the graph. The bump
  is recorded in `AGENTS.md`, `Cargo.toml`, and `.github/workflows/ci.yml`.

### Boundary checkers (run on repo head)

| Script | Exit | Output |
| --- | --- | --- |
| `scripts/check-runtime-boundaries.sh` | **0** | `runtime boundary checks passed` |
| `scripts/check-dependency-direction.sh` | **0** | `dependency direction: ok` |
| `scripts/check-service-tunnel-boundaries.sh` | **0** | `service-tunnel boundary checks passed` (includes rules 9–11, below) |
| `scripts/check-console-boundaries.sh` | **0** | `check-console-boundaries: passed` |
| `scripts/check-console-browser-security.sh` | **0** | `check-console-browser-security: passed` |
| `scripts/check-m11-transit-boundaries.sh` | **0** | `check-m11-transit-boundaries: passed` |
| `scripts/check-m12-floodfill-boundaries.sh` | **1** | Plan 281 type-5 deferral grep now legitimately matches Plan 332/333/334 NetDB type-5 work. Not in `AGENTS.md` or `ci.yml`. See the note in **M12 floodfill** above. |

`check-console-boundaries.sh` rule 7 also asserts that **every** `i2pr-*`
workspace member appears in the dependency-direction `expected` map. That
rule is what keeps the map from drifting back to the `i2pr-tunnel` /
`tools/i2pr-interop` gap closed during Plans 356–358: adding a crate without
a map entry now fails CI instead of silently escaping the check.

### Rules 9–11: the outproxy static guard (Plan 343)

`scripts/check-service-tunnel-boundaries.sh` treats the two outproxy files as a **pair**. This
is the one place where a runtime-neutral crate and the socket-owning daemon are guarded by a
single rule, which is what keeps the seam honest: the rule fails if *either* half grows a
capability the design forbids.

- **Both files must exist** — `crates/i2pr-service-tunnels/src/outproxy.rs` and
  `crates/i2pr-daemon/src/outproxy_route.rs`. A missing file is exit 1, so the guard cannot be
  satisfied by deleting the thing it guards.
- **Neither may name a direct-clearnet capability**: the pattern is
  `TcpStream|TcpListener|UdpSocket|to_socket_addrs|lookup_host|\bTcpSocket\b|openssl|native_tls|reqwest|hyper`
  over both files. `std::net::IpAddr` is deliberately **not** matched — parsing an address is how
  the target grammar *refuses* IP literals, the opposite of opening a socket. The first draft
  matched `std::net::` and correctly failed on the real source; narrowing it was the fix, and
  the distinction is recorded so a later reader does not "helpfully" widen it back.
- **Anti-vacuity positive control (rule 10)** — the same `TcpStream|TcpListener` pattern is run
  over `crates/i2pr-daemon/src/service_tunnels_http.rs`, which legitimately does name the local
  TCP listener a client speaks to. If that ever stops matching, the script exits 1 with
  *"rule 9 positive control no longer matches; the outproxy guard is vacuous"*. A negative grep
  that can no longer fail is worse than no guard, because it reports safety it is not checking.
- **Rule 11** — neither file may contain `libloading|dlopen|Library::new|Command::new|std::process`.
  `UseOutproxyPlugin` is a Proposal 170 wire boolean that selects the configured provider path,
  never a module to load, so a plugin or process capability in either half would be something the
  guardrails do not allow.

Plan 343 recorded a **vacuous inversion** in its own evidence harness: the first `Command::new`
inversion targeted a function in the wrong file, the edit silently no-opped, and the checker
correctly reported "passed". Had the edit been believed applied, the closure record would have
claimed evidence for a guard that was never exercised. The lesson is the anti-vacuity mechanism
above: **an inversion harness must verify its edit applied.**

---

## Tests

**134 integration test files** in `crates/i2pr-daemon/tests/`, plus in-crate `#[cfg(test)]`
modules (notably `src/lib.rs:5496``, `src/config.rs`, and the module-local test blocks in
`console.rs`, `i2pcontrol_dispatch.rs`, `outbound_secret.rs`, `outproxy_route.rs`,
`transit_volume.rs`, and `service_els2.rs`).

The area inventory below names **72** of the 134 files. The remaining 62 were
added by later plans and are not yet assigned an area; the inventory is not
inflated to hide that.

### Acceptance-suite inventory by area

| Area | Count | Representative files |
| --- | --- | --- |
| CLI | 1 | `cli.rs` |
| NetDB | 9 | `netdb_integration.rs`, `netdb_tunnel_unit.rs`, `netdb_tunnel_live.rs`, `netdb_tunnel_external.rs`, `netdb_routing_consumer.rs` — the Plan 497 routing-table consumer lane: with the table attached the lookup asks the table's floodfill and never the flat store's nearer one, without it the lookup falls back, and both fallback reasons are counted, `netdb_peer_budget_receipt.rs` — the Plan 500 lane: the routing driver's receipt counts a lookup that spent its per-lookup peer budget in a bucket **separate** from the bucket for a lookup that ran out of candidates, `peer_reconcile_boundary.rs` — the Plan 506 static guard: the reconcile's lock order, its canonical write APIs, its bounded cadence, the single unsolicited-store ingest and the version gate at selection, `peer_reconcile_live.rs` — the Plan 506 out-of-crate lane: a peer that arrives through the production unsolicited-store path becomes known to both build-with stores after one reconcile, `netdb_retention_feasibility.rs` — the Plan 507 feasibility artefact: a store at its quota refuses a new peer and accepts it after a prune, `netdb_retention_boundary.rs` — the Plan 507 static guard: the retention window is the router's own acceptance window, the prune is the tree's own explicit prune, the ordering is prune then snapshot, the locks are one at a time, every prune is counted and no configuration key appears, `netdb_retention_live.rs` — the Plan 507 out-of-crate lane: the product prunes both live stores, a store at its quota turns over, a poisoned lock is counted and the router's own RouterInfo is never a victim, `transit_creator_retention_feasibility.rs` — the Plan 508 feasibility artefact: the window is derived from the coordinator's own bounds, the facts are bounded over many simulated builds and a live fact is never declined early, `transit_creator_retention_boundary.rs` — the Plan 508 static guard: the window is derived, every install carries an expiry, every use site consults it, the sweep drops the expired facts and no configuration key appears, `transit_creator_retention_live.rs` — the Plan 508 out-of-crate lane: a fact inside its window still bypasses transit, a fact at the last live instant still bypasses it, and an expired fact does not |
| Exploratory tunnels / build / liveness | 4 | `exploratory_build_unit.rs`, `exploratory_build_live.rs`, `exploratory_tunnel_external.rs` |
| Destinations (M6) | 3 | `destination_tunnel_unit.rs`, `destination_tunnel_live.rs`, `destination_tunnel_external.rs` |
| Streaming (M6) | 3 | `streaming_tunnel_unit.rs`, `streaming_tunnel_live.rs`, `streaming_tunnel_external.rs` |
| SAM 3.1 | 10 | `sam_loopback.rs`, `sam_forward_naming.rs`, `sam_plan146_reference.rs`, `sam_stream.rs`, `sam_stream_independent.rs`, `sam_stream_product.rs`, `sam_stream_raw_product.rs`, `sam_stream_self_composed.rs`, `sam_stream_final_acceptance.rs` |
| I2CP (M9) | 6 | `i2cp_loopback.rs`, `i2cp_message_data_plane.rs`, `i2cp_zero_hop_lifecycle.rs`, `i2cp_final_acceptance.rs`, `i2cp_adversarial_matrix.rs`, `i2cp_resource_matrix.rs` |
| I2PControl (Proposal 170) | 6 | `i2pcontrol_base.rs`, `i2pcontrol_tunnels.rs`, `i2pcontrol_inspection.rs`, `i2pcontrol_differential.rs`, `i2pcontrol_shared_service_manager.rs`, `i2pcontrol_els2_black_box.rs` |
| Router console (Plans 356–358) | 1 | `console_loopback.rs` — real `127.0.0.1:0` socket: shell, assets, path traversal refusal, bounded shutdown, the authenticated authority policy, and one end-to-end case that reads **real** control data through `ConsoleControlClient` |
| Service tunnels (M10) | 24 | `service_tunnels_foundation.rs`, `service_tunnels_local_roundtrip.rs`, `service_tunnels_final_acceptance.rs`, `service_tunnels_adversarial_matrix.rs`, `service_tunnels_remote_qualification.rs`, `service_tunnels_application_remote_qualification.rs`, `service_tunnels_remote_route_integration_qualification.rs`, `service_tunnels_remote_transport_qualification.rs`, `service_tunnels_application_product_only_remote_qualification.rs`, `service_tunnels_application_genuine_remote_qualification.rs`, `service_tunnels_independent_application_clients.rs`, `service_tunnels_plan210_real_service_destination_material.rs`, `service_tunnels_plan212_router_backed_product.rs`, plus 12 per-profile `service_tunnel_*_product.rs` suites |
| M11 transit | 3 | `m11_transit_data_plane.rs`, `m11_transit_live_owner.rs`, `m11_transit_i2pd_external.rs` |
| M12 floodfill | 4 | `floodfill_controlled_lifecycle.rs`, `floodfill_normal_optin.rs`, `floodfill_i2pd_external.rs` |
| SSU2 | 6 | `ssu2_daemon_preflight.rs`, `ssu2_introducer_mesh.rs` — the Plan 490 private controlled mesh for the introducer (Bob) role, `ssu2_introducer_handoff.rs` — the Plan 498 relay-handoff drain lane: the discriminator that a relay block reaches the role owner, the control, the refusal row and the bound row, plus the Plan 499 carriage rows that the drain carries the produced block to the tag's target and counts a carriage refusal by reason, `ssu2_introducer_live_mesh.rs` — the Plan 499 live three-party UDP mesh, `ssu2_qualification_export.rs`, `ssu2_qualification_receipts.rs` |
| NTCP2 operator profile | 1 | `ntcp2_private_lan_profile.rs` — the configuration refusals, the family-coherence rule, the controlled profile's refusal to widen to a private-LAN bind, the advertisement refusal, and one environment-gated row that binds the operator's real RFC1918 endpoint on both transports and asserts the record carries no capability letter |
| Java / M6 cross-family | 1 | `java_tunnel_external.rs` |

### Black-box product discipline

These suites drive behavior **only** through real TCP / SAM / I2CP / I2PControl after
listener startup. They must not call private bridge, `LeaseSet2`, driver, or pump APIs:
`sam_stream_self_composed.rs`, `sam_stream_product.rs`, `sam_stream_raw_product.rs`,
`sam_stream_final_acceptance.rs`, `i2cp_message_data_plane.rs`, `i2cp_final_acceptance.rs`,
`i2cp_adversarial_matrix.rs`, `i2cp_resource_matrix.rs`,
`i2pcontrol_els2_black_box.rs`, `service_tunnels_local_roundtrip.rs`,
`service_tunnels_plan212_router_backed_product.rs`, the `service_tunnel_*_product.rs`
family, and `console_loopback.rs`. Each listener binds `127.0.0.1:0` and uses loopback only.

`console_loopback.rs` probes must send `Host: localhost:<real-port>`, not a bare
`localhost`: the authority policy allow-lists exact authority values including the
port, so a bare `localhost` is correctly refused with `403` and a test that
"helpfully" relaxed the header would be testing the wrong thing. HTTP/1.1 header
names arrive lowercase on the wire, so the header assertions compare
case-insensitively.

### Runtime-test discipline

Loopback suites flake under parallel Cargo. Run the daemon's suites with:

```text
cargo test --locked -p i2pr-daemon --test <name> -- --test-threads=1
```

macOS CI builds every test executable **once** and then runs each one with
`--test-threads=1` rather than letting Cargo parallelize the loopback listeners.
Environment-gated external lanes are `#[ignore]`-gated and require an explicit
`--ignored --exact` run; a missing environment must **fail**, never silently pass.

Focused suites run while refreshing this document (repo head `5d3088ba`):

| Command | Result |
| --- | --- |
| `cargo test --locked -p i2pr-daemon --test cli --test netdb_tunnel_unit -- --test-threads=1` | **ok** — `cli`: 7 passed, 0 failed; `netdb_tunnel_unit`: 22 passed, 0 failed |
| `cargo test --locked -p i2pr-daemon --lib outproxy_route -- --test-threads=1` | **ok** — 12 passed, 0 failed, 538 filtered out. Includes `an_exhausted_request_records_both_its_last_reason_and_the_exhaustion` |
| `cargo test --locked -p i2pr-daemon --test cli -- --test-threads=1` | **ok** — 7 passed, 0 failed |
| `bash scripts/check-service-tunnel-boundaries.sh` | **ok** — exit **0**, `service-tunnel boundary checks passed` |

The full 72-file daemon suite and the full workspace suite were deliberately **not** run
during this documentation refresh.

### `tests/cli.rs` in detail

Invokes the compiled binary via `Command::new(env!("CARGO_BIN_EXE_i2pr"))`:

| Test | Coverage |
| --- | --- |
| `help_and_version_are_available` | `--help` lists subcommands; `--version` prefix |
| `missing_config_maps_to_exit_code_ten` | Missing -> 10 |
| `missing_required_argument_maps_to_usage_exit_code_two` | Missing `--config` -> 2 |
| `malformed_and_unknown_config_are_rejected` | Malformed TOML -> 11, unknown -> 11, semantic -> 12 |
| `dry_run_succeeds_and_live_run_is_not_implemented` | `--dry-run` ok; live run -> 41 (identity load) |
| `identity_lifecycle_is_explicit_and_inspection_redacts_private_material` | Generate -> inspect, no secret text |
| `dry_run_does_not_create_identity_state` | `run --dry-run` does not create `data_dir` |

In-crate `#[cfg(test)]` in `src/lib.rs` and `src/config.rs` also cover composition
regressions (`daemon_graph_contains_no_ntcp2_transport_service`,
`daemon_graph_rejects_ntcp2_enabled_config`) and the `[ssu2]` NTCP2 activation safety rows.
The NTCP2 activation gate's own `#[cfg(test)]` module in `src/ntcp2_activation.rs` covers each
refusal variant by name, every non-RFC1918 address shape, and the invariant that the gate
never widens a scope.

---

## Distinctive design choices

1. **The daemon is the composition root, and that is legitimate.** It is the only production
   owner of Tokio, sockets, timers, channels, and every listener; `check-runtime-boundaries.sh`
   enforces the inverse for every other crate.
2. **NTCP2 is disabled by default and profile-gated.** The default is `false`. An enabled
   NTCP2 surface must name an explicit qualification profile, and only `controlled_loopback`
   and `private_lan_qualification` are accepted; every other shape is rejected during config
   validation with a stable semantic error naming the offending field.
3. **The composition graph never registers `ntcp2-transport`.** A `lifecycle` service owns the
   shutdown signal; `i2pr-transport-ntcp2` is not a direct dependency at all. The daemon owns
   its own `ntcp2-router` service instead.
4. **No default config path.** Every subcommand requires `--config`, so operator intent is
   always explicit and reproducible.
5. **`deny_unknown_fields` on all 22 `Raw*` config structs.** Extra keys are an error
   (exit 11), never a silently ignored typo.
6. **Schema version is `!=`, not `>=`.** `CURRENT_SCHEMA_VERSION` is `1`; any other value is
   `UnsupportedSchemaVersion`. Migration requires a binary update first.
7. **Profile is locked to `"balanced"`.** `RouterProfile` has a single variant; any other
   value is rejected. A deliberate placeholder for future routing policies.
8. **`ExitCode` is `#[repr(u8)]`** with explicit numeric assignments, asserted by integration
   tests, so operators and automation can depend on it.
9. **The I2CP pre-session reject path awaits `stream.shutdown()` before any bookkeeping**
   (`src/i2cp.rs:1136–1138`), making the close deterministic instead of drop-timing
   dependent.
10. **Secret types refuse both `Debug` and `Clone`.** `OutboundSecretKey` and
    `ServiceEls2Material` are zeroized or redacted, and an in-crate test proves the success
    type is not `Debug`.
11. **`TransitParticipation::Disabled` is a value, not an absence.** The disabled posture
    still projects counters (`0`/`0`/`0.0`) so a relayed-nothing router reports truth rather
    than a gap; a poisoned lock is a gap, never a zero.
12. **The three `pub(crate)` internals are deliberate.** `addressbook_fetch`, `news`, and
    `service_lifecycle` are private because none of their invariants are consumer contracts.
13. **The live `run` path is async through the runtime owner.** `main()` hands `run_daemon`
    to `i2pr_runtime::run_blocking`; the binary has no `#[tokio::main]` of its own.
14. **The outproxy is split at the socket line, and the split is the design.** The
    runtime-neutral `i2pr-service-tunnels::outproxy` owns every decision; the daemon's
    `outproxy_route.rs` implements its `OutproxyProvider` trait and supplies only the Streaming
    connect and the credential. The daemon owns sockets, so the daemon owns the route — and
    nothing else about the outproxy.
15. **"No clearnet fallback" is structural, not procedural.** There is no fallback branch to
    remove later because there is never a fallback, and no `OutproxyFailure` catch-all a caller
    could read as "maybe try a direct socket". `OutproxyEndpoint::parse` makes every configured
    endpoint an I2P destination, and the `.i2p` bypass is re-checked inside the route owner
    rather than trusted from the caller.
16. **A static guard must be able to fail.** Rule 9's positive control requires the same pattern
    to keep matching `service_tunnels_http.rs`; if it stops matching, the checker fails rather
    than reporting a vacuous pass. Plan 343 also recorded the inverse lesson — an inversion
    harness that targets the wrong file no-ops and "passes" convincingly.
17. **Diagnostics survive exhaustion.** `note_exhausted` records the last attempt's reason
    *alongside* the exhaustion, so "rejected the credential N times" is available to an operator
    instead of being collapsed into "exhausted". A discarded failure reason is the one fact
    needed to separate a wrong password from a dead outproxy.
18. **A guard closure is a closure.** `send_handshake`'s dispatch request is dropped
    deliberately — the handshake cares only that the streaming manager accepted the bytes — and
    `terminate` reads the port tuple from the live connection so a mismatch fails closed instead
    of closing the wrong stream. Close is a lifecycle event, here as everywhere else.
19. **The console binds before it builds.** See
    [Router console](#router-console-plans-356358). Building the authority policy from a
    port that has not been resolved yet would either forbid the real port or allow whatever
    was configured instead of whatever was bound; binding first removes the question.
20. **A disabled console may not shadow another subsystem's budget.** `normalize_console`
    applies the runtime ceilings only when `console.enabled` is true. A *disabled* console
    keeps its defaults and never contributes a budget error, so a misconfigured `[console]`
    cannot be mistaken for a broken SAM or I2CP budget.
21. **The console's authorization is an allow-set, not a deny-list.** `LocalConsolePrincipal`
    names two methods. A new control method is invisible to the console until someone adds
    it to that set, which is the direction a read-only posture should fail in.
22. **A secret that survives config parsing must be hashed, not remembered.**
    `RawConsoleConfig` converts a plaintext `password` to an Argon2id PHC string during
    normalization and hands only the hash to `Config`; the plaintext is not stored, not
    `Clone`, and `ConsolePasswordHash` has a redacting hand-written `Debug`.
23. **The activation gate is a pure function, not an inline branch.** The configuration
    layer already refuses every shape the NTCP2 activation gate refuses, so a gate branch
    living inside the service closure could never be reached from a configuration file and
    would therefore be untested by construction. `ntcp2_activation_scope` takes an
    already-normalized `Ntcp2Config` and returns a scope or a typed refusal, which makes
    every branch reachable by a unit test — including the branch that proves the gate does
    not widen a scope.
24. **The routing table is a cache; the flat store is the authority.** The Plan 497 consumer may
    only **reorder** the flat store's own selection — never narrow it and never widen it — and
    every fallback is counted **by reason** (`fell_back_disabled`, `fell_back_no_candidate`, or
    `fell_back_unavailable` for a poisoned table). A consumer that could silently drop a
    candidate would make the table's answer indistinguishable from a smaller store, so the
    counted reason is the contract, not a diagnostic extra.

---

## The encrypted-service consumer path (Plan 351, ADR 0033)

Plan 349 built `encrypted_service_resolver.rs` with **zero** production callers. Plan 351 supplies
one, and this section exists because every layer here had a hidden assumption that a `.b33` broke.

### The chain

```text
I2PControl definition options          target_destination = <b33>, delay_open = true,
                                       leaseset_password = <optional lookup secret>
        |  (Gate 2; symmetric with the publisher's slot)
        v
ServiceTunnelSpec::validate           Gate 1: encrypted target requires a DelayOpen client
        |
        v
project_remote_target                  Three outcomes; a .b33 is never Remote or LocalCoOwned
        |  EncryptedService(address)
        v
resolve_encrypted_destination_for_service
        |
        +--> EncryptedServiceResolver::begin        today's blinded storage key
        +--> begin_encrypted_lease_lookup           key supplied VERBATIM, kind still LeaseSet2
        +--> ingest_tunnel_lease_store             -> EncryptedLeaseSet2Ready
        +--> resolver.ingest_store                  unwrap (ADR 0032 profile lives in here)
        +--> bind_inner_to_address                  signing-key + sigtype gate
        +--> ValidatedLeaseSet2 + install            under the INNER destination hash
```

### Three decisions a reader will otherwise get wrong

**The lookup key is not a destination hash.** Every ordinary lookup derives its key from a
`DestinationHash`; a blinded storage key cannot be derived from one, because no `Destination` exists
at lookup time. `NetDbSeam::begin_lease_set2_lookup_for_key_with_store` takes the key verbatim
instead. The lookup *kind* is unchanged — a reference client issues `LeaseSet2` (code `1`) for an
encrypted service — so **no new wire type is introduced**.

**The install key is the inner record's own destination hash, gated by a signature.** A `.b33`
carries the unblinded signing public key, which is *not* a `Destination` hash: the address lacks the
ECIES public key, the certificate, and the padding a `Destination` encoding needs. The hash can only
come from the record. `bind_inner_to_address` therefore compares the inner `Destination`'s signing
key and sigtype against the address, and only then returns the hash. Trust is transitive through a
signature against a key obtained out of band — not "the record said so". Without the gate, any valid
`LeaseSet2` for any destination would pass.

**The type-7 relationship does not generalize.** `service_els2.rs` publishes type 7, where
`DERIVE_PUBLIC(CONVERT_ED25519_PRIVATE(seed))` reproduces the destination's Ed25519 public key, so
address and inner record agree by construction. A type-11 `.b33` has no such relationship. The
binding is written against what production publishes, and
`the_installed_hash_is_the_unblinded_destination_hash` asserts the premise before using it.

### The failure cannot escape

`resolve_encrypted_destination_for_service` returns `Result<(), EncryptedTargetStatus>` — never a
`ServiceProductError`. That is a deliberate narrowing: this path runs where a propagated error shuts
the product down, so what can escape is a closed enum whose reasons are `&'static str` and which
cannot carry a secret, a derived key, or a fetched payload.
`provision_encrypted_service_target` records the outcome and returns `()`.

`EncryptedServiceResolver::cancel` runs on every early return, and `ingest_store` removes the
request from its table before it can fail, so the in-flight lease is released on every outcome.

### The secret

The consumer lookup secret arrives through the I2PControl definition options in the same
`leaseset_password` slot the publisher uses, is installed by the same reconciliation that installs
publisher material, and is held as `Arc<LookupSecret>` — `Zeroizing`, not `Clone`, no serde, redacted
`Debug`. It is never placed in `Config`, in a `Raw*Config` struct, or in a `DestinationRef`.
`scripts/check-config-secret-hygiene.sh` enforces that, and also records the pre-existing leak paths
that motivate it (see Plan 352).

### The consumer client credential (Plan 380)

A `.b33` that declares `B32_FLAG_REQUIRES_CLIENT_KEY` needs the PSK or DH key its publisher
authorised this router to use. That value is the operator's and is not derivable from the address:
for PSK the client holds the same pre-shared key the publisher listed in `LeaseSetClientAuths`, and
for DH it holds the private half of the key pair whose public half the publisher listed. It is
carried as one more option, `leaseset_client_credential`, with a closed grammar —
`psk:<64 lowercase hex>` or `dh:<64 lowercase hex>`. The DH form carries the private key only; the
public key is derived from it, because the record names the client's own public key and a
configuration able to state a mismatched pair would be a credential that can never authorize
anything, for a reason the operator could not see.

**It has no Proposal 170 field.** Proposal spells the *publisher's* authorized-client list and
expects a consumer to already hold the matching secret out of band. So the value crosses the wire
inside `CustomOptions` as `{"i2pr": {"LeasesetClientCredential": "..."}}`, shaped by
`i2pr-i2pcontrol::extension_options`. The untyped `CustomOptions` blob form stays refused for the
reason it always was, the frozen `PROPOSAL_TUNNEL_MANAGER_FIELDS` inventory is untouched, and an
unrecognised extension name is refused rather than ignored.

**Where the plaintext exists.** In exactly one place: `normalize_definition_with_filter_root`, which
parses it for its grammar and hands it to the secret owner before the definition can reach a
generation file. The manager holds only `SealedEncryptedTargetCredential` — ciphertext plus the
owner that can open it — and its install signature takes that type, so "the manager never holds the
key" is a property of the signature rather than a rule. The plaintext is opened once more in
`resolve_encrypted_destination_for_service`, scoped to one resolution, and dropped zeroized when
that function returns. It is not in `Config`, in any `Raw*Config` struct, in a `DestinationRef`, in
`SECRET_OPTIONS`, or in any status surface: `get` and `rawConfig` report
`clientCredentialConfigured` and nothing else.

**It is a second sealed domain, not a reuse of the first.** Plan 341's router-bound owner derives
its key from `i2pr:outproxy:secret-box:v1`; the credential derives from
`i2pr:els2-consumer-credential:secret-box:v1`, and each carries its own stored-form marker
(`$i2pr1o$` / `$i2pr1e$`) used as AEAD associated data. Sharing one key would let a stored outproxy
credential copied into this slot open successfully there. Both domains come from one `Arc` —
`RouterSecretOwner` is a supertrait — so "derive once from the router identity" stays literally
true and the outproxy runtime keeps an ordinary `Arc<dyn OutboundSecretStore>` by upcasting.

**Both seal steps are idempotent.** `edit` merges the stored options with the request's, so
`normalize_definition_with_filter_root` runs again over a value that is already a stored form. Each
seal step recognises its own marker and passes the form through. See *The seal is not idempotent*
below for why that is a correctness property and not a nicety.

### The seal is not idempotent (defect found and corrected by Plan 380)

`edit` merges options, so the seal step runs a second time over the credential the definition
already holds. Before Plan 380 corrected this, that re-sealed the stored form **in both domains**:

- The ELS2 credential's seal step tried to parse its own ciphertext as a plaintext credential, so
  *every* edit of a credential-bearing service failed outright.
- The Plan 342 outproxy credential re-sealed silently. The stored form became sealed twice; opening
  it once returned the *previous stored form as text*, which `RouterOutproxyProvider` then presented
  to the outproxy as the HTTP proxy password. The outproxy answers 407 and no status surface says
  why.

Plan 342's rows could not see the second case because they exercised generation round trips — which
do not re-run the seal step — and no Plan 342 or Plan 376 row ever edited an outproxy tunnel.
`plan342_sealed_block_survives_a_generation_round_trip` carried the comment "so an untouched
credential survives an edit"; the first clause was true and the second was untested. Both claims are
now tested, by `plan380_an_unrelated_edit_leaves_the_sealed_credential_unchanged`, and both seal
steps are pinned by `scripts/check-encrypted-service-consumer-caller.sh`.

### Removing a credential

`edit` merges, so an edit that omits the credential does **not** remove it — the same limitation
Plan 376 found and pinned for the outproxy block, with the same remedy: `delete` the definition and
`create` it again. `sync_encrypted_target_credentials` drops the credential when a definition stops
naming an encrypted target, when the option is absent from the merged options, and in a sweep for
definitions no longer in the map. A deleted definition therefore leaves nothing behind.

### What this does not claim

No daily rollover re-resolution, no cross-router result, no Java or i2pd direction of Plan 347. The
blinding rotates daily and there is no periodic re-resolution, so a resolution computed before a
midnight boundary addresses the **wrong DHT key**. Type 5 stays `advertised = false`, and Plan 380
adds no support surface: the credential is i2pr-local configuration, not an advertised capability.

## The exploratory request's target (Plan 514)

Plan 513 landed the discovery spine. The request it sent was **aimed
wrongly**, and the operator could not switch it off.

### The aim was the flat store's iteration order

The live cycle picked the peer to ask with `store.floodfill_advertisers().next()`.
The store is a `BTreeMap` keyed by the peer's hash
(`crates/i2pr-netdb/src/store.rs:84`), so its first floodfill advertiser is the
**lowest-hash** one, and the choice did not depend on the search key at all:
every exploration asked the **same** peer, until a smaller-hash floodfill
entered the store. A peer that did not answer was asked again and again.

The routing table's closest-first selection — the mechanism Plan 489 built,
Plan 497 exposed through `RoutingTableConsumer::candidates`, and Plan 513 gave
a reader — was **not consulted** on that path. The table's own exploration
entry point, `RoutingTableConsumer::begin_exploration`
(`src/netdb_participation.rs:894`), had **no production caller**.

### The target is the table's closest floodfill

`RoutingTableConsumer::closest_floodfill` (`src/netdb_participation.rs:1000`)
asks the table for its closest-first answer and returns the first candidate.
The spine's exploratory start (`src/netdb_participation.rs:1424`) asks it for
the search key's routing key — the key the router **already computed** for the
request — and addresses the request to that peer.

The reference asks the same question of its own floodfill bucket set
(`libi2pd/NetDbRequests.cpp:479`, at the exact pin); the derivation, and the
distance rule the tree uses, are in `specs/QUALIFICATION-GATES.md` §13.

### A request the table cannot target is refused, not redirected

A table that names no floodfill — off, empty, or unreachable — ends the
request. The refusal is counted under `explorations_refused_no_target`
(`src/netdb_participation.rs:1130`), and the request is **never** redirected
to an arbitrary peer. The reference ends its request too, rather than asking a
substitute (`libi2pd/NetDbRequests.cpp:502-503`).

### The operator's switch is real

`netdb.exploration_enabled` is now read, at
`src/lib.rs:1943`, and consulted by the discovery cycle at
`src/lib.rs:2087`. It was parsed, defaulted to `false` and cross-validated
against `netdb.routing_table_enabled`, and read by **nothing**: the
exploration ran whenever the routing table was on.

The switch gates the **exploratory request** only. The RouterInfo-kind
resolution stays above it, so the routing table keeps its production reader
and Plan 497's **M2** is not reopened. The switch's default does **not**
move: the key's own documentation already said the exploration is off by
default and that it cannot be enabled while the routing table is off, so the
wiring was missing, not the design.

### What is deliberately not claimed

- The **peer exclusion** the reference applies — it never asks a peer it has
  already asked — is **not** implemented, and the exploration asks **one**
  peer per exploration rather than advancing within one. The exploration's
  budget and advance belong to Plan 489/500/512; the gap is recorded as a
  finding for a successor.
- The **stock-peer** half remains blocked externally on Plan 493's authorized
  two-host topology.
- Nothing is advertised: `router.version` stays `0.9.62`, `caps` and the
  published RouterInfo are unchanged, and the inventory's advertised count
  stays zero.

## The live NetDB discovery spine (Plan 513)

Plan 497 recorded that the shared routing table had **no production reader**:
the daemon populated and persisted it, and the coordinator asked it, but the
coordinator was never in the daemon's service graph. Plan 513 gives the table a
live reader and makes the router's DHT discovery a driven cycle.

### The cycle

`NetDbParticipation::drive_discovery` (`src/lib.rs:2053`) advances one bounded
cycle on the participation owner's existing cadence:

1. a RouterInfo-kind lookup that is in flight is awaited;
2. an exploration that is in flight is awaited;
3. a named peer an earlier exploration found is resolved;
4. otherwise a new exploration starts.

The order is not arbitrary. An exploration's answer is peer **hashes**, and a
hash the router cannot resolve is a name, not a peer, so the resolution is the
**enabling** half and the exploration that produces names to resolve comes
first.

One request at a time, one exploration at a time, one resolution at a time. A
request the wire never answered is **abandoned** when its deadline passes and
**counted**; it is never retried in a loop.

### The two deliveries

`ServiceProduct::drive_netdb_discovery` (`src/service_product.rs:4199`) hands
the composed request to the transport, and the two kinds take **different**
paths:

- an **exploratory** request (wire lookup type `3`) is delivered **directly**
  to the floodfill being asked, by `deliver_discovery_direct`;
- a **RouterInfo-kind** request (wire lookup type `2`) rides the **outbound**
  exploratory tunnel, by `deliver_discovery_via_outbound_tunnel`, and takes its
  reply through the **inbound** exploratory tunnel.

Plan 102's closed authority amendment forbids substituting a direct peer link
for the exploratory-tunnel path, so a router that holds no usable outbound
exploratory role **refuses** the RouterInfo-kind request rather than sending
it directly. The refusal is counted.

### The table is a preference, never a replacement

The routing table's answer **reorders** the flat store's own selection. It
never narrows it: a peer the flat store could serve is never dropped, and a
peer the flat store would not have served is ignored. Every fallback — the
table off, the table empty, the table unreachable — is counted **by reason**,
on the table's own `RoutingTableCounters` and on the spine's
`NetDbDiscoveryCounters`.

### A suggestion is never evidence

The peers an exploration names enter the routing table as **suggestions**, and
the table accepts a suggestion only from a peer it already holds with **signed**
evidence. A name enters the spine's bounded unresolved-peer queue only when
the table **accepted** it: a refusal the router ignores would be a decoration.

A queued name is then resolved by a RouterInfo-kind lookup, and the reply's
signed record is validated by the lookup engine's own rules and inserted into
the router's NetDB — so the named peer becomes a peer the router can ask. A
reply from a peer the exploration never asked introduces **nothing**: the
reply's sender is unauthenticated.

### What the spine does not own

The spine owns **no task, no timer, no listener and no socket**. The daemon's
existing tick drives it: the tick advances the RouterInfo-kind validation
clock, refreshes the inbound exploratory tunnel's reply path, and hands the
composed request to the transport. The delivery outcome is counted as
delivered or failed — never silently.

### What is deliberately not claimed

No **stock-peer** evidence is claimed: the qualification that would exercise a
real exploration and a real RouterInfo lookup against a pinned stock router is
blocked externally on Plan 493's authorized two-host topology. The reference's
own **direct** fallback — i2pd 2.61.0 downgrades the exploratory request to a
type-2 RouterInfo lookup when the exploratory tunnels are unavailable — is
**not** implemented; the divergence is recorded in
`specs/QUALIFICATION-GATES.md` §12. The **version gate** is inherited, and its
scope is stated rather than assumed: Plan 491's DSM gate is scoped by its own
definition (`dsm_target_eligibility`) to the peers the router sends a **DSM**
to, which is the two **publication** paths; Plan 513 adds no DSM and changes
neither publication path. Nothing is advertised: `router.version` stays
`0.9.62`, `caps` and the published RouterInfo are unchanged, and the support
inventory's advertised count stays zero.

## The destination role in the tunnel liveness scheduler (Plan 512)

Plan 487 recorded, and left open, the finding that **the destination inbound
pool has no transit-hop failure detection**: a dead transit hop is invisible
to the router until the tunnel expires, so a destination whose tunnels die
with a hop is not replenished until the lease expires.

Plan 512 lands the mechanism. It is worth being precise about what was
already there, because the gap was **not** a missing scheduler.

### What was already there

- `src/tunnel_liveness.rs` — Plan 504's **pair-level** scheduler. A probe
  leaves through one tunnel and must come back through the other; the answer
  is correlated on the **acknowledged** message id; and a pair that misses
  **two consecutive** responses is reported unhealthy. The scheduler itself
  is **role-agnostic**: `register_pair` checks only the pair ceiling, the
  slot inequality and the duplicate key.
- `src/exploratory_build.rs` — the **exploratory** population,
  `exploratory_liveness_pairs`, which pairs the exploratory pool's outbound
  registrations with the inbound registration whose gateway is the **same
  peer**.
- `src/service_product.rs` — `sync_liveness_pairs`, which registers exactly
  that population, and `drive_tunnel_liveness`, which drives the scheduler on
  the existing tick.
- Every **consequence** of a failure: the pool's failure seam
  (`DestinationRuntime::mark_tunnel_failed`), the changed lease set, the
  daemon tick's republication trigger, the replenishment deficit, and the
  existing submit path that builds the replacement.

So the detection existed and the consequences existed. What did not exist was
the **population** for the destination role and the **routing** of its
failure.

### What Plan 512 adds

**The population** is a separate function,
`service_product::destination_liveness_pairs`, over a bounded binding table
(`destination_role_bindings`) that maps a coordinator destination-role slot to
the destination that owns it, the destination runtime's pool slot, the
direction, and the inbound's local receive id.

The pairing is deliberately **within one destination**: the destination's
outbound role carries the probe out and the same destination's inbound role
carries the answer back. Pairing across destinations would let one
destination's dead outbound fail **another** destination's inbound, which is
a false failure the plan must not produce.

**The composition** is `ExploratoryBuildCoordinator::compose_destination_liveness_probe`.
The destination role's inbound tunnel does not live in the exploratory pool —
it is bound in the shared data-plane registry under the destination-role slot
namespace — so its routing comes from the registry's typed
`InboundGatewayRoute` rather than the pool's `PublicTunnelRouting`. The
probe is addressed to the inbound tunnel's **gateway**, the way i2pd 2.61.0
addresses it at the exact pin (`libi2pd/TunnelPool.cpp:452`), so the answer
returns through the inbound tunnel the destination advertises and **no remote
cooperation** is required.

The exploratory composition keeps its **same-peer** precondition unchanged.
That rule is correct for the exploratory population — both its tunnels are
built to one peer — and would leave the destination population empty, because
the destination role's two directions are built in separate rounds from
separate peer selections.

**The failure arm** is `ServiceProduct::fail_destination_liveness_pair`. An
unhealthy destination pair fails the destination's inbound tunnel through the
runtime's own `mark_tunnel_failed` seam, so the **existing** republication
and replenishment paths are the ones that fire. i2pd's **"don't fail last
tunnel"** rule is adopted verbatim, and the refusal is **counted** rather
than silent.

### The safety property is inherited, not relaxed

Only an **authenticated** response proves a pair, and only a **proven** pair
may be failed. That is Plan 504's gate, and the destination population
inherits it unchanged. It is the property that bounds the blast radius of
the gateway-addressed probe: a destination probe path that is systematically
broken proves nothing, and therefore **fails nothing**.

### What is deliberately not claimed

- **Per-hop attribution.** The failure is a **pair-level** fact: the probe
  did not come back. No peer profile is written, and the decision type
  carries a pool slot and no peer identity.
- **i2pd's both-sides failure.** i2pd fails the outbound and the inbound of
  the pair; `i2pr` fails the **inbound** only. The divergence and its cost
  are recorded in the closure record.
- **Stock-peer evidence.** The mechanism is landed and proven in-tree. The
  qualification that would exercise a real dead transit hop is blocked
  externally on Plan 493's authorized two-host topology.

The static guard is `crates/i2pr-daemon/tests/destination_liveness_boundary.rs`;
the lane is `crates/i2pr-daemon/tests/destination_liveness_live.rs`. Plan
504's guard and lane stay **byte-identical**, and their SHA-256 digests are
pinned by the new guard, so the claim is checkable rather than asserted.

## The outbound peer-session pool (Plan 522)

The router used to dial per need and hold nothing between needs, so the
publication's ordinary path was the handshake path. Plan 522 gives the
router a **bounded pool** of outbound peer sessions, and the transport an
**outbound-only** keepalive that holds a pooled session open.

**What changed:**

- `src/peer_pool.rs` — the bounded owner. It is runtime-neutral: it owns no
  session, no socket, no task and no timer, and it reads its clock from its
  caller. The population is the router's **own** version-gated floodfill
  selection, under the **default** policy the publication's owner already
  holds, so there is no second policy and no second source of truth. The
  ceiling is one publication cycle's worth of peers (`15`) and the
  per-period dial budget is two; a candidate beyond either bound is
  **refused and counted** under its own reason, never evicted and never
  skipped silently. Every counter carries a stable reason label, and the
  admission is counted once per peer, so the receipt agrees with the pool's
  own peer list;
- `src/peer_pool_task.rs` — the pool's **own** task, for the same reason the
  publication has one: a session establishment is an await, and the daemon's
  `select!` loop must never block on a handshake. The bootstrap's lock is
  taken to read the population and is **released** before any dial, so a
  slow handshake can never hold the router's NetDB;
- `src/service_product.rs` — `DestinationDialPurpose::PeerPoolMaintenance`,
  the pool's own reason for a dial. The pool dials through the **one**
  existing narrow seam, so the router's own publication and the pool share a
  single dial path, and the publication's delivery path is **unchanged**;
- `src/config.rs` — `ssu2.peer_pool` is an explicit opt-in, **off** by
  default, refused unless the SSU2 service itself runs, and refused when the
  session's idle timeout cannot hold the keepalive's unanswered deadline;
- `src/router_i2np.rs` — the runtime's keepalive interval is derived from
  the pool's own switch and the transport's own constant, so a router
  without the pool sends no keepalive at all;
- `src/i2pcontrol_inspection.rs` — the counters reach the inspection plane.
  The cell holds counts only: it names no peer, no address and no key.

**The pool writes no peer profile.** A dial outcome is this router's
condition as often as the peer's, so the pool counts its outcomes in its own
counters and nowhere else.

**What is deliberately not claimed.** The **stock-peer** half needs Plan
493's authorized two-host topology. The **dial scope** limit is not
discharged: the transport's own dial target accepts a loopback endpoint or
an RFC1918 IPv4 address, so a public deployment cannot use the pool today,
and that limit is a transport boundary with its own plan-of-record. Nothing
is advertised: `router.version` stays `0.9.62`, the published record's
`caps` string is unchanged, and `Cargo.toml` and `Cargo.lock` are
byte-identical.

## The router's relay-tag request (Plan 524)

A router behind a firewall cannot be reached directly, so it needs an
**introducer**: a peer that will introduce it to the peers that want to talk
to it. Plan 524 gives the router the **acquisition** half of that
composition — it learns that a peer is an introducer, asks such a peer for a
relay tag while the router is firewalled, and records the answer as an
**authenticated introducer**. The **publication** half — building the
introducer-only address, choosing it over the direct form, and signing it —
is a successor's subject, and so is the `C` capability the router publishes
about **itself**.

- `src/introducer_request.rs` — the bounded owner. It is runtime-neutral: it
  owns no task, no timer, no listener and no socket, and the product's
  existing tick drives it. It refuses every window while the operator's
  opt-in is off, refuses every window while the router is not firewalled,
  and draws its candidates from the peers with a live session. The
  candidate window is **bounded** and **rotates**, so a busy router sees
  every peer it holds a session with over time instead of re-examining the
  same head of the list for ever; a peer the router already asked is not
  asked again while its session lives, and the asked set is pruned against
  the live sessions so it never outgrows them. Every refusal is counted
  under its own reason, and the counters name no peer, no address, no key
  and no tag;
- `src/introducer_request.rs` — `read_capability`, the read of the peer's
  advertised capability. It takes the **validated** record from the
  authoritative store and parses the address with the transport's own
  strict parser. A peer with no stored record, with no SSU2 address, with
  an unparsable address, or with an address that advertises no introducer
  letter is **not** an introducer, and each is a distinct refusal — never a
  silent absence. The test is the peer's **advertised** `C`, never its
  published introducer set;
- `src/service_product.rs` — `drive_introducer_request`, the tick step. It
  reads the router's own reachability state, draws the bounded window from
  the peers with a live session, reads their advertised capabilities, and
  queues **at most one** relay-tag request through the runtime. It never
  dials: the request rides a session the router already holds;
- `src/config.rs` — `ssu2.introducer_request` is an explicit opt-in, **off**
  by default, and refused unless the SSU2 service itself runs;
- `src/i2pcontrol_inspection.rs` — the counters reach the inspection plane.
  The cell holds counts only: it names no peer, no address, no key and no
  tag;
- `crates/i2pr-runtime/src/ssu2_runtime.rs` — the runtime's bounded
  outstanding-request set and the recording. A tag is recorded **only** in
  answer to a request this router made; a tag for any other peer is refused
  and counted, because recording an introducer nobody asked for would put an
  unauthenticated peer into the router's introducer set;
- `src/ssu2_relay_drain.rs` — the drain **skips** the tag answer. The
  handoff keeps the event — Plan 499's live-mesh evidence reads the minted
  tag there — but the role owner never served it, so it is not charged with
  a refusal it did not make.

**What is deliberately not claimed.** Nothing is advertised: `router.version`
stays `0.9.62`, the published record's `caps` string is unchanged, the
inventory's advertised count stays **zero**, and `Cargo.toml` and
`Cargo.lock` are byte-identical. The **stock-peer** half needs Plan 493's
authorized two-host topology.

## The router's published introducer set (Plan 525)

Plan 524 gave the router the **acquisition** half of the SSU2 introducer
composition — it learns that a peer is an introducer, asks such a peer for
a relay tag while the router is firewalled, and records the answer as an
**authenticated introducer**. Plan 525 gives the router the **publication**
half: it publishes the validated introducer set it learned, in the
specification's introducer-only address form. The `C` capability the router
publishes about **itself** is a third, separate matter.

- `src/self_publication.rs` — the owner. It asks the runtime for **which
  form** the router publishes and hands the address to the netdb builder
  that matches that form: `build_with_ssu2_address` for the direct form and
  `build_with_ssu2_introducer_address` for the introducer-only one. It
  never infers a form from a refusal, and it never writes an option. The
  counters **distinguish** the two published forms and the withdrawal, and
  the empty-introducer-set reason — which could never move before this
  plan, because nothing produced the refusal — has its own counter;
- `src/router_info_publication_task.rs` — the owner's counter snapshot is
  published into the live cell on the publication's own cycle. The counters
  are counts and stable reason labels only: they name no peer, no address,
  no key and no tag;
- `src/i2pcontrol_inspection.rs` — the cell reaches the inspection plane,
  so the running router's own published-address posture is observable. An
  unpublished or poisoned slot is a **gap**, never a fabricated zero;
- `crates/i2pr-runtime/src/ssu2_runtime.rs` — the runtime owns the
  decision, the material and the install gate. The daemon holds no copy of
  the publication rule;
- `crates/i2pr-netdb/src/local.rs` — the netdb's second builder and its
  second qualification rule. The builder carries **no** floodfill permit,
  so a record it builds is an ordinary router's record.

**The withdrawal is unchanged.** A firewalled router with **no** introducer
keeps publishing the netdb's ordinary zero-address record, where the
reference publishes an unpublished `s`+`i`+`v` form. That divergence belongs
to Plan 520's path.

**What is deliberately not claimed.** Nothing is advertised:
`router.version` stays `0.9.62`, the published record's `caps` string is
unchanged, the inventory's advertised count stays **zero**, and
`Cargo.toml` and `Cargo.lock` are byte-identical. The **stock-peer** half
needs Plan 493's authorized two-host topology.

## The router's advertised introducer role (Plan 526)

Plan 525 gave the router the **publication** of the introducer set it learned.
Plan 526 gives the router the **advertisement** of its own introducer role: the
specification's `"C"` letter in the `caps` option of the router's own address.

- `src/lib.rs` — the composition. The role owner is constructed **only** behind
  the explicit `ssu2.introducer_service` opt-in. The blanket refusal that
  Plan 520 kept made the role owner unreachable in **every** configuration, so
  the role could never run and Plan 499's closed claim that the daemon's live
  loop constructs it was false of the tree. The refusal is gone; the opt-in is
  the only gate, and the opt-in keeps its default and its refusals;
- `src/self_publication.rs` — the owner. It matches on the **typed** published
  form, hands the address to the netdb builder that carries that form's own
  rule, and counts the three published forms — the direct form, the
  **role-bearing** direct form and the introducer-only form — under their own
  reasons. The third rule's refusal has its own reason too. The owner's
  **apply** half is separated from its **ask** half, so the hand-off and the
  counters can be exercised with a material the caller supplies;
- `crates/i2pr-runtime/src/ssu2_runtime.rs` — the runtime owns the role's live
  state, composes the capability, and refuses a stale role letter at the
  install gate;
- `crates/i2pr-netdb/src/local.rs` and `crates/i2pr-netdb/src/floodfill_role.rs`
  — the netdb's **third** builder and its **third** qualification rule, beside
  the untouched direct and introducer rules. The builder carries **no**
  floodfill permit.

**What is deliberately not claimed.** Nothing is advertised in the inventory
sense: `router.version` stays `0.9.62`, the inventory's advertised count stays
**zero**, and `Cargo.toml` and `Cargo.lock` are byte-identical. The `B`
(peer-test) advertisement is out of scope, and the advertisement is confined to
the controlled loopback profile, so the `C` is a local qualification claim and
not yet a network-reachable one. The **stock-peer** half needs Plan 493's
authorized two-host topology.

## The router's reachability is read, not asserted (Plan 527)

The daemon's firewalled gate reads the runtime's tracker:

```rust
        let firewalled = self.inner.ssu2_handle.service().snapshot().reachability
            == i2pr_transport::ReachabilityState::Firewalled;
```

Before Plan 527 that read was **permanently false**. The tracker the product
reads could only ever receive confirming evidence, because every
contradicting signal was written into a second tracker inside the peer-relay
service that **nothing** read. So the gate refused every request — a router
that was not firewalled asked nobody, and a router that could never become
firewalled asked nobody either — and the introducer composition below it
never ran.

The gate itself is **unchanged**: the router asks only while its own
reachability state is `Firewalled`, and every refusal is still counted under
its own reason. What changed is that the state is now **reachable**, and the
gate's flag is read from the tracker rather than injected.

## Cross-references

### Deep dives

- [Overview](overview.md) — crate index and data flow.
- [i2pr-runtime.md](i2pr-runtime.md) — supervisor, `ServiceGraph`, `MAX_SERVICE_COUNT`,
  `run_blocking`.
- [i2pr-api.md](i2pr-api.md) — SAM 3.1 and I2CP wire/state machines.
- [i2pr-client.md](i2pr-client.md) — destination lifecycle, ECIES, Streaming.
- [i2pr-netdb.md](i2pr-netdb.md) — RouterInfo/LeaseSet2 validation, `ReplyPathProvider`, ELS2.
- [i2pr-tunnel.md](i2pr-tunnel.md) — `OutboundGatewayRole`, `LocalInboundEndpointRole`,
  `ExploratoryPool`, `DataPlaneRegistry`.
- [i2pr-service-tunnels.md](i2pr-service-tunnels.md) — HTTP/SOCKS5/IRC/Streamr parsing.
- [i2pr-addressbook.md](i2pr-addressbook.md) — the runtime half of the AddressBook.
- [i2pr-i2pcontrol.md](i2pr-i2pcontrol.md) — JSON-RPC and control-plane contracts.
- [i2pr-storage.md](i2pr-storage.md) — `IdentityStore`, `ByteCache`.
- [i2pr-crypto.md](i2pr-crypto.md) — `OsRng`, `RouterIdentityBundle`, `red25519`.
- [i2pr-proto.md](i2pr-proto.md) — I2NP envelopes, LeaseSet2 carriers.
- [i2pr-transport.md](i2pr-transport.md) — `DeliveryRequest`, `EncodedI2npMessage`, `Deadline`.
- [i2pr-su3.md](i2pr-su3.md) — SU3 framing and signature verification.
- [i2pr-netdb-persist.md](i2pr-netdb-persist.md) — `CacheLoader`, `ReseedIngestor`.
- [i2pr-console.md](i2pr-console.md) — the socketless console substrate this crate hosts.
- [NTCP2 private-LAN qualification procedure](../operations/ntcp2-private-lan-qualification.md) —
  the operator-facing procedure for the guarded operator profile.
- [tooling.md](tooling.md) — scripts, fixtures, lanes, CI.
- [dependency-graph.md](dependency-graph.md) — the allowlist this crate satisfies.

### ADRs

- [`0002` tokio runtime boundary](../../docs/adr/0002-tokio-runtime-boundary.md)
- [`0003` bounded supervised services](../../docs/adr/0003-bounded-supervised-services.md)
- [`0026` staged interoperability progression and Java debt](../../docs/adr/0026-staged-interoperability-progression-and-java-debt.md)
- [`0027` floodfill role provenance and advertisement](../../docs/adr/0027-floodfill-role-provenance-and-advertisement.md)
- [`0028` I2PControl Proposal 170 control plane](../../docs/adr/0028-i2pcontrol-proposal-170-control-plane.md)
- [`0034` EggServe/Axum router console HTTP substrate](../../docs/adr/0034-eggserve-axum-router-console-http-substrate.md)
- [`0035` Private manager protocol and inherited authority](../../docs/adr/0035-private-manager-protocol-and-inherited-authority.md)

### Closure records and plans of record

- **M6 destinations/Streaming** — Plan 134 authority; Plan 152 retained corrective; Plan 190
  `InboundGatewayRoute`; Plan 192 inbound-delivery closure; Plan 193 i2pd Streaming.
  Closure: [`130`](../../plans/closure/destination-streaming/130-status.md),
  [`119`](../../plans/closure/destination-streaming/119-status.md),
  [`122`](../../plans/closure/destination-streaming/122-status.md),
  [`124`](../../plans/closure/destination-streaming/124-status.md).
- **SAM** — Plans 149/150/151. Closure: [`147`](../../plans/closure/sam/147-status.md),
  [`149`](../../plans/closure/sam/149-status.md), [`151`](../../plans/closure/sam/151-status.md),
  [`152`](../../plans/closure/sam/152-status.md).
- **I2CP** — Plans 164–172. Closure:
  [`170`](../../plans/closure/i2cp/170-m9-i2cp-independent-clients-and-final-closure.md).
- **SSU2** — Plans 158–161. Closure: `plans/closure/ssu2/`.
- **SSU2 introducer (Bob) relay role** — Plans 490, 498, 499. Closure:
  [`490`](../../plans/closure/network-participation/490-status.md),
  [`499`](../../plans/closure/network-participation/499-status.md). Plan-of-record:
  [`498`](../../plans/implementation/network-participation/498-ssu2-introducer-role-owner-in-the-live-inbound-loop.md),
  [`499`](../../plans/implementation/network-participation/499-ssu2-introducer-live-tag-request-and-relay-block-carriage.md).
  Plan 498's implementation discharges Plan 490's **Medium #1** finding — the
  runtime's relay handoff was decoded, authenticated, and then dropped because
  the daemon never drained it — and it corrects that finding's prescription:
  `ServiceProduct::poll_inbound` is not on the live path, so the drain lives in
  the daemon's own inbound loop. Plan 499 discharges Plan 490's **Medium #2**
  finding — no live socket carriage of a Relay Request: the runtime gained a
  bounded outbound relay queueing API and the daemon's drain gained a
  carriage seam, and the live three-party UDP mesh shows the role completing
  on the wire. The role stays default-off and non-advertised.
- **M10 service tunnels** — Plans 174–180, 182, 213–215. Closure:
  `plans/closure/service-tunnels/`. Plan-of-record:
  [`174`](../../plans/implementation/service-tunnels/174-m10-service-tunnel-foundation-and-shared-stream-runtime.md),
  [`175`](../../plans/implementation/service-tunnels/175-m10-generic-client-server-service-tunnels.md),
  [`176`](../../plans/implementation/service-tunnels/176-m10-http-i2p-proxy-and-connect.md),
  [`177`](../../plans/implementation/service-tunnels/177-m10-socks5-i2p-connect-proxy.md),
  [`178`](../../plans/implementation/service-tunnels/178-m10-irc-client-profile-and-privacy-filtering.md),
  [`179`](../../plans/implementation/service-tunnels/179-m10-irc-server-profile-and-authenticated-peer-hostname.md).
- **M11 transit** — Plan 268. Closure:
  [`268-status.md`](../../plans/closure/transit-tunnels/268-status.md).
- **M12 floodfill** — Plans 270–283, 302–303, 306; type-5 floor later populated by Plans
  330–334. Closure: `plans/closure/floodfill/`.
- **NetDB network participation** (peer-profile store, floodfill-indexed routing table,
  consumer phase) — Plans 488, 489, 497. Closure:
  [`488`](../../plans/closure/network-participation/488-status.md),
  [`489`](../../plans/closure/network-participation/489-status.md). Plan-of-record:
  [`497`](../../plans/implementation/network-participation/497-netdb-participation-consumer-phase.md).
  Plan 433 owns the `NetDbTunnelCoordinator` composition into the daemon's service graph and
  is registered **blocked** on Plans 431/432, so Plan 497's routing-table consumer is a seam
  the daemon owns but does not yet compose into the tunnel-building path.
- **Exploratory tunnels** — Plans 185/188; Plan 109/110 corrective. Closure:
  [`117`](../../plans/closure/exploratory-tunnels/117-status.md),
  [`117-corrective-closure`](../../plans/closure/exploratory-tunnels/117-corrective-closure.md).
  Plan-of-record:
  [`185`](../../plans/implementation/mixed-router-interop/185-m6-live-one-hop-exploratory-tunnels-and-liveness.md),
  [`184`](../../plans/implementation/mixed-router-interop/184-m6-authenticated-i2np-runtime-and-reference-preflight.md),
  [`193`](../../plans/implementation/mixed-router-interop/193-m6-i2pd-mixed-router-streaming-qualification.md),
  [`108`](../../plans/implementation/exploratory-tunnels/108-conformance-amendment.md).
- **I2PControl / Proposal 170** — Plans 319–343. Closure:
  `plans/closure/i2pcontrol-proposal-170/`. Most recent daemon commits: `5d3088ba` (Plan 343
  floor + the `note_exhausted` comment/code fix), `a752ecc0` / `4dc69d00` (daemon outproxy route
  owner), `f9d404aa` (runtime-neutral outproxy policy), `11842388` (Plan 341 restart-safe
  outbound proxy secret owner).
  - [`343`](../../plans/closure/i2pcontrol-proposal-170/343-status.md) —
    `passed-outproxy-provider-policy-and-route-owner-with-no-reachable-request-path`.
  - [`341`](../../plans/closure/i2pcontrol-proposal-170/341-status.md) —
    `passed-outbound-secret-owner-with-no-routing-and-no-outproxy-claim`.
  - [`327`](../../plans/closure/i2pcontrol-proposal-170/327-status.md) — **still blocked**:
    `blocked-prop170-outproxy-provider-needs-routed-provider-and-secret-owner`. Its two dated
    2026-10-05 corrections narrow the blocker list from two to one; Plan 341 removed the
    secret-owner blocker, and the provider-gap blocker remains because the provider is not
    reachable by a client. **The status token is deliberately unchanged.**
  - Normative design record (frozen 2026-10-05):
    [`specs/references/proposal-170-outproxy-provider.md`](../../specs/references/proposal-170-outproxy-provider.md)
    — §1 states the one invariant, §2 records that **no pinned reference is authority** (i2pd has
    no I2P-routed outproxy at all; its only outproxy options are a clearnet upstream defaulting
    to `127.0.0.1:9050`), so the `OutproxyType` vocabulary and the `SSLProxies` subset rule are
    i2pr's own design and are **not** presented as interoperability-derived.
  - Plan 342 remains **registered** and open: the seven Proposal 170 option fields, the HTTP and
    SOCKS request-path integration, and the loopback outproxy wire lane. That gap is why Plan 343
    closed only half the line.
- **NTCP2** — Plan 101 guard; the development interop lane is closed and normal-daemon NTCP2
  stays disabled by default. An enabled NTCP2 surface is gated on an explicit qualification
  profile, and the operator-facing procedure for the guarded operator profile is
  [`docs/operations/ntcp2-private-lan-qualification.md`](../operations/ntcp2-private-lan-qualification.md).
- **Router console** — Plans 356–358, ADR 0034. Plan-of-record:
  [`356`](../../plans/implementation/router-console/356-eggserve-axum-self-contained-console-foundation.md),
  [`357`](../../plans/implementation/router-console/357-loopback-browser-security-and-optional-authentication.md),
  [`358`](../../plans/implementation/router-console/358-prop170-control-client-and-read-only-overview.md).
  Closure: `plans/closure/router-console/`. The console is **experimental, loopback-only,
  disabled by default, and non-advertised**; `specs/support.toml` is unchanged.
- **M6 Java** — Plan 236 is a bounded diagnostic blocked at
  `P236-C-JAVA-RESPONSE-EMISSION-OBSERVABILITY-GAP`; see
  [`236-status.md`](../../plans/closure/mixed-router-interop/236-status.md). Do not infer
  Java-family Router-A behavior.
- **Planning system** — [`plans/README.md`](../../plans/README.md),
  [`plans/registry.md`](../../plans/registry.md),
  [`specs/CONFORMANCE.md`](../../specs/CONFORMANCE.md),
  [`specs/support.toml`](../../specs/support.toml).

### Registry-lag warning

`plans/registry.md` and `specs/support.toml` lag their closure records and, in places,
contradict them. Where they disagree, `plans/closure/*/*-status.md` wins. Concretely, as of
this refresh:

- The registry's **narrative** paragraph still says "Plan 322 therefore remains blocked on
  the three transit selectors alone" and "Plan 334 is blocked", while both closure records
  say otherwise: [`322-status.md`](../../plans/closure/i2pcontrol-proposal-170/322-status.md)
  is `passed-canonical-routerinfo-sources-with-the-transit-participation-posture-unchanged`
  (Plan 340 closed Group A) and
  [`334-status.md`](../../plans/closure/i2pcontrol-proposal-170/334-status.md) was
  **reclosed** 2026-10-05 as `passed-mode-mapping-and-control-surface-complete` after Plans
  337/338. The registry's own tables (rows 107 and 118) are already updated — the
  narrative paragraph is the stale part.
- `plans/registry.md:39` (the Proposal 170 / I2PControl row) reads "Plans 322/327 remain
  blocked on router owners" and "Plan 334 is blocked with its control plane complete".
  **Half of that is stale and half is not**, so do not treat the row as uniformly wrong:
  322 and 334 are closed as above, but **327 genuinely is still blocked** with
  `blocked-prop170-outproxy-provider-needs-routed-provider-and-secret-owner` — its status
  token was deliberately left unchanged when its blocker list shrank from two to one. Plan
  343 does not unblock it, because the route owner is not reachable by a request path. The
  row also predates Plan 343 entirely, so it does not mention the provider that now exists.
- `specs/support.toml`'s M12 row still reads "Plan 281 deferred EncryptedLeaseSet type 5" and
  describes the floor as type 0/1/3/7. That is superseded by Plans 330–334, which populated
  type 5 (`m11_transit_tunnels` row, `specs/support.toml:370`).
- No Encrypted LeaseSet2 capability is advertised and no live interoperability is claimed.
  Full Proposal 170 conformance is not claimed.
