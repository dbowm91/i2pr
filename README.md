# i2pr

An experimental I2P router written in Rust. **Not production-ready.** It makes no
anonymity, privacy, or censorship-resistance guarantees. Do not use it for
security-sensitive traffic.

Protocol support is incomplete. Local tests and scoped interoperability results
do not establish general router compatibility. Experimental service and control
interfaces are disabled by default. NTCP2 remains experimental and
non-advertised.

## Quickstart

Requires Rust 1.95.0, pinned by `rust-toolchain.toml`.

### 1. Build

```sh
cargo build --locked -p i2pr-daemon --bin i2pr
./target/debug/i2pr --help
```

The executable is `i2pr`; the crate is `i2pr-daemon`.

### 2. Configure local state

Create `config.toml` with the minimum required settings:

```toml
schema_version = 1

[router]
data_dir = "/absolute/path/to/i2pr-state"
```

Replace the example with an absolute path you own. Relative `data_dir` values
resolve from the process working directory, not from the config file. Create the
directory and restrict its permissions before generating an identity:

```sh
mkdir -p /absolute/path/to/i2pr-state
chmod 700 /absolute/path/to/i2pr-state
```

### 3. Validate and create an identity

```sh
./target/debug/i2pr check-config --config config.toml
./target/debug/i2pr identity generate --config config.toml
./target/debug/i2pr identity inspect --config config.toml
```

Identity inspection reports public algorithm information and never prints
private material.

### 4. Dry-run or start

```sh
./target/debug/i2pr run --dry-run --config config.toml
./target/debug/i2pr run --config config.toml
```

The dry run does not touch the network or persistent state. A normal run stays
in the foreground until stopped. Optional listeners such as SAM, I2CP,
I2PControl, service tunnels, and the console are disabled by default.

## Test a local SAM connection

On macOS or Linux, the standalone example starts an ephemeral loopback SAM
listener without requiring a router config or identity:

```sh
cargo run --locked -p i2pr-daemon --example sam_loopback_listener -- --port 0
```

It prints the selected port as JSON. In another terminal, replace `61008` with
that port and send a SAM 3.1 test sequence:

```sh
printf 'HELLO VERSION MIN=3.0 MAX=3.1\nSESSION CREATE STYLE=STREAM ID=demo DESTINATION=TRANSIENT\nNAMING LOOKUP NAME=ME\n' | nc 127.0.0.1 61008
```

This harness example tests the local SAM command path. It skips normal router
startup and is not evidence of router-to-router communication. The router also
has an experimental SAM 3.3 PRIMARY/subsession profile. It is loopback-only,
disabled by default, and not advertised for public or remote-router use.

## Other experimental features

- The router console is read-only, loopback-only, disabled by default, and
  non-advertised.
- The managed application runtime is disabled by default. Applications require
  explicit offline trust and policy grants. `UnsafeDirect` has ordinary host
  networking without a sandbox. The `Secured` profile is limited to qualified
  Linux x86_64 and aarch64 hosts running static native ELF executables.

See the [protocol support summary](docs/protocol-support.md) for current evidence
and limitations. The [architecture overview](docs/architecture.md) and
[security model](docs/security-model.md) describe internal boundaries.

## Development

```sh
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
```

Use `--test-threads=1` for the `i2pr-daemon` and `i2pr-runtime` loopback suites.
The full routine floor and evidence checks are listed in [`AGENTS.md`](AGENTS.md).
See [`CONTRIBUTING.md`](CONTRIBUTING.md) before making changes.

## License

i2pr is licensed under the [MIT License](LICENSE). The project remains
clean-room, and its provenance rules still apply to reused material. See
[`GUARDRAILS.md`](GUARDRAILS.md).
