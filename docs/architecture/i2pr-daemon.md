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

What it **does not** do yet:

- Open NTCP2 listeners (disabled under current authority).
- Run `Ntcp2RuntimeService` or register `ntcp2-transport`.
- Apply live configuration changes.
- Drive a live exploratory tunnel build (Plan 107 lands the
  substrate; the live build lands in Plan 108+).
- Accept HTTPS reseed at runtime (the offline source path is the
  only allowlisted Plan 106 acquisition path; HTTPS is deferred
  to a future plan).

## Module layout

Flat — six files at the crate root:

| File | Responsibility | Main items |
| --- | --- | --- |
| `src/main.rs` | Executable shell: parse CLI, dispatch through `execute()`, print results, map errors to stable exit codes | `main()` |
| `src/lib.rs` | Crate root: re-exports `cli`, `config`, `error`, `bootstrap`, `netdb_seam`; defines `execute()` (pure dispatch), `CommandOutcome`, `IdentitySummary`, `initialize_logging()`, `bootstrap_daemon()`, `build_daemon_graph()`, `run_daemon()` | `CommandOutcome`, `IdentitySummary`, `execute()`, `initialize_logging()`, `bootstrap_daemon()`, `build_daemon_graph()`, `run_daemon()` |
| `src/cli.rs` | CLI vocabulary — `clap` derives | `Cli`, `Command`, `IdentityCommand`, `CheckConfigArgs`, `IdentityArgs`, `RunArgs` |
| `src/config.rs` | Strict versioned TOML configuration (`serde(deny_unknown_fields)` everywhere) | `CURRENT_SCHEMA_VERSION`, `DEFAULT_MAX_TASKS`, `DEFAULT_MAX_BUFFERED_BYTES`, `RouterProfile`, `LogFormat`, `RouterConfig`, `LoggingConfig`, `LimitsConfig`, `NetDbConfig`, `ReseedConfig`, `ReseedSourceConfig`, `Config`, `ConfigError` |
| `src/error.rs` | Typed error hierarchy and stable exit-code mapping | `ExitCode`, `DaemonError` |
| `src/bootstrap.rs` | Plan 106 NetDB/bootstrap state machine | `BootstrapState`, `BootstrapPolicy`, `BootstrapSnapshot`, `BootstrapReport`, `ReseedAttemptSummary`, `Bootstrap`, `bootstrap_daemon`, `bootstrap_with_offline_reseed`, `build_trust_set`, `store_summary` |
| `src/netdb_seam.rs` | Plan 106 runtime-facing seam for Plan 105 actions | `NetDbSeam`, `ExploratoryPathStatus` |

There are no subdirectories.

## Public surface

### Crate root (`src/lib.rs`)
- `pub mod bootstrap;`
- `pub mod cli;`
- `pub mod config;`
- `pub mod error;`
- `pub mod netdb_seam;`
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

### Which crates are wired in today

| Subsystem | Crate |
| --- | --- |
| Crypto (`OsRng`, `RouterIdentityBundle`) | `i2pr-crypto` |
| Storage (`IdentityStore`, `ByteCache`) | `i2pr-storage` |
| NetDB (`RouterInfoStore`, `ValidatedRouterInfo`, `LocalRouterInfoBuilder`, lookup state machines, `ReplyPathProvider`) | `i2pr-netdb` |
| NetDB composition (`CacheLoader`, `ReseedIngestor`) | `i2pr-netdb-persist` |
| Tunnel substrate (`ExploratoryPool`, `BuildRecordLayout`, `BuildCryptography` seam, `ExploratoryPoolReplyPathProvider`) | `i2pr-tunnel` |
| Runtime (supervisor, service graph, lifecycle) | `i2pr-runtime` |
| Transport / NTCP2 / Proto / Core | declared, **not yet used in production daemon** |

## Dependencies

| Dependency | Source | Actually used |
| --- | --- | --- |
| `clap` | workspace | Yes (CLI parsing) |
| `i2pr-crypto` | path | Yes (RNG + identity) |
| `i2pr-core` | path | Yes (service classification) |
| `i2pr-proto` | path | **No** (declared for future integration) |
| `i2pr-runtime` | path | Yes (supervisor, service graph) |
| `i2pr-storage` | path | Yes (identity store) |
| `i2pr-transport` | path | **No** (declared for future integration) |
| `serde` | workspace | Yes (config deserialization) |
| `thiserror` | workspace | Yes (error derives) |
| `toml` | workspace | Yes (TOML parsing) |
| `tracing` | workspace | Yes (transitive via logging) |
| `tracing-subscriber` | workspace | Yes (`EnvFilter`, `try_init()`) |
| `tempfile` (dev) | — | For filesystem tests |

`i2pr-transport-ntcp2` is intentionally **not** a direct
dependency — it would flow through `i2pr-runtime` once the runtime
integration lands.

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
