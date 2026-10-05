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

`i2pr run` does **not** currently reach a serving state — see *Known limitation*
below. To exercise the real SAM 3.1 listener, use the harness example, which
binds an ephemeral loopback port and prints it as JSON:

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

## Known limitation: `i2pr run` fails startup

`i2pr run` (without `--dry-run`) starts, then shuts down after 30 seconds:

```text
error: supervisor terminated: supervisor failed: service lifecycle failed during startup: ReadinessTimeout
```

The Essential `lifecycle` service awaits cancellation and never signals initial
readiness, so the supervisor's readiness timeout fires before `sam-bridge` starts.
**No listener is ever opened and the router does not run.** `check-config`,
`identity generate|inspect`, and `run --dry-run` all work; use the
`sam_loopback_listener` example above for a live SAM listener.

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
| Architecture decisions (ADR 0000–0031) | [`docs/adr/`](docs/adr/) |
| Controlled testnet boundary | [`docs/private-testnet.md`](docs/private-testnet.md) |
| Contributing conventions | [`CONTRIBUTING.md`](CONTRIBUTING.md) |
| Non-negotiable guardrails | [`GUARDRAILS.md`](GUARDRAILS.md) |
| Full build/test/lint floor, agent skills | [`AGENTS.md`](AGENTS.md) |

## Repository layout

19 crates: 18 production plus `i2pr-testkit` (deterministic fixtures, which no
production crate may depend on), plus the non-production `tools/i2pr-interop`
launcher. `i2pr-runtime` is the only production owner of Tokio, sockets, timers,
and channels; `i2pr-api`, `i2pr-i2pcontrol`, `i2pr-addressbook`, and
`i2pr-service-tunnels` are runtime-neutral by construction. Crate index and
enforced dependency allowlist:
[`docs/architecture/`](docs/architecture/).

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
