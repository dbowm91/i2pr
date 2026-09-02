# i2pr

An experimental I2P router written in Rust. **Not production-ready.** Not suitable for anonymity, privacy, censorship resistance, or any security-sensitive workload. NTCP2 stays experimental and non-advertised.

## Status

The local Milestone 6 product (destinations, garlic, LeaseSet2, Streaming) remains closed under corrected local-correctness semantics via [**Plan 134**](plans/134-status.md). Independent-router interoperability is tracked as external acceptance debt.

Milestone 7 / SAM has two strong retained sub-results:

- [**Plan 146**](plans/146-status.md) passed bidirectional SAM 3.1 private-destination reference requalification against pinned Java I2P/i2pd behavior.
- [**Plan 147**](plans/147-status.md) landed the dedicated same-socket raw STREAM owner, TCP↔Streaming byte pump, actual Streaming `Established` wait, OS-CSPRNG runtime path, and supervised ACK/retransmit driver.

A post-Plan-148 audit found that Milestone 7 still cannot close: the canonical Plan 147 byte-product test manually installs SAM destination bridges, local peer LeaseSet2 routing, inbound-tunnel factories, and destination drivers after `SESSION CREATE`, while the production `SESSION CREATE` path does not self-compose those prerequisites. Plan 147 also deferred mandatory SILENT/backpressure/fault/lifecycle acceptance, and current `SILENT=true` success handling is not byte-correct. Plan 148 is therefore retained as a blocked audit rather than the executable closure plan.

The current corrective authority is [**Plan 145**](plans/145-status.md), with [**Plan 149**](plans/149-status.md) as the **next executable plan**. Plan 149 makes the SAM localhost product self-compose from protocol commands alone and closes the deferred raw-path acceptance matrix. [**Plan 150**](plans/150-m7-sam31-external-client-reproducible-final-closure.md) is blocked on Plan 149 and will run correctly pinned external clients plus final FORWARD/NAMING closure. `sam_independent_clients = 0-passed`; Milestone 7 remains open and Milestone 8 is not yet the next product layer.

The `[sam]` config section remains disabled by default and loopback-only when enabled.

For the full plan hierarchy, MVP roadmap, and what's implemented vs. not, see [**`plans/README.md`**](plans/README.md).

## Workspace

```text
crates/
  i2pr-proto/               Bounded wire codecs, typed errors, no I/O
  i2pr-crypto/              Protocol-specific cryptographic wrappers
  i2pr-storage/             Atomic persistence and migration support
  i2pr-core/                Shared contracts, lifecycle, budgets, health
  i2pr-transport/           Transport-neutral link management
  i2pr-transport-ntcp2/     NTCP2 protocol implementation (no I/O)
  i2pr-runtime/             Tokio-owned supervision, cancellation, I/O
  i2pr-netdb/               RouterInfo + LeaseSet2 validation, store, lookup, publication
  i2pr-netdb-persist/       Persistent cache + bounded SU3 reseed ingestion
  i2pr-tunnel/              Tunnel identity, exploratory pool, ECIES-X25519 short-build, runtime-neutral data plane
  i2pr-client/              Local destinations, ECIES-X25519-AEAD-Ratchet session layer, I2P Streaming
  i2pr-api/                 Application-protocol adapter (SAM 3.1): bounded parser, typed commands, private-destination codec
  i2pr-daemon/              CLI, configuration, composition, supervision
  i2pr-testkit/             Deterministic simulation and adversarial fixtures
tools/
  i2pr-interop/             Non-production interop launcher (test only)
```

The dependency direction is enforced by `scripts/check-dependency-direction.sh`. Architecture deep-dives live under [`docs/architecture/`](docs/architecture/); the index is [`docs/architecture/overview.md`](docs/architecture/overview.md).

## Build, test, lint

Requires Rust 1.95.0 (pinned via `rust-toolchain.toml`); MSRV is 1.88.

```text
cargo fmt --all --check
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
```

Focused seams and the constrained-host lane are documented in [`AGENTS.md`](AGENTS.md).

## OpenCode skills

Loadable skill bundles under [`.opencode/skills/`](.opencode/skills/) cover the routine development seam ([`i2pr-local-dev`](.opencode/skills/i2pr-local-dev/SKILL.md)), documentation navigation ([`i2pr-architecture`](.opencode/skills/i2pr-architecture/SKILL.md)), the closed NTCP2 interop lane ([`i2pr-ntcp2-interop`](.opencode/skills/i2pr-ntcp2-interop/SKILL.md)), the Plan 046 rootless sandbox ([`i2pr-rootless-sandbox`](.opencode/skills/i2pr-rootless-sandbox/SKILL.md)), and the Plan 048–051 Multipass recovery guest ([`i2pr-multipass-recovery`](.opencode/skills/i2pr-multipass-recovery/SKILL.md)). Load the matching skill before touching its surface.

## License

No license selected yet. Do not copy code from I2P+, i2pd, Emissary, or other routers until license compatibility is reviewed. Specifications and observed behavior may be used for clean-room implementation.
