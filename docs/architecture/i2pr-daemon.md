# `i2pr-daemon` — Deep Dive

**Crate:** `i2pr-daemon` — **Path:** `crates/i2pr-daemon` — **Binary:** `i2pr` (`src/main.rs`)
**Size:** 52 `.rs` files, 78 256 lines of `src` (48 at the crate root + 4 under `src/sam/`).
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
- **Configuration** (`src/config.rs`, 4 587 lines): strict versioned TOML, `deny_unknown_fields`
  on all 22 `Raw*` structs, semantic validation, and normalization.
- **Identity lifecycle**: explicit generation and inspection of `<data_dir>/router.identity`.
  No auto-generated side effects on `run --dry-run`.
- **Identity -> NetDB bootstrap** (`bootstrap_daemon`, `src/lib.rs:1453`): identity load,
  cache revalidation, local RouterInfo construction, readiness, one optional bounded reseed.
- **Service-graph composition** (`src/lib.rs`): `i2pr_runtime::ServiceGraph`,
  `i2pr_runtime::Supervisor`, service lifecycles, and graceful shutdown.
- **ALL listeners.** SAM 3.1 (`sam.rs`), I2CP (`i2cp.rs`), I2PControl (`i2pcontrol.rs`,
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
- **M11 transit composition** (`transit_compose.rs`, `transit_owner.rs`, `transit_volume.rs`) —
  in the **disabled** posture only.
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

NTCP2 remains **DISABLED** in production: `default_ntcp2_enabled() == false`
(`src/config.rs:718`), `build_daemon_graph` rejects `ntcp2.enabled = true`, and
`ntcp2-transport` is never registered (Plan 101 guard). `i2pr-transport-ntcp2` and
`i2pr-transport-ssu2` are **not** direct daemon dependencies.

---

## Module layout

49 files at the crate root plus the `sam/` subdirectory (4 files). Line counts are from
`wc -l` on repo head. The tables below cover every `src/` file; the row count matches the
filesystem (53 rows).

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
| `src/config.rs` | 4 587 | Strict versioned TOML: 22 `Raw*` structs with `deny_unknown_fields`, semantic validation, normalization, `bind_socket()` / `loopback_test_profile()` helpers. Plans 356–358 add the `[console]` section, its redacting `Debug`, and `normalize_console` (loopback-only enforcement, bundled-theme validation, enabled-gated runtime ceilings, Argon2id PHC validation at parse time) | `Config`, `RouterConfig`, `LoggingConfig`, `LimitsConfig`, `NetworkConfig`, `Ntcp2Config`, `TransportConfig`, `NetDbConfig`, `ReseedConfig`, `ReseedSourceConfig`, `NewsConfig`, `SamConfig`, `Ssu2Config`, `I2cpConfig`, `I2pControlConfig`, `I2pControlPassword`, `FloodfillConfig`, `ServiceTunnelsConfig`, `RouterProfile`, `LogFormat`, `ConfigError`, `CURRENT_SCHEMA_VERSION`, plus `ConsoleConfig`, `ConsolePasswordHash`, `RawConsoleConfig` |
| `src/lib.rs` | 2 324 | Crate root: module declarations, `pub use` re-exports, `execute()` dispatch, logging init, bootstrap, service-graph construction, `run_daemon()`. Plan 360 adds the readiness contract (`readiness_expectation`, `service_spec`, `signal_running`, `bind_then_serve`) and the startup diagnostic (`describe_startup_failure`, `describe_supervisor_error`) | `CommandOutcome`, `IdentitySummary`, `execute()`, `initialize_logging()`, `bootstrap_daemon()`, `build_daemon_graph()`, `build_daemon_graph_with_inspection()`, `build_shared_service_manager()`, `run_daemon()` |
| `src/error.rs` | 128 | Typed error hierarchy and the stable exit-code mapping | `ExitCode` (`#[repr(u8)]`), `DaemonError` |

### Identity and bootstrap

| File | Lines | Responsibility | Key public types |
| --- | --- | --- | --- |
| `src/bootstrap.rs` | 631 | Plan 106 bounded NetDB bootstrap state machine, cache revalidation, bounded offline SU3 reseed ingestion, trust-set loading. Owns no runtime, sockets, or tunnels | `Bootstrap`, `BootstrapState`, `BootstrapPolicy`, `BootstrapSnapshot`, `BootstrapReport`, `ReseedAttemptSummary`, `BootstrapError`, `CacheLoaderReport`, `ReseedBundleReport` |

> `bootstrap_daemon` itself lives in `src/lib.rs:1453`, not in `bootstrap.rs` — it is the
> composition-root wrapper that loads the `IdentityStore`, builds `LocalRouterInfoBuilder` and
> `RouterInfoStoreConfig`, then calls `Bootstrap::run`.

### NetDB, discovery, and routing seams

| File | Lines | Responsibility | Key public types |
| --- | --- | --- | --- |
| `src/netdb_seam.rs` | 1 206 | Plan 106/107 runtime-facing seam over the Plan 105 lookup state machines; Plan 117 composition outcomes; Plan 122 dedicated LeaseSet2 lookup state machine and separate reply-path provider | `NetDbSeam`, `NetDbSeamError`, `CompositionOutcome`, `ExploratoryPathStatus`, `LeaseSet2ResponseOutcome` |
| `src/netdb_tunnels.rs` | 887 | Plan 186 NetDB-over-exploratory-tunnels coordinator: authoritative bounded store, ordinary-path reference bootstrap, floodfill verification, tunnel-path proofs, bounded lookup/publication/search matrices, typed tunnel loss | `NetDbTunnelCoordinator`, `NetDbTunnelError`, `NetDbTunnelCounters`, `TunnelPathProof`, `PublicationPathProof` |
| `src/outbound_lookup.rs` | 980 | Plan 117 §8/§10 outbound exploratory data-plane composition; Plan 187 `deliver_outbound_cells` for client-composed Garlic cells | `compose_outbound_lookup`, `compose_outbound_publication`, `deliver_outbound_cells`, `encode_standard_envelope`, `encode_store_envelope`, `OutboundLookupDispatch`, `OutboundLookupError`, `MAX_OUTBOUND_LOOKUP_CELLS = 8`, `MAX_OUTBOUND_PUBLICATION_CELLS = 8` |
| `src/inbound_dispatch.rs` | 290 | Plan 117 §9 inbound exploratory `TunnelData` dispatch; Plan 187 `GarlicComplete` outcome for destination carriers | `dispatch_inbound_tunnel_data`, `route_databasestore`, `route_database_search_reply`, `InboundDispatchOutcome`, `InboundResponseKind`, `InboundDispatchError`, `MAX_RECOVERED_ENVELOPE = MAX_I2NP_PAYLOAD_SIZE` |
| `src/destination_peers.rs` | 478 | Bounded projection and selection of validated Destination build peers from the validated RouterInfo store | `DestinationPeerCandidate`, `DestinationSelectionError`, `CandidateProjectionSummary`, `project_validated_store`, `select_destination_path`, `select_destination_path_os` |
| `src/peer_test.rs` | 417 | Plan 285 bounded I2NP peer testing: echo a `(message id, timestamp)` unchanged, match the echo against an outstanding probe, derive RTT from the supplied timestamp | `PeerTestTracker`, `PeerTestEcho`, `PeerTestOutcome`, `PeerTestError`, `MAX_OUTSTANDING_PEER_TESTS = 32`, `DEFAULT_PEER_TEST_TIMEOUT_MS = 10_000` |

### Tunnels, transit, and floodfill

| File | Lines | Responsibility | Key public types |
| --- | --- | --- | --- |
| `src/router_i2np.rs` | 1 607 | Plan 184 central authenticated router-I2NP dispatcher, narrow router delivery, daemon-owned `ssu2-router` service, controlled identity generation, reference RouterInfo verification | `dispatch_router_i2np`, `RouterI2npOutcome`, `RouterDeliveryService`, `Ssu2DaemonService`, `Ssu2DaemonHandle`, `generate_controlled_identity`, `verify_reference_router_info` |
| `src/exploratory_build.rs` | 2 201 | Plans 185/188/314 short-build owner plus Plan 390's explicit bounded mixed-role registry capacity: exploratory limits plus at most 32 service groups × 8 retained inbound roles and one transient outbound activation | `ExploratoryBuildCoordinator`, `BuildRequest`, `BuildDirection`, `PeerBuildMaterial`, `BuildCoordinatorOutcome`, `BuildCoordinatorCounters`, `SubmitResult`, `InboundRouteOutcome`, `tunnel_state_at`, `next_creator_tunnel_id_value` |
| `src/tunnel_liveness.rs` | 707 | Plan 185 bounded creator-side tunnel liveness scheduler: first test, repeat interval, response timeout, and failure threshold all bounded below the two-minute idle deletion boundary; one central scheduler, no per-tunnel task or timer | `TunnelLivenessScheduler`, `LivenessConfig`, `LivenessAction`, `LivenessTestId`, `LivenessCounters`, `LivenessError`, `route_inbound_with_liveness`, `first_due_after`, `repeat_interval`, `response_timeout` |
| `src/destination_tunnels.rs` | 1 412 | Plan 187 destination LeaseSet2/Garlic-over-tunnels coordinator; Plan 190 typed `InboundGatewayRoute` -> `i2pr_netdb::ReplyPath` adapter; Plan 201 §G sanitized boundary observation counters | `DestinationTunnelCoordinator`, `DestinationTunnelError`, `DestinationTunnelCounters`, `DestinationTunnelPathProof`, `RemoteMaterialProof`, `RemoteLeaseSummary`, `LeaseStoreIngestOutcome`, `reply_path_for_inbound_route`, `ReplyPathDerivationError`, `note_lookup_boundary` |
| `src/transit_compose.rs` | 4 231 | Plan 253 M11 transit composition: message-level STBM dispatch, role-specific STBM/OTBRM routing, `TunnelData` previous-peer forwarding, expiry/cancellation, delivery-failure rollback, creator-correlation bypass. **Disabled by default** (`TransitIngressGate::disabled()`) | `TransitBuildService`, `TransitIngressGate`, `TransitDispatch`, `TransitTunnelDataDispatch`, `TransitCounters`, `TransitServiceError` |
| `src/transit_owner.rs` | 1 393 | M11 live transit owner: build/disposition evidence types, OBEP delivery, live gateway/inbound outcomes, and the controlled-disabled probe used by the qualification lane | `TransitOwner`, `TransitLiveOwner<R>`, `TransitOwnerError`, `TransitLiveError`, `TransitDataDisposition`, `TransitDataForwardEvidence`, `ObepDeliveryOutcome`, `LiveBuildOutcome`, `LiveGatewayOutcome`, `LiveInboundOutcome`, `TransitBuildEvidence`, `controlled_transit_disabled_probe` |
| `src/transit_volume.rs` | 450 | Plan 340 transit volume, trailing-window bandwidth, and the authoritative **participation-posture owner** behind Proposal 170's three remaining RouterInfo selectors | `TransitParticipation` (`Disabled` default / `Enabled(Arc<Mutex<TransitVolumeCounters>>)`), `TransitVolumeCounters`, `TransitVolumeSnapshot` |
| `src/floodfill.rs` | 3 089 | M12 floodfill delivery boundary and the controlled activation/withdrawal composition: resource leases through effect completion, fresh bounded I2NP envelopes, Store acks, direct/tunnel lookup replies, `run_floodfill_owner` (bounded ingress + maintenance + effect drain) | floodfill owner/coordinator types, `activate_controlled`, `withdraw_controlled` |

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
| `src/app_manager_bridge.rs` | Plan 368 trusted AppManager bridge over an **injected** duplex stream: handshake, strict directional control loop, manager-asserted grants re-derived through the administrator path, capability check before backend allocation, exact SAM/I2CP octet forwarding through bounded per-stream queues, one backend watcher per stream, and deterministic teardown on manager EOF. No listener, no socket, no loopback fallback | `AppManagerBridge`, `AppManagerComposition`, `AppManagerBridgeError`, `ManagerTransport` |

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

### SAM 3.1

The TCP listener is an admission adapter over one SAM connection driver. The
same driver accepts an injected bounded async byte stream and an explicit
connection profile. Listener clients retain the ordinary loopback `STREAM
FORWARD` behavior; private managed-app connections deny host-target forwarding
before registration or any host connect. The private seam binds no listener,
requires no listener config, and never uses a localhost socket pair. Raw STREAM
mode transfers the generic transport into the same supervised raw driver.

| File | Lines | Responsibility | Key public types |
| --- | --- | --- | --- |
| `src/sam.rs` | 3 335 | Plans 137–149 supervised SAM 3.1 composition root; listener adapter and private async-stream entry share one driver; private SAM denies `STREAM FORWARD`; Plan 294 canonical address-book step in `NAMING LOOKUP` | `SamServiceState`, `SamServiceError`, private connection seam, `StreamingPools`, `execute_session_create`, `execute_stream_connect`, `execute_stream_accept`, `set_addressbook_handle` |
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
| `src/i2pcontrol_tunnels.rs` | 7 829 | Plan 289 TunnelManager control state over the **one** shared M10 manager (ADR 0031, Plan 337). The composition root builds that manager once in `build_shared_service_manager` and injects the same `Arc` into the control state and the destination-group product, so a control-created runtime is the runtime the product delivers and publishes through. `for_config` takes the manager rather than building one. The I2PControl service declares `depends_on("ssu2-router")`. Three shared-manager changes are load-bearing: `candidate_set` carries startup-owned specs through verbatim, `verify_agreement` scopes its "no extra runtime" rule to names this coordinator does not own, and `shutdown` reconciles back to the startup-only set instead of tearing the manager down. `sync_els2_materials` installs or drops the Plan 334 publication material at the end of a committed transaction and **fails the transaction closed** when a type-5 definition's identity record is unreadable. `rollback_state` (Plan 338) reconciles the shared manager to the rolled-back candidate; all five failure paths await it, and a rollback that cannot reconcile is logged, not swallowed | tunnel control-state, versioned generation store, transaction coordinator, twelve-family typed mapping types |
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
| `src/service_product.rs` | 5 186 | Plan 209 production composition helper wiring SSU2 + tunnels + LeaseSet2 + delivery backend; Plan 294 address-book install; Plan 318 normal-daemon adapter with bounded validated-RouterInfo handoff, bounded startup inbound replay, exact-three first-hop resolution, readiness gated on usable group pools; Plans 388–389 interleave bounded post-start builds and retain current-generation per-Destination stage counts; Plan 390 provisions aggregate role capacity; Plan 392 initializes identity-bound staged bridge state on committed router-backed generations | `ServiceProduct`, `ServiceProductSpec`, `ServiceProductError`; entry points `ServiceProduct::start` (`pub async`, `src/service_product.rs:895`) and `ServiceProduct::start_over_existing_daemon` (`pub(crate) async`) |
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

### The actual `pub mod` / `mod` declarations (`src/lib.rs:9–62`)

50 `pub mod` + 4 private modules + 4 `sam/` submodules:

`pub mod` — `addressbook`, `app_gateway`, `app_manager_bridge`, `app_runtime`,
`bootstrap`, `cli`, `config`, `console`, `control_sources`,
`destination_peers`, `destination_streaming`, `destination_tunnels`,
`encrypted_service_resolver`, `error`, `exploratory_build`, `floodfill`,
`i2cp`, `i2pcontrol`, `i2pcontrol_dispatch`,
`i2pcontrol_inspection`,
`i2pcontrol_tunnels`, `inbound_dispatch`, `netdb_seam`, `netdb_tunnels`,
`outbound_lookup`, `outbound_secret`, `outproxy_options`, `outproxy_route`,
`peer_test`, `router_i2np`,
`sam`, `service_delivery`, `service_els2`, `service_generation`, `service_product`,
`service_tunnels`, `service_tunnels_http`, `service_tunnels_http_bidir`,
`service_tunnels_http_server`, `service_tunnels_irc_client`,
`service_tunnels_irc_server`, `service_tunnels_socks5`, `service_tunnels_socks_irc`,
`service_tunnels_streamr`, `service_tunnels_tls`, `transit_compose`, `transit_owner`,
`transit_volume`, `tunnel_liveness`.

`console` and `i2pcontrol_dispatch` were added by Plans 356–358;
`app_gateway`, `app_manager_bridge` by Plan 368 and `app_runtime` by Plan 369.

Private — `mod addressbook_fetch`, `mod news`, `mod service_lifecycle`,
`mod app_runtime_qualification` (the Plan 369 WP5 `#[cfg(test)]` black-box
module; it compiles out of every real build, which is why it may name the
fixture binaries freely), and `mod tests` (in-crate test module).

### The actual `pub use` re-exports (`src/lib.rs:57–65`)

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
`MAX_ALLOWED_RESEED_BYTES = 16 MiB`, `MAX_ALLOWED_BOOTSTRAP_RECORDS = 65_536`.

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

### `bootstrap_daemon` (`src/lib.rs:1453`) — ordered startup

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

`run_daemon(config)` (`src/lib.rs:1482`) is the real daemon path:

1. Compute wall-clock seconds.
2. Run `bootstrap_daemon(&config, now_seconds, None)`.
3. Build one `InspectionHandles` from config and one `ServiceLifecycleController`.
4. Build the service graph via `build_daemon_graph_inner` (Plan 288 shares one
   `InspectionHandles` across the SAM/I2CP/I2PControl factories; Plan 295 installs the log
   ring, metrics, ban ledger, and static attestations; the SSU2 factory publishes its runtime
   service).
5. Publish the local RouterInfo base64 through `MAX_LOCAL_ROUTER_INFO_BYTES`.
6. Create the `Supervisor` with the graph, run it alongside the signal owner, then keep the
   normal SSU2 owner active through the bounded service-group drain before cancelling the
   supervisor and joining its scopes.

`main()` (`src/main.rs`) is the outermost shell: `Cli::parse()`, `execute(cli)`, and on
`RunReady` `i2pr_runtime::run_blocking(i2pr_daemon::run_daemon(config))` — the binary is
**not** synchronous; it hands an `async fn` to the runtime owner.

`ServiceProduct` has exactly **one** `pub` entry point, `ServiceProduct::start`
(`src/service_product.rs:895`); `ServiceProduct::start_over_existing_daemon` is
`pub(crate)`, the normal-daemon adapter that reuses an already-running SSU2 owner. Build the
one shared `ServiceTunnelManager` with `build_shared_service_manager` (Plan 337) and inject
the same `Arc` into both the control state and the destination-group product.

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

### `router_i2np.rs` — the controlled `ssu2-router` service

`Ssu2DaemonService` runs under `SSU2_SERVICE_NAME = "ssu2-router"` supervision
(`src/lib.rs:597`) and is the **only** inbound entry point for authenticated router I2NP.
`dispatch_router_i2np(inbound: &i2pr_runtime::Ssu2InboundI2np, now_ms: u64)`
(`src/router_i2np.rs:354`) is the single dispatch seam; it takes its input from the runtime
SSU2 session owner, so an inbound body only reaches it after the runtime has authenticated
the peer session. The dispatcher returns one of exactly four `RouterI2npOutcome` variants —
`TunnelBuildReserved { .. }`, `PeerTest { .. }`, `TunnelData { .. }`,
`RouterControl { .. }` — or `Unsupported { .. }` for a body kind it will not touch, and
fails closed on `RouterI2npError::Empty` / `TooLarge` before any decode
(`src/router_i2np.rs:471`, `474`). `dispatch_router_i2np_with_transit_bodies` (387) is the
same seam plus the bounded `TransitInboundBodies` extraction, and
`controlled_transit_disabled_probe` gates the transit-shaped path. `RouterDeliveryService`
is the narrow outbound seam over the existing `send_i2np` path.

The service **is** registered in the production graph: `src/lib.rs:1104` constructs
`Ssu2DaemonService::new(&ssu2_config, identity)` when `[ssu2] enabled = true`, and the whole
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
  disabled in production (Plan 101); SSU2 contracts are reached through the
  composition-owned path, not a direct crate edge.
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

**73 integration test files** in `crates/i2pr-daemon/tests/`, plus in-crate `#[cfg(test)]`
modules (notably `src/lib.rs:1718`, `src/config.rs`, and the module-local test blocks in
`console.rs`, `i2pcontrol_dispatch.rs`, `outbound_secret.rs`, `outproxy_route.rs`,
`transit_volume.rs`, and `service_els2.rs`).

### Acceptance-suite inventory by area

| Area | Count | Representative files |
| --- | --- | --- |
| CLI | 1 | `cli.rs` |
| NetDB | 5 | `netdb_integration.rs`, `netdb_tunnel_unit.rs`, `netdb_tunnel_live.rs`, `netdb_tunnel_external.rs` |
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
| SSU2 | 1 | `ssu2_daemon_preflight.rs` |
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

---

## Distinctive design choices

1. **The daemon is the composition root, and that is legitimate.** It is the only production
   owner of Tokio, sockets, timers, channels, and every listener; `check-runtime-boundaries.sh`
   enforces the inverse for every other crate.
2. **NTCP2 is disabled and unenableable.** The default is `false`, and explicit
   `ntcp2.enabled = true` is rejected during config validation with a stable semantic error.
3. **The composition graph never registers `ntcp2-transport`.** A `lifecycle` service owns the
   shutdown signal; `i2pr-transport-ntcp2` is not a direct dependency at all.
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
  stays disabled.
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
