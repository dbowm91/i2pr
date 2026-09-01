# `i2pr-daemon` — Deep Dive

Composition root and CLI entrypoint. Glues together the other
workspace crates into the `i2pr` binary. Provides a real Tokio
daemon composition with identity load, supervisor, service graph,
bootstrap pipeline, and graceful shutdown. NTCP2 is disabled
while support remains experimental; the daemon owns no NTCP2
listener under normal operation.

Path: `crates/i2pr-daemon/`

Binary: `i2pr` (declared via `[[bin]]` in `Cargo.toml`).

## Purpose

`i2pr-daemon` is the top of the dependency graph — it sees every
crate that will eventually participate in the running daemon. Its
work is scoped to:

- **CLI parsing** via `clap` derives: subcommands, flags, `--help`.
- **Configuration** parsing, validation, normalization, schema
  version checking; NTCP2 `enabled = true` is rejected while
  support remains experimental. Plan 106 added bounded `[netdb]`
  and `[reseed]` sections.
- **Identity lifecycle**: explicit generation and inspection of
  `<data_dir>/router.identity`. No auto-generated side effects on
  `run --dry-run`.
- **Daemon composition**: real Tokio supervisor, service graph,
  lifecycle service, netdb-bootstrap service, and graceful
  shutdown. `i2pr run` starts the daemon; it is no longer a
  non-networked shell.
- **NetDB/bootstrap pipeline** (Plan 106): runs cache revalidation,
  local RouterInfo construction, and bounded offline reseed before
  the supervisor starts. Produces a sanitized `BootstrapReport`
  with a bounded `BootstrapSnapshot` and `ReseedAttemptSummary`.
- **Stable process exit codes** that operators and automation can
  rely on.
- **SAM 3.1 loopback service** (Plans 137–147): owns the supervised
  listener, transactional sessions, per-destination Streaming pools,
  STREAM CONNECT/ACCEPT dispatch, loopback-only STREAM FORWARD
  registrations, bounded/cancellable raw forwarding, and local naming
  outcomes. SAM remains disabled by default and is never a public
  network listener. Plan 147 closed the dedicated same-socket
  raw-socket handoff to live destination delivery (see
  [`plans/147-status.md`](../../plans/147-status.md)). Plan 148 is the
  two-independent-client final closure authority and remains
  `blocked-external-client-build-failure` per
  [`plans/148-status.md`](../../plans/148-status.md).

What it **does not** do yet:

- Open NTCP2 listeners (disabled under current authority).
- Run `Ntcp2RuntimeService` or register `ntcp2-transport`.
- Apply live configuration changes.
- Drive a live exploratory tunnel build (Plan 107 lands the
  substrate; Plan 108 landed the local architecture but its
  wire/cryptographic algorithm is not protocol-conformant against
  the current official I2P Tunnel Creation Specification — see
  [`plans/108-conformance-amendment.md`](../../plans/108-conformance-amendment.md);
  the locally conformant build lands after Plan 109/110 corrective
  work).
- Accept HTTPS reseed at runtime (the offline source path is the
  only allowlisted Plan 106 acquisition path; HTTPS is deferred
  to a future plan).

## Module layout

Flat — nine files at the crate root (the row count in the table below
matches the filesystem; previous revisions of this doc listed six,
which undercounted `netdb_seam`, `outbound_lookup`, and
`inbound_dispatch`):

| File | Responsibility | Main items |
| --- | --- | --- |
| `src/main.rs` | Executable shell: parse CLI, dispatch through `execute()`, print results, map errors to stable exit codes | `main()` |
| `src/lib.rs` | Crate root: re-exports `cli`, `config`, `error`, `bootstrap`, `netdb_seam`; defines `execute()` (pure dispatch), `CommandOutcome`, `IdentitySummary`, `initialize_logging()`, `bootstrap_daemon()`, `build_daemon_graph()`, `run_daemon()` | `CommandOutcome`, `IdentitySummary`, `execute()`, `initialize_logging()`, `bootstrap_daemon()`, `build_daemon_graph()`, `run_daemon()` |
| `src/cli.rs` | CLI vocabulary — `clap` derives | `Cli`, `Command`, `IdentityCommand`, `CheckConfigArgs`, `IdentityArgs`, `RunArgs` |
| `src/config.rs` | Strict versioned TOML configuration (`serde(deny_unknown_fields)` everywhere) | `CURRENT_SCHEMA_VERSION`, `DEFAULT_MAX_TASKS`, `DEFAULT_MAX_BUFFERED_BYTES`, `RouterProfile`, `LogFormat`, `RouterConfig`, `LoggingConfig`, `LimitsConfig`, `NetDbConfig`, `ReseedConfig`, `ReseedSourceConfig`, `Config`, `ConfigError` |
| `src/error.rs` | Typed error hierarchy and stable exit-code mapping | `ExitCode`, `DaemonError` |
| `src/bootstrap.rs` | Plan 106 NetDB/bootstrap state machine | `BootstrapState`, `BootstrapPolicy`, `BootstrapSnapshot`, `BootstrapReport`, `ReseedAttemptSummary`, `Bootstrap`, `bootstrap_daemon`, `bootstrap_with_offline_reseed`, `build_trust_set`, `store_summary` |
| `src/netdb_seam.rs` | Plan 106/117 runtime-facing seam for Plan 105 actions | `NetDbSeam`, `CompositionOutcome`, `ExploratoryPathStatus` |
| `src/outbound_lookup.rs` | Plan 117 §8/§10 outbound exploratory data-plane composition | `compose_outbound_lookup`, `compose_outbound_publication`, `OutboundLookupDispatch`, `MAX_OUTBOUND_LOOKUP_CELLS`, `MAX_OUTBOUND_PUBLICATION_CELLS` |
| `src/inbound_dispatch.rs` | Plan 117 §9 inbound exploratory `TunnelData` dispatch | `dispatch_inbound_tunnel_data`, `route_databasestore`, `route_database_search_reply`, `InboundDispatchError`, `MAX_RECOVERED_ENVELOPE` |
| `src/sam.rs` | Plans 137–140 supervised SAM 3.1 listener and composition root | `SamServiceState`, `execute_session_create`, `execute_stream_connect`, `execute_stream_accept`, `STREAM FORWARD` ownership/bridge, local `NAMING LOOKUP`; Plan 140 audit and blocked handoff |
| `src/sam/streams.rs` | Plan 138 + Plan 143 + Plan 144 SAM Streaming bridge (captured-outbound seam removed, Plan 129 destination stack drives live bridge through `i2pr_client::deliver`, canonical-streaming routing for SYN responses) | `SamDestinationBridge`, `SamDestinations`, `bridge_to_peer`, `BridgeDiagnostics`, `SamDestinationHandle::lookup_by_peer_hash`, `receiver_streaming`, `peer_destination_hash`, strict destination decoding |

There are no subdirectories.

## Public surface

### Crate root (`src/lib.rs`)
- `pub mod bootstrap;`
- `pub mod cli;`
- `pub mod config;`
- `pub mod error;`
- `pub mod inbound_dispatch;`
- `pub mod netdb_seam;`
- `pub mod outbound_lookup;`
- `pub use error::DaemonError;`
- `enum CommandOutcome`:
  - `Validated { dry_run, config }`
  - `IdentityGenerated { path }`
  - `IdentityInspected { path, summary }`
  - `RunReady { config }`
- `struct IdentitySummary { signing_algorithm, encryption_algorithm }`.
- `fn execute(Cli) -> Result<CommandOutcome, DaemonError>`.
- `fn initialize_logging(&LoggingConfig)`.
- `fn bootstrap_daemon(&Config, now_seconds, offline_reseed_path) -> Result<(BootstrapReport, Arc<Mutex<Bootstrap>>), DaemonError>`.
- `fn build_daemon_graph(&Config) -> Result<i2pr_runtime::ServiceGraph, DaemonError>`.
- `async fn run_daemon(Config) -> Result<(), DaemonError>`.

### `src/config.rs`
- `enum RouterProfile { Balanced }`.
- `enum LogFormat { Text }`.
- `struct RouterConfig { data_dir, profile }`.
- `struct LoggingConfig { filter, format }`.
- `struct LimitsConfig { max_tasks, max_buffered_bytes }`.
- `struct NetDbConfig { enabled, max_records, max_encoded_bytes, min_router_infos, min_floodfill_advertisers }`.
- `struct ReseedConfig { enabled, max_sources, max_su3_bytes, sources }`.
- `struct ReseedSourceConfig { signer_id, certificate_path }`.
- `struct Config { schema_version, router, logging, limits, network, transport, netdb, reseed }`.
- `impl Config { fn load(&Path) -> Result<Self, DaemonError>; fn parse(&str) -> Result<Self, ConfigError> }`.
- `enum ConfigError` with `fn exit_code() -> ExitCode`.

### `src/error.rs`
- `enum ExitCode #[repr(u8)]`; `fn as_i32() -> i32`.
- `enum DaemonError` with `fn exit_code() -> ExitCode`.

## CLI surface

```
i2pr [--version] [--help]
```

About string (`cli.rs:12`): *"Experimental I2P router (NTCP2
disabled while support is experimental)."*

### Subcommands

| Subcommand | Flags | Description |
| --- | --- | --- |
| `check-config` | `--config <PATH>` (required) | Parse and semantically validate a configuration without side effects. |
| `identity generate` | `--config <PATH>` (required) | Generate and atomically persist a new router identity. |
| `identity inspect` | `--config <PATH>` (required) | Load and validate the existing router identity without displaying secrets. |
| `run` | `--config <PATH>` (required), `--dry-run` (bool) | Validate configuration and perform the future daemon startup path. |

All `--config` arguments are `#[arg(long)]`. **No positional
arguments, no default config path** — operator intent is always
explicit.

### Defaults baked into config parsing

| Field | Default |
| --- | --- |
| `router.profile` | `"balanced"` |
| `logging.filter` | `"info"` |
| `logging.format` | `"text"` |
| `limits.max_tasks` | `4_096` |
| `limits.max_buffered_bytes` | `67_108_864` (64 MiB) |
| `netdb.enabled` | `true` |
| `netdb.max_records` | `4_096` |
| `netdb.max_encoded_bytes` | `4 MiB` |
| `netdb.min_router_infos` | `50` |
| `netdb.min_floodfill_advertisers` | `5` |
| `reseed.enabled` | `false` |
| `reseed.max_sources` | `4` |
| `reseed.max_su3_bytes` | `8 MiB` |

### Stable exit codes

| Code | Name | When |
| --- | --- | --- |
| 0 | `Success` | Command completed. |
| 10 | `ConfigUnavailable` | Config file could not be read. |
| 11 | `ConfigParse` | Invalid TOML or unsupported schema. |
| 12 | `ConfigSemantic` | Syntactically valid but semantically invalid. |
| 20 | `RuntimeNotImplemented` | `run` without `--dry-run`. |
| 30 | `IdentityStorage` | Identity persistence failure. |
| 31 | `IdentityCrypto` | Identity generation failure. |
| 40 | `RuntimeBindFailed` | TCP listener could not bind. |
| 41 | `RuntimeIdentity` | Router identity not found/invalid. |
| 42 | `RuntimeListenerFailed` | Listener accept loop failed. |
| 43 | `RuntimeDialFailed` | Outbound connection failed. |
| 44 | `RuntimeHandshakeFailed` | NTCP2 Noise handshake failed. |
| 45 | `RuntimeShutdownTimeout` | Supervised shutdown exceeded deadline. |
| 46 | `RuntimeSupervisorFailed` | Child task crashed and supervisor terminated. |
| 47 | `RuntimeBootstrap` | Bootstrap pipeline failed. |
| 70 | `Internal` | Unexpected internal failure. |

`clap`'s own usage errors produce exit code **2**.

## Composition step

`execute(cli: Cli) -> Result<CommandOutcome, DaemonError>` is the
pure dispatch hub (`lib.rs`):

```
execute(cli: Cli) -> Result<CommandOutcome, DaemonError>
│
├─ Command::CheckConfig
│    ├─ Config::load(path)
│    └─ return Validated { config }    // no side effects
│
├─ Command::Identity::Generate
│    ├─ Config::load(path)
│    ├─ IdentityStore::prepare_directory(data_dir)
│    ├─ IdentityStore::in_data_dir(data_dir)
│    ├─ OsRng                          // from i2pr-crypto
│    ├─ RouterIdentityBundle::generate(&mut rng)
│    └─ store.save_new(&bundle)        // atomic write
│
├─ Command::Identity::Inspect
│    ├─ Config::load(path)
│    ├─ IdentityStore::in_data_dir(data_dir)
│    ├─ store.load()
│    └─ return IdentityInspected { path, summary }   // no secrets
│
└─ Command::Run
     ├─ Config::load(path)
     ├─ if dry_run → return Validated { config }
     └─ return RunReady { config }     // run_daemon called by main
```

`run_daemon(config)` (`lib.rs`) is the real daemon path:

1. Compute current wall-clock seconds.
2. Run `bootstrap_daemon(&config, now_seconds, None)`:
   a. load persistent router identity from `data_dir`;
   b. construct the NetDB store and bootstrap state;
   c. revalidate the persistent RouterInfo cache through
      `CacheLoader::load_into`;
   d. construct and self-validate the local RouterInfo;
   e. recompute bootstrap readiness;
   f. if reseed is enabled and the cache is below threshold, run
      the bounded offline reseed pipeline;
   g. return a sanitized `BootstrapReport`.
3. Build the service graph via `build_daemon_graph(&config)`.
4. Create the `Supervisor` with the graph.
5. Register a ctrl-c handler that triggers graceful shutdown.
6. Run the supervisor loop until shutdown or failure.

NTCP2 is excluded from the service graph under current authority.
The graph contains a `lifecycle` service that waits for the
shutdown signal and a `netdb-bootstrap` service that observes the
supervisor's cancellation token.

`bootstrap_daemon(&config, now_seconds, offline_reseed_path)` is
the Plan 106 synchronous pipeline. The function returns both the
sanitized `BootstrapReport` and an `Arc<Mutex<Bootstrap>>` so future
long-lived runtime adapters can observe the in-memory store
without re-running any pipeline stage.

`build_daemon_graph(&Config)` (`lib.rs`) is the testable seam
that builds the service graph without running the supervisor.
It rejects `ntcp2.enabled = true` as a defense-in-depth check.

`main()` (`main.rs`) is the outermost shell:

1. `Cli::parse()`.
2. `i2pr_daemon::execute(cli)`.
3. On `Validated`: `initialize_logging(&config.logging)`,
   print success.
4. On `RunReady`: `initialize_logging`, `run_daemon(config)`,
   print result.
5. On `IdentityGenerated` / `IdentityInspected`: print the result.
6. On `Err`: print to stderr, exit with the mapped code.

`initialize_logging()` (`lib.rs`) builds a
`tracing_subscriber::EnvFilter` from the config filter string and
calls `try_init()` — duplicate init is silently ignored for test
embedding.

### Plan 106 bootstrap state machine (`src/bootstrap.rs`)

`bootstrap_daemon` owns the Plan 106 bounded startup/readiness
pipeline. It composes the Plan 103/104/105 surfaces
(`RouterInfoStore`, `LocalRouterInfoBuilder`,
`ReseedSignerTrustSet`) without owning a runtime, sockets, or
tunnels.

`BootstrapState` is a bounded seven-variant enum
(`Empty`, `CacheSufficient`, `ReseedRequired`, `Reseeding`,
`ReadyForNetworkIntegration`, `DegradedInsufficientPeers`,
`Failed`). `BootstrapPolicy::from_config` derives the readiness
thresholds from the validated `Config`.

The pipeline never opens sockets, never performs DNS, never
contacts I2P peers, and never accepts plain HTTP bytes. The
HTTPS reseed adapter is deferred to a future plan; the offline
SU3 source path is the only allowlisted Plan 106 acquisition
path.

### Plan 106/107 runtime seam (`src/netdb_seam.rs`)

`NetDbSeam` exposes the Plan 105 lookup state machines behind a
stable runtime-facing surface. `path_status()` returns
`ExploratoryPathStatus::Available` when an injected
`i2pr_netdb::ReplyPathProvider` reports at least one valid inbound
tunnel, and `ExploratoryPathStatus::BlockedExploratoryTunnelUnavailable`
otherwise. `set_reply_path_provider` accepts any `Box<dyn
ReplyPathProvider>`; the production wiring is the
`i2pr_tunnel::ExploratoryPoolReplyPathProvider` adapter. A peer
transport link is not equivalent to a complete reply path.

### Plan 117 composition state machine (`src/netdb_seam.rs`)

Plan 117 §7.2–7.3 replaces the Plan 107/108 post-path placeholder
with the live composition root. After `accept_reply_path` succeeds,
`advance_after_path` drives the lookup state machine through
`RouterInfoLookup::handle_pending_after_path` and emits the next
typed `LookupAction::SendDatabaselookup` so the runtime adapter
never has to reach into private fields. The seam exposes the
bounded `CompositionOutcome` vocabulary
(`NeedInboundExploratory`, `NeedOutboundExploratory`,
`LookupReadyForTunnelDispatch`, `NoEligibleCandidates`) so the
runtime scheduler can request the right exploratory build at every
step. The
[`composition_outcome_with_registry`](../daemon/src/netdb_seam.rs)
helper derives the readiness outcome from the real
`DataPlaneRegistry` state at the supplied deterministic time; the
legacy caller-set readiness bits are deprecated.

### Plan 117 outbound composition (`src/outbound_lookup.rs`)

The outbound composition root wraps a typed `DatabaseLookupMessage`
or `DatabaseStoreMessage` in the standard I2NP envelope through
`i2pr-proto`, drives `OutboundGatewayRole::forward_cells` against a
`Router`-delivery `TunnelPayloadHeader`, and packages the resulting
`TunnelData` cells as complete short-transport I2NP messages
addressed to the outbound first hop. The Plan 117 corrective
closure ([`plans/117-corrective-closure.md`](../../plans/117-corrective-closure.md))
corrected two bugs: the ROUTER delivery target was the lookup key
rather than the selected floodfill peer, and the raw 1028-byte
`TunnelData` body was being placed directly in `EncodedI2npMessage`
instead of being wrapped in a complete short-transport I2NP
envelope. No hand-built STBM, no second I2NP codec, no direct
transport-to-floodfill fallback. The composition hard-ceilings are
`MAX_OUTBOUND_LOOKUP_CELLS` and `MAX_OUTBOUND_PUBLICATION_CELLS`.

### Plan 117 inbound dispatch (`src/inbound_dispatch.rs`)

The inbound dispatch helper routes one `TunnelDataMessage` by
`tunnel_id` to the activated `LocalInboundEndpointRole` in the
`i2pr_tunnel::DataPlaneRegistry`, decodes the recovered standard
I2NP envelope exactly once through the existing `i2pr-proto`
decoder, and supports only `DatabaseStore`, `DatabaseSearchReply`,
and `DeliveryStatus` body kinds. Unknown tunnel ids fail closed
without allocating role state. The recovered envelope ceiling is
`MAX_RECOVERED_ENVELOPE`. `route_databasestore` and
`route_database_search_reply` drive the Plan 105 ingestion
helpers.

### Plan 117 status

Plan 117 closed per Plan 118 as
`closed-for-progression-with-evidence-gap`. The all-i2pr Phase G
production-seam trajectory remains passed: it drives real
`EstablishedMaterial` through the canonical `TunnelEntry` /
`EstablishedTunnel` pool and exercises lookup success, wrong-target
rejection, `DatabaseSearchReply` iteration, and publication. The
historical Phase H Emissary parser result remains
`passed-emissary-wire-format-compatibility` at
`h_emissary_database_lookup_parsed` against pinned Emissary
revision `9b43484a21d5a1291c4881cdae62a36c527f8c0f`
(`emissary-core 0.4.0`).

The corrected native test now belongs to `emissary-core`'s own
`#[cfg(test)]` build and reaches native OBEP admission plus reply
AEAD opening, but strict i2pr reply Mapping decoding rejects the
pinned reference's request-prefixed reply plaintext. Native
publication, lookup, and inbound return evidence is not claimed;
the reference-side defect is localized to the pinned Emissary
revision, and no upstream correction is available. See
[`plans/117-status.md`](../../plans/117-status.md) and the Plan 118
disposition in

### Plan 122 LeaseSet2 lookup seam (`src/netdb_seam.rs`)

Plan 122 extends the daemon `NetDbSeam` with a dedicated LeaseSet2
lookup state machine and a separate reply-path provider so Plan 117
router-side exploration is not consulted for destination lookups. The
seam exposes `begin_lease_set2_lookup`, `advance_lease_set2_after_path`,
`ingest_lease_set2_response`, `ingest_lease_set2_store`,
`lease_set2_delivery_outcome`, `cancel_lease_set2_lookup`, and
`active_lease_set2_lookup`. The typed errors live in `NetDbSeamError`
and the typed ingestion results in `LeaseSet2ResponseOutcome`. Both
are re-exported from the daemon crate root (`lib.rs:18–20`). The
local Plan 122 deterministic composition reaches a `Complete` outcome
immediately when no floodfill candidate exists, surfacing the
typed terminal result rather than a stuck pending state.
[`plans/118-planning-authority-cleanup-and-plan117-disposition.md`](../../plans/118-planning-authority-cleanup-and-plan117-disposition.md).
Phase I authenticated transport remains
`deferred-host-lane-unavailable` on this host and is tracked
separately under the external acceptance debt ledger in
[`plans/118-123-milestone6-router-construction-roadmap.md`](../../plans/118-123-milestone6-router-construction-roadmap.md).

Plan 119 closed as `passed-leaseset2-protocol-foundation` per
[`plans/119-status.md`](../../plans/119-status.md); the ordinary
online-signed published Standard LeaseSet2 carrier is wired into
`i2pr-proto` and `i2pr-netdb`. Plan 120 closed as
`passed-destination-lifecycle-and-pools` and lands the first
`i2pr-client` destination runtime. Plan 121 closed as
`passed-ecies-destination-session-layer` and adds the ECIES-X25519-
AEAD-Ratchet destination session layer in `i2pr-client` (with the
primitive audit, wrapped primitives in `i2pr-crypto`, and the
bounded structural Garlic payload block codec in `i2pr-proto`).
Plan 122 closed as `passed-corrected-local-destination-routing` per
[`plans/122-status.md`](../../plans/122-status.md) and
[`plans/124-status.md`](../../plans/124-status.md); it composes the
Plan 119 LeaseSet2 lookup surface, the Plan 120 destination runtime,
the Plan 121 ECIES session layer, and the Plan 116 tunnel data plane
into the first complete local destination routing pipeline. Plan 124
closed as `passed-plan122-corrective-closure` and corrected the
Plan 122 composition defect where `compose_outbound_delivery`
retained an ECIES Garlic envelope but fed the plaintext inner I2NP
`Data` envelope into the outbound tunnel role. The corrected
composition wraps the encrypted envelope in an `I2npBody::Garlic`
carrier and feeds the standard-encoded I2NP Garlic message bytes
into the outbound tunnel data plane; `OutboundDeliveryPlan` exposes
`garlic_i2np_bytes: Vec<u8>` as the canonical carrier. Milestone 6
subsequently closed through the Plans 126–130 corrective sequence;
Plan 129's integrated gate is `superseded-by-plan130-final-gate` and
Plan 130 closed as
`passed-milestone6-final-wire-runtime-corrective-closure`
([`plans/130-status.md`](../../plans/130-status.md)); the next product
layer is SAM baseline planning (Milestone 7). Plan 137 closed
the SAM 3.1 loopback server and session lifecycle as
`passed-m7-sam31-loopback-server-session-lifecycle`
([`plans/137-status.md`](../../plans/137-status.md)); Plan 138 closed
the SAM 3.1 STREAM CONNECT / ACCEPT transport bridge as
`passed-m7-sam31-stream-connect-accept-bridge`
 ([`plans/138-status.md`](../../plans/138-status.md)); Plan 139 closes
the loopback-only STREAM FORWARD and local NAMING LOOKUP hardening as
`passed-m7-sam31-forward-naming-hardening`
([`plans/139-status.md`](../../plans/139-status.md)); the next product
step is independent-client interoperability and Milestone 7 closure
(Plan 148, currently `blocked-external-client-build-failure` per
[`plans/148-status.md`](../../plans/148-status.md) — the pinned i2plib
and libsam3 sources are not present in the local cache and no
build/install lane exists for them).

### Which crates are wired in today

| Subsystem | Crate |
| --- | --- |
| Crypto (`OsRng`, `RouterIdentityBundle`) | `i2pr-crypto` |
| Storage (`IdentityStore`, `ByteCache`) | `i2pr-storage` |
| NetDB (`RouterInfoStore`, `ValidatedRouterInfo`, `LocalRouterInfoBuilder`, lookup state machines, `ReplyPathProvider`) | `i2pr-netdb` |
| NetDB composition (`CacheLoader`, `ReseedIngestor`) | `i2pr-netdb-persist` |
| Tunnel substrate (`ExploratoryPool`, `BuildRecordLayout`, `BuildCryptography` seam, `ExploratoryPoolReplyPathProvider`, `ShortBuildI2npBridge`, `DataPlaneRegistry`, `OutboundGatewayRole`, `LocalInboundEndpointRole`) | `i2pr-tunnel` |
| Runtime (supervisor, service graph, lifecycle) | `i2pr-runtime` |
| Wire codecs (I2NP envelopes, LeaseSet2 carriers, database-lookup/store messages) | `i2pr-proto` (used by `outbound_lookup.rs`, `inbound_dispatch.rs`, `netdb_seam.rs`, `bootstrap.rs`) |
| Transport contracts (`Deadline`, `DeliveryRequest`, `EncodedI2npMessage`, `PeerId`) | `i2pr-transport` (used by `outbound_lookup.rs`) |
| `i2pr-core` | declared (`Cargo.toml:13`); not referenced by daemon source today |

## Dependencies

| Dependency | Source | Actually used |
| --- | --- | --- |
| `clap` | workspace | Yes (CLI parsing) |
| `i2pr-crypto` | path | Yes (RNG + identity) |
| `i2pr-core` | path | Declared but not yet referenced by daemon source |
| `i2pr-proto` | path | Yes (I2NP envelopes in `outbound_lookup`, `inbound_dispatch`, `netdb_seam`, `bootstrap`) |
| `i2pr-runtime` | path | Yes (supervisor, service graph, `tokio::spawn`) |
| `i2pr-storage` | path | Yes (identity store) |
| `i2pr-transport` | path | Yes (delivery contracts in `outbound_lookup`) |
| `i2pr-netdb` | path | Yes (RouterInfo store + validation + lookup state machines) |
| `i2pr-netdb-persist` | path | Yes (`CacheLoader`, `ReseedIngestor`) |
| `i2pr-tunnel` | path | Yes (`ExploratoryPool`, bridge, data-plane registry, roles) |
| `i2pr-api` | path | Yes (SAM 3.1 parser, session registry, line reader, server state machine — Plans 136–137; STREAM CONNECT/ACCEPT plus FORWARD/naming outcomes and atomic inbound mode — Plans 138–139) |
| `i2pr-client` | path | Yes (`DestinationRegistry`, `DestinationRuntime`, per-destination `StreamingManager` pool — Plan 137; `StreamingDestinationAdapter`, `DestinationRouting`, `EciesSessionManager`, `DestinationOutboundRole` — Plan 138) |
| `rand_chacha` | 0.9 | Yes (deterministic RNG for the Plan 138 STREAM adapter seam) |
| `rand_core` | 0.9 | Yes (RNG injection in `outbound_lookup`) |
| `serde` | workspace | Yes (config deserialization) |
| `thiserror` | workspace | Yes (error derives) |
| `tokio` | workspace | Yes (`tokio::signal::ctrl_c`, `tokio::spawn`, `tokio::main`-equivalent `run_blocking` in `main.rs`) |
| `toml` | workspace | Yes (TOML parsing) |
| `tracing` | workspace | Yes (transitive via logging) |
| `tracing-subscriber` | workspace | Yes (`EnvFilter`, `try_init()`) |
| `tempfile` (dev) | workspace | For filesystem tests |

`i2pr-transport-ntcp2` is intentionally **not** a direct
dependency — it would flow through `i2pr-runtime` once the runtime
integration lands and NTCP2 is enabled in the service graph.

## Tests

Unit tests in `src/lib.rs` and `src/config.rs` include composition
regression tests (`daemon_graph_contains_no_ntcp2_transport_service`,
`daemon_graph_rejects_ntcp2_enabled_config`) and NTCP2 activation
safety tests (omitted section → disabled, explicit false accepted,
explicit true rejected).

Integration tests in `tests/cli.rs` invoke the compiled binary via
`Command::new(env!("CARGO_BIN_EXE_i2pr"))`:

| Test | Coverage |
| --- | --- |
| `help_and_version_are_available` | `--help` lists subcommands; `--version` prefix |
| `missing_config_maps_to_exit_code_ten` | Missing → 10 |
| `missing_required_argument_maps_to_usage_exit_code_two` | Missing `--config` → 2 |
| `malformed_and_unknown_config_are_rejected` | Malformed TOML → 11, unknown → 11, semantic → 12 |
| `dry_run_succeeds_and_live_run_is_not_implemented` | `--dry-run` ✓; live run → 41 (identity load) |
| `identity_lifecycle_is_explicit_and_inspection_redacts_private_material` | Generate → inspect, no secret text |
| `dry_run_does_not_create_identity_state` | `run --dry-run` does not create `data_dir` |

## Distinctive design choices

1. **NTCP2 is disabled and unenableable.** The default is `false`;
   explicit `enabled = true` is rejected during config validation
   with a stable semantic error.
2. **Composition graph excludes NTCP2.** `build_daemon_graph` never
   registers `ntcp2-transport`; a minimal `lifecycle` service owns
   the shutdown signal.
3. **No default config path.** Every command requires `--config`.
4. **`run` without `--dry-run` starts a real daemon.** Config is
   validated first, then the supervisor runs the service graph.
5. **`<data_dir>/router.identity` is the on-disk path.** Created
   by `identity generate`; never created by `run --dry-run`
   (verified by the integration test).
6. **`deny_unknown_fields` everywhere.** Every `Raw*` config struct
   rejects unknown keys. Extra keys are an error (exit code 11).
7. **Limits have hard safety caps** in `MAX_ALLOWED_*` constants
   (e.g. `max_tasks` ≤ 1 000 000; `max_buffered_bytes` ≤ 1 TiB).
8. **Logging uses `tracing-subscriber` with `EnvFilter`.** `try_init`
   means duplicate init is silently ignored for test embedding.
9. **`ExitCode` is `#[repr(u8)]`** with explicit numeric assignments,
   asserted by integration tests. Stable API for operators.
10. **Schema version is `==`, not `>=`.** `schema_version = 2` is
   `UnsupportedSchemaVersion` (code 11). Schema migration requires
   a binary update first.
10. **Profile is locked to `"balanced"`.** Any other profile is
    rejected (`config.rs:170-178`). A placeholder for future
    routing policies.
11. **No `#[tokio::main]`** — the binary is synchronous today.
12. **`_command_name` (`main.rs:47-54`) is `#[allow(dead_code)]`** —
    reserved for future logging/metrics.

## Cross-references

- [Overview](overview.md)
- [i2pr-storage](i2pr-storage.md) — primary consumer via
  `IdentityStore`.
- [i2pr-crypto](i2pr-crypto.md) — provides `OsRng` and
  `RouterIdentityBundle::generate`.
- [i2pr-runtime](i2pr-runtime.md) — future `run` driver.
- Plan-of-record: sequence of `m1-` plans and `m2-` plans; the
  composition root is implicit in the latest active milestone.
