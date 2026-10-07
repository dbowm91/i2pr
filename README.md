# i2pr

An experimental I2P router written in Rust. **Not production-ready.** It makes no
anonymity, privacy, or censorship-resistance claim and is not suitable for any
security-sensitive workload.

Loopback-only by default. SAM, I2CP, I2PControl, and service tunnels are
disabled by default and non-advertised when enabled. NTCP2 is experimental and
non-advertised. A result on `127.0.0.1` is **not** router-to-router
interoperability evidence.

## Quickstart

Requires Rust 1.95.0 (pinned by `rust-toolchain.toml`).

### 1. Build

```sh
cargo build --locked -p i2pr-daemon --bin i2pr
./target/debug/i2pr --help
```

The executable is named `i2pr` (the crate is `i2pr-daemon`). Subcommands:
`check-config`, `identity generate`, `identity inspect`, `run`. Examples below
write it as `i2pr`; substitute `./target/debug/i2pr` if you have not put it on
`PATH`.

### 2. Write a config

`schema_version` and `router.data_dir` are the only required fields; everything
else has a default. Loopback-only and disabled-by-default is the default posture,
so this validates as-is:

```toml
schema_version = 1

[router]
data_dir = "/absolute/path/to/state"
```

Two things that are easy to get wrong:

- **`data_dir` resolves against the process working directory, not the location of
  the config file.** Use an absolute path.
- **The directory must be mode `0700`.** Identity storage refuses to run in a
  group- or world-accessible directory. `/tmp` is `0777`, so a scratch directory
  under `/tmp` must be `chmod 700` first.

### 3. Create and inspect a router identity

```sh
chmod 700 /path/to/state
i2pr check-config    --config config.toml
i2pr identity generate --config config.toml
i2pr identity inspect  --config config.toml
```

`identity inspect` reports the algorithm types and never prints private material.
Expected output:

```text
configuration is valid; no network or persistent state was touched
router identity generated and stored at /path/to/state/router.identity
router identity is valid at /path/to/state/router.identity; signing algorithm type 7, encryption algorithm type 4; private material was not displayed
```

### 4. Validate without touching the network

```sh
i2pr run --dry-run --config config.toml
```

```text
configuration is valid; dry run complete (no network or persistent state was touched)
```

### 5. Talk SAM 3.1 over loopback

Since **Plan 360**, `i2pr run` reaches a serving state and binds its configured
loopback listeners; before that it exited `ReadinessTimeout` with nothing bound
(see *Running the router* below). To get a SAM 3.1 listener without writing a
config, use the harness example, which binds an ephemeral loopback port and
prints it as JSON:

```sh
cargo run --locked -p i2pr-daemon --example sam_loopback_listener -- --port 0
```

```text
{"port":61008,"pid":17897}
```

Then, with that port:

```sh
printf 'HELLO VERSION MIN=3.0 MAX=3.1\nSESSION CREATE STYLE=STREAM ID=demo DESTINATION=TRANSIENT\nNAMING LOOKUP NAME=ME\n' \
  | nc 127.0.0.1 61008
```

`DESTINATION=TRANSIENT` tells the router to self-compose a fresh destination, so
no prior key material is needed. Verified replies (the destination is truncated
here; the real value runs to several hundred base64 characters):

```text
HELLO REPLY RESULT=OK VERSION=3.1
SESSION STATUS RESULT=OK DESTINATION=7E5IGOAPci23ST0Z9zgztSgRVCa6eycYZwv...
NAMING REPLY RESULT=OK VALUE=7E5IGOAPci23ST0Z9zgztSgRVCa6eycYZwv...
```

Notes for the SAM surface: the only accepted session style is `STREAM` —
`STREAMING`, `DATAGRAM`, and `RAW` are rejected as unsupported; `SESSION CREATE`
requires `DESTINATION=`; and `NAMING LOOKUP NAME=ME` is only valid inside a
session.

## Router console (experimental)

Plans 356–358 added a loopback browser console: a self-contained HTML/CSS/JS
shell with bundled themes, served by EggServe with an Axum router. It is
**experimental, loopback-only, disabled by default, and non-advertised**, and it
is not listed in `specs/support.toml`.

Its security posture is the point, not the pixels:

- `[console]` refuses any non-loopback bind at config-parse time.
- Every request must match an exact `Host` authority (`localhost:<port>`,
  `127.0.0.1:<port>`, `[::1]:<port>`); anything else is `403`.
- Optional Argon2id authentication with a bounded session store, CSRF tokens,
  and a console-wide login throttle.
- The overview is **read-only**: it renders `RouterInfo` and `ClientServices`
  through an in-process Proposal 170 dispatcher with a closed allow-set. It
  never reaches `TunnelManager` or `AddressBook`, and needs no I2PControl
  listener, password, or token.
- `i2pr-console` owns no socket and has zero workspace dependencies, so it
  cannot reach router state except through the `ControlClient` trait.

Since **Plan 360** the console is no longer blocked by a router that refuses
to serve: `i2pr run` binds its configured loopback listeners, and an enabled
`[console]` section is registered into that composition. It stays **disabled by
default**, and the end-to-end console behaviour is still exercised by driving
`ConsoleServiceState` directly in
`cargo test -p i2pr-daemon --test console_loopback -- --test-threads=1`.
`check-config` validates the `[console]` section.

## Managed application runtime (experimental)

Plan 369 added a managed-application runtime: the router can supervise a
separate **manager** process, which in turn starts applications through a
direct-exec host. It is **disabled by default**, and it currently launches
nothing.

```toml
[app_runtime]
enabled = true   # default: false
```

- The manager (`i2pr-appd`) is the router's own binary-directory **sibling**.
  There is no configuration value, environment variable, or argument that can
  select a different executable — the block is `deny_unknown_fields`, so a
  `manager_path` key is a hard parse error rather than a silently ignored
  setting.
- The manager is spoken to over **two inherited anonymous pipes**. There is no
  listener, no port, and no discovery endpoint, and it cannot be run standalone
  and have it mean anything.
- **Enabling it launches nothing**, and that is structural rather than pending:
  the shipped manager owns an empty launch catalog, and the private manager
  protocol has no manager-receivable launch request. Turning it on buys a
  supervised, bounded process and its health signal.
- `LaunchProfile::Secured` is **refused before any exec**. No qualified OS
  sandbox backend exists, so there is no containment claim of any kind.
- A broken manager degrades the app runtime and nothing else: the router stays
  up and reports ready.

Plan 369 WP5 qualified the whole chain black-box — daemon → manager → host →
fixture application → real SAM and I2CP — with no loopback listener. That run
found that the manager link never flushed its frames, which had made every
launch fail; see
[`plans/closure/managed-native-app-runtime/369-status.md`](plans/closure/managed-native-app-runtime/369-status.md).
Managed-app v1 remains unreleased and is not advertised anywhere.

## Running the router

`i2pr run --config <cfg>` starts the router, binds its configured loopback
listeners, and stays up until signalled. **This did not work before Plan 360**:
the run used to exit `ReadinessTimeout` with no listener bound, because the
Essential `lifecycle` service awaited cancellation and never signalled initial
readiness. See `plans/closure/workspace-foundation/360-status.md`.

`check-config`, `identity generate|inspect`, and `run --dry-run` all work. The
`sam_loopback_listener` example above remains the quickest way to get a live SAM
listener without writing a config.

Exit codes: `0` success, `10` config unreadable, `11` bad TOML/schema, `12` config
semantically invalid, `20` capability not in this milestone, `30` identity
storage, `46` supervisor terminated. Full set in
`crates/i2pr-daemon/src/error.rs`.

## Documentation

| Topic | Document |
| --- | --- |
| What is implemented vs. only planned | [`docs/protocol-support.md`](docs/protocol-support.md) |
| Crate boundaries, ownership, data flow | [`docs/architecture.md`](docs/architecture.md) |
| Per-crate deep dives | [`docs/architecture/`](docs/architecture/) |
| Security boundaries, threat model | [`docs/security-model.md`](docs/security-model.md) |
| Scripts, fixtures, evidence lanes, CI | [`docs/architecture/tooling.md`](docs/architecture/tooling.md) |
| Architecture decisions (ADR 0000–0035) | [`docs/adr/`](docs/adr/) |
| Controlled testnet boundary | [`docs/private-testnet.md`](docs/private-testnet.md) |
| Contributing conventions | [`CONTRIBUTING.md`](CONTRIBUTING.md) |
| Non-negotiable guardrails | [`GUARDRAILS.md`](GUARDRAILS.md) |
| Full build/test/lint floor, agent skills | [`AGENTS.md`](AGENTS.md) |

## Repository layout

21 crates: 20 production plus `i2pr-testkit` (deterministic fixtures, which no
production crate may depend on), plus the non-production `tools/i2pr-interop`
launcher. `i2pr-runtime` is the only production owner of Tokio, sockets, timers,
and channels; `i2pr-api`, `i2pr-i2pcontrol`, `i2pr-addressbook`, `i2pr-console`,
and `i2pr-service-tunnels` are runtime-neutral by construction — `i2pr-console`
also owns no socket at all, so the daemon hosts its loopback listener. Crate
index and enforced dependency allowlist: [`docs/architecture/`](docs/architecture/).

## Development

```sh
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test  --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
```

Use `--test-threads=1` for the `i2pr-daemon` and `i2pr-runtime` loopback suites.
The full routine floor — every evidence-integrity checker, plus the known checker
coverage gaps — is in [`AGENTS.md`](AGENTS.md). Plan-of-record:
[`plans/registry.md`](plans/registry.md), with closure records in
`plans/closure/`.

## License

No repository-wide license has been selected yet. Do not copy code from external
router implementations unless provenance and compatibility have been reviewed
and explicitly authorized. A narrow, project-owned exception is recorded in
[ADR 0028](docs/adr/0028-i2pcontrol-proposal-170-control-plane.md) for the
Proposal 170 / I2PControl work in `eggstack/emissary`; it does not cover
unrelated Emissary or upstream code, I2P+, i2pd, or Java I2P. Specifications and
observed behavior remain valid clean-room sources.
