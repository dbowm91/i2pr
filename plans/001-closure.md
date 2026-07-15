# Milestone 0 closure record

## Scope and outcome

This record closes the corrective pass in
`plans/002-milestone-0-corrective-closure.md`. The pass remains bounded to
repository foundation work. It adds no Milestone 1 protocol behavior and does
not change the four-crate bootstrap shape.

## Final workspace and documentation

The existing crates remain:

- `i2pr-proto` — protocol namespace vocabulary only;
- `i2pr-core` — runtime-neutral lifecycle, health, cancellation, and resource
  contracts;
- `i2pr-daemon` — non-networked CLI and future composition root;
- `i2pr-testkit` — deterministic test foundations.

The corrective pass changed or added:

- `.github/workflows/ci.yml` — pinned normal-toolchain quality jobs on Linux
  and macOS, dependency policy, and an Ubuntu Rust 1.85 MSRV check;
- `AGENTS.md` and `README.md` — closure, conformance, ledger, and toolchain
  guidance;
- `docs/architecture.md` — cancellation scope and async-wake limitation;
- `plans/001-closure.md` — this closure record;
- `docs/protocol-support.md`, `specs/CONFORMANCE.md`, `specs/README.md`, and
  `specs/support.toml` — the human-readable matrix, machine-readable support
  inventory, and schema/status policy. The ledger contains 12 Milestone 1
  surfaces.

No new Cargo dependencies or crates were added. The proposed initial crate
structure was retained; later transport, NetDB, tunnel, client, API, storage,
and service crates remain deferred to detailed plans.

## Toolchain and CI policy

Rust `1.85` remains the declared MSRV in the workspace manifest. Rust `1.95.0`
remains the pinned normal development and CI toolchain. The workflow checks
`cargo check --workspace --all-targets` under Rust `1.85.0` on Ubuntu and runs
formatting, checks, tests, Clippy, rustdoc, and dependency-direction checks on
Ubuntu and macOS using Rust `1.95.0`.

The repository is private and the workflow is configured for both pushes and
pull requests. The authoritative remote evidence for this closure is the
[Milestone 0 CI workflow](https://github.com/dbowm91/i2pr/actions/workflows/ci.yml)
and the successful run associated with the final `main` commit.

The Rust 1.85 toolchain was not installed in the local execution environment,
so the MSRV command was not run locally. It is nevertheless continuously
checked by the dedicated remote job; no dependency was weakened or check
removed to accommodate that environment limitation.

## Local validation

All commands below passed on the pinned Rust `1.95.0` toolchain:

| Command | Result |
| --- | --- |
| `rtk cargo fmt --all --check` | passed |
| `rtk cargo check --workspace` | passed |
| `rtk cargo check --workspace --all-targets` | passed |
| `rtk cargo test --workspace` | 23 tests passed |
| `rtk cargo clippy --workspace --all-targets --all-features -- -D warnings` | passed |
| `RUSTDOCFLAGS="-D warnings" rtk cargo doc --workspace --no-deps` | passed |
| `rtk bash scripts/check-dependency-direction.sh` | passed |
| `rtk cargo deny check advisories bans sources` | passed |

`git diff --check` also passed. No network listener, router identity, data
directory, reseed operation, or persistent router state was introduced.

## Security and support decisions

- Configuration validation remains strict and side-effect-free.
- The atomic cancellation token is documented as a runtime-neutral bootstrap
  flag for cooperative polling; it does not provide async wake semantics or
  async wait/select operations. Runtime-specific cancellation belongs at
  runtime-facing service boundaries in a later plan.
- `specs/support.toml` records Milestone 1 structures and protocol surfaces as
  `not-implemented` with empty evidence and `advertised = false`. Namespace
  names, constants, and planning entries do not constitute support.
- No protocol, transport, RouterInfo, identity, NetDB, tunnel, client, or API
  capability is advertised.
- No project license was selected and no router implementation code or
  third-party router code was copied.

## Known limitations and Milestone 1 gate

The router runtime, all wire codecs, persistent identity, transports, NetDB,
tunnels, destinations, streaming, APIs, and service tunnels remain
unimplemented. Interoperability evidence does not yet exist. Milestone 1 may
begin only from its existing detailed plans, with the support ledger updated
before any implementation claim and with the MSRV and normal-toolchain checks
remaining green.
