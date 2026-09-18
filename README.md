# i2pr

An experimental I2P router written in Rust. **Not production-ready.** Not suitable for anonymity, privacy, censorship resistance, or any security-sensitive workload. NTCP2 stays experimental and non-advertised.

## Status

Experimental router. **Not production-ready**, with no anonymity or privacy
claim. Loopback-only by default: SAM, I2CP, and service tunnels stay disabled
by default and non-advertised when enabled; NTCP2 stays experimental and
non-advertised. No localhost result is router-to-router interoperability
evidence.

| Milestone | Scope | State |
| --- | --- | --- |
| M6 local product | Destinations, garlic, LeaseSet2, Streaming (loopback) | Closed |
| M7 SAM 3.1 | Localhost SAM product plus external-client evidence | Closed |
| M8 SSU2 v2 | Direct-session interop against exact-pinned i2pd over loopback UDP | Closed (bounded scope) |
| M9 I2CP | Loopback server product plus independent LeaseSet2 lifecycle | Closed |
| M10 service tunnels | Local product plus remote generic / HTTP / IRC application closure | Closed (hosted double-pass; docs normalization pending) |
| M6 mixed-router | i2pd first-family Streaming | Closed; Java second family open |

Interoperability beyond the rows above is not claimed.

## Workspace

```text
crates/
  i2pr-proto/               Bounded wire codecs, typed errors, no I/O
  i2pr-crypto/              Protocol-specific cryptographic wrappers
  i2pr-storage/             Atomic persistence and migration support
  i2pr-core/                Shared contracts, lifecycle, budgets, health
  i2pr-transport/           Transport-neutral link management
  i2pr-transport-ntcp2/     NTCP2 protocol implementation (no I/O)
  i2pr-transport-ssu2/      SSU2 v2 protocol (runtime-neutral), path validation/publication, peer-test/relay/introducers
  i2pr-runtime/             Tokio-owned supervision, cancellation, transport I/O
  i2pr-netdb/               RouterInfo + LeaseSet2 validation, store, lookup, publication
  i2pr-netdb-persist/       Persistent cache + bounded SU3 reseed ingestion
  i2pr-tunnel/              Tunnel identity, exploratory pool, ECIES-X25519 short-build, runtime-neutral data plane
  i2pr-client/              Destinations, ECIES-X25519-AEAD-Ratchet session layer, routing, I2P Streaming
  i2pr-api/                 Runtime-neutral application-protocol adapters (SAM 3.1 plus the M9 I2CP wire/profile foundation; no sockets)
  i2pr-service-tunnels/     Runtime-neutral M10 service-tunnel config/policy (no sockets; generic client/server tunnel composition lives in i2pr-daemon)
  i2pr-daemon/              CLI, configuration, composition, supervision, application listener ownership
  i2pr-testkit/             Deterministic simulation and adversarial fixtures
tools/
  i2pr-interop/             Non-production interop launcher (test only)
```

The dependency direction is enforced by `scripts/check-dependency-direction.sh`. Architecture deep-dives live under [`docs/architecture/`](docs/architecture/); the index is [`docs/architecture/overview.md`](docs/architecture/overview.md).

## Build, test, lint

Requires Rust 1.95.0 (pinned via `rust-toolchain.toml`); MSRV is 1.88.

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
```

Focused seams and the constrained-host lane are documented in [`AGENTS.md`](AGENTS.md).

## OpenCode skills

Loadable skill bundles under [`.opencode/skills/`](.opencode/skills/) cover the routine development seam ([`i2pr-local-dev`](.opencode/skills/i2pr-local-dev/SKILL.md)), documentation navigation ([`i2pr-architecture`](.opencode/skills/i2pr-architecture/SKILL.md)), planning register/close mechanics ([`i2pr-planning`](.opencode/skills/i2pr-planning/SKILL.md)), the closed NTCP2 interop lane ([`i2pr-ntcp2-interop`](.opencode/skills/i2pr-ntcp2-interop/SKILL.md)), the historical rootless sandbox ([`i2pr-rootless-sandbox`](.opencode/skills/i2pr-rootless-sandbox/SKILL.md)), and the historical Multipass recovery guest ([`i2pr-multipass-recovery`](.opencode/skills/i2pr-multipass-recovery/SKILL.md)). Load the matching skill before touching its surface.

## License

No license selected yet. Do not copy code from I2P+, i2pd, Emissary, or other routers until license compatibility is reviewed. Specifications and observed behavior may be used for clean-room implementation.
