# `i2pr-daemon` — Deep Dive

Composition root and CLI entrypoint. Glues together the other
workspace crates into the `i2pr` binary. Deliberately minimal and
non-networked at this milestone: it validates configuration and
manages identity files but refuses to open listeners or run a runtime.

Path: `crates/i2pr-daemon/`

Binary: `i2pr` (declared via `[[bin]]` in `Cargo.toml`).

## Purpose

`i2pr-daemon` is the top of the dependency graph — it sees every
crate that will eventually participate in the running daemon. Today
its work is deliberately scoped to:

- **CLI parsing** via `clap` derives: subcommands, flags, `--help`.
- **Configuration** parsing, validation, normalization, schema
  version checking.
- **Identity lifecycle**: explicit generation and inspection of
  `<data_dir>/router.identity`. No auto-generated side effects on
  `run --dry-run`.
- **Stable process exit codes** that operators and automation can
  rely on.

What it **does not** do yet:

- Open listeners, reseed, or start a runtime service graph.
- Run `Ntcp2RuntimeService` or `Supervisor`.
- Apply live configuration changes.

`run` without `--dry-run` returns `RuntimeNotImplemented` (exit
code 20) **after** config validation succeeds — a deliberate
"validate first, then refuse" pattern.

## Module layout

Flat — four files at the crate root:

| File | Responsibility | Main items |
| --- | --- | --- |
| `src/main.rs` | Executable shell: parse CLI, dispatch through `execute()`, print results, map errors to stable exit codes | `main()` |
| `src/lib.rs` | Crate root: re-exports `cli`, `config`, `error`; defines `execute()` (pure dispatch), `CommandOutcome`, `IdentitySummary`, `initialize_logging()` | `CommandOutcome`, `IdentitySummary`, `execute()`, `initialize_logging()` |
| `src/cli.rs` | CLI vocabulary — `clap` derives | `Cli`, `Command`, `IdentityCommand`, `CheckConfigArgs`, `IdentityArgs`, `RunArgs` |
| `src/config.rs` | Strict versioned TOML configuration (`serde(deny_unknown_fields)` everywhere) | `CURRENT_SCHEMA_VERSION`, `DEFAULT_MAX_TASKS`, `DEFAULT_MAX_BUFFERED_BYTES`, `RouterProfile`, `LogFormat`, `RouterConfig`, `LoggingConfig`, `LimitsConfig`, `Config`, `ConfigError` |
| `src/error.rs` | Typed error hierarchy and stable exit-code mapping | `ExitCode`, `DaemonError` |

There are no subdirectories.

## Public surface

### Crate root (`src/lib.rs`)
- `pub mod cli;` (`lib.rs:9`)
- `pub mod config;` (`lib.rs:10`)
- `pub mod error;` (`lib.rs:11`)
- `enum CommandOutcome` (`lib.rs:22`):
  - `Validated { dry_run, config }` (`lib.rs:24`)
  - `IdentityGenerated { path }` (`lib.rs:31`)
  - `IdentityInspected { path, summary }` (`lib.rs:36`)
- `struct IdentitySummary { signing_algorithm, encryption_algorithm }`
  (`lib.rs:46-51`).
- `fn execute(Cli) -> Result<CommandOutcome, DaemonError>`
  (`lib.rs:54`).
- `fn initialize_logging(&LoggingConfig)` (`lib.rs:101`).

### `src/cli.rs`
- `struct Cli` (`Parser`, `cli.rs:14`).
- `enum Command` (`Subcommand`, `cli.rs:22`).
- `enum IdentityCommand` (`Subcommand`, `cli.rs:37`).
- `struct CheckConfigArgs { config: PathBuf }` (`cli.rs:46-49`).
- `struct IdentityArgs { config: PathBuf }` (`cli.rs:54-57`).
- `struct RunArgs { config: PathBuf, dry_run: bool }` (`cli.rs:62-68`).

### `src/config.rs`
- `const CURRENT_SCHEMA_VERSION: u64 = 1` (`config.rs:11`).
- `const DEFAULT_MAX_TASKS: u64 = 4_096` (`config.rs:13`).
- `const DEFAULT_MAX_BUFFERED_BYTES: u64 = 67_108_864` (64 MiB)
  (`config.rs:15`).
- `enum RouterProfile` (`config.rs:96`).
- `enum LogFormat` (`config.rs:102`).
- `struct RouterConfig` (`config.rs:110`).
- `struct LoggingConfig` (`config.rs:118`).
- `struct LimitsConfig` (`config.rs:128`).
- `struct Config` (`config.rs:137`).
- `impl Config { fn load(&Path) -> Result<Self, DaemonError>; fn parse(&str) -> Result<Self, ConfigError> }`
  (`config.rs:150, 161`).
- `enum ConfigError` (`config.rs:262`) with `fn exit_code() -> ExitCode`
  (`config.rs:281`).

### `src/error.rs`
- `enum ExitCode #[repr(u8)]` (`error.rs:15`); `fn as_i32() -> i32`
  (`error.rs:36`).
- `enum DaemonError` (`error.rs:43`) with `fn exit_code() -> ExitCode`
  (`error.rs:71`).

## CLI surface

```
i2pr [--version] [--help]
```

About string (`cli.rs:12`): *"Experimental I2P router workspace
(live daemon execution not enabled)."*

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
| 70 | `Internal` | Unexpected internal failure. |

`clap`'s own usage errors produce exit code **2**.

## Composition step

`execute(cli: Cli) -> Result<CommandOutcome, DaemonError>` is the
pure dispatch hub (`lib.rs:54-95`):

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
     ├─ if !dry_run → Err(RuntimeNotImplemented)
     └─ return Validated { config }
```

`main()` (`main.rs:9-41`) is the outermost shell:

1. `Cli::parse()`.
2. `i2pr_daemon::execute(cli)`.
3. On `Validated`: `initialize_logging(&config.logging)`,
   print success.
4. On `IdentityGenerated` / `IdentityInspected`: print the result.
5. On `Err`: print to stderr, exit with the mapped code.

`initialize_logging()` (`lib.rs:101-104`) builds a
`tracing_subscriber::EnvFilter` from the config filter string and
calls `try_init()` — duplicate init is silently ignored for test
embedding.

### Which crates are wired in today

| Subsystem | Crate |
| --- | --- |
| Crypto (`OsRng`, `RouterIdentityBundle`) | `i2pr-crypto` |
| Storage (`IdentityStore`) | `i2pr-storage` |
| Transport / NTCP2 / Runtime / Proto / Core | declared, **not yet used** |

## Dependencies

| Dependency | Source | Actually used |
| --- | --- | --- |
| `clap` | workspace | Yes (CLI parsing) |
| `i2pr-crypto` | path | Yes (RNG + identity) |
| `i2pr-core` | path | **No** (declared for future integration) |
| `i2pr-proto` | path | **No** (declared for future integration) |
| `i2pr-runtime` | path | **No** (declared for future integration) |
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

Unit tests in `src/lib.rs:107-202` and `src/config.rs:292-372`. Six
`#[test]` functions across files.

Integration tests in `tests/cli.rs:1-163` invoke the compiled
binary via `Command::new(env!("CARGO_BIN_EXE_i2pr"))`:

| Test | Coverage |
| --- | --- |
| `help_and_version_are_available` | `--help` lists subcommands; `--version` prefix |
| `missing_config_maps_to_exit_code_ten` | Missing → 10 |
| `missing_required_argument_maps_to_usage_exit_code_two` | Missing `--config` → 2 |
| `malformed_and_unknown_config_are_rejected` | Malformed TOML → 11, unknown → 11, semantic → 12 |
| `dry_run_succeeds_and_live_run_is_not_implemented` | `--dry-run` ✓; live run → 20 |
| `identity_lifecycle_is_explicit_and_inspection_redacts_private_material` | Generate → inspect, no secret text |
| `dry_run_does_not_create_identity_state` | `run --dry-run` does not create `data_dir` |

## Distinctive design choices

1. **Five declared-but-unused deps.** `i2pr-core`, `i2pr-proto`,
   `i2pr-runtime`, `i2pr-transport` are reserved for the eventual
   runtime integration. This is a "seat reservation" pattern.
2. **No default config path.** Every command requires `--config`.
3. **`run` without `--dry-run` is a hard error after config
   validation.** Config is always validated even on the error path.
4. **`<data_dir>/router.identity` is the on-disk path.** Created
   by `identity generate`; never created by `run --dry-run`
   (verified by the integration test).
5. **`deny_unknown_fields` everywhere.** Every `Raw*` config struct
   rejects unknown keys. Extra keys are an error (exit code 11).
6. **Limits have hard safety caps** in `MAX_ALLOWED_*` constants
   (e.g. `max_tasks` ≤ 1 000 000; `max_buffered_bytes` ≤ 1 TiB).
7. **Logging uses `tracing-subscriber` with `EnvFilter`.** `try_init`
   means duplicate init is silently ignored for test embedding.
8. **`ExitCode` is `#[repr(u8)]`** with explicit numeric assignments,
   asserted by integration tests. Stable API for operators.
9. **Schema version is `==`, not `≥`.** `schema_version = 2` is
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
