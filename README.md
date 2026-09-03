# i2pr

An experimental I2P router written in Rust. **Not production-ready.** Not suitable for anonymity, privacy, censorship resistance, or any security-sensitive workload. NTCP2 stays experimental and non-advertised.

## Status

The local Milestone 6 product (destinations, garlic, LeaseSet2, Streaming) remains closed under corrected local-correctness semantics via [**Plan 134**](plans/134-status.md). Independent-router interoperability is tracked as external acceptance debt.

Milestone 7 / SAM has three strong retained sub-results:

- [**Plan 146**](plans/146-status.md) passed bidirectional SAM 3.1 private-destination reference requalification against pinned Java I2P/i2pd behavior.
- [**Plan 147**](plans/147-status.md) landed the dedicated same-socket raw STREAM owner, TCP↔Streaming byte pump, actual Streaming `Established` wait, OS-CSPRNG runtime path, and supervised ACK/retransmit driver.
- [**Plan 149**](plans/149-status.md) makes `SESSION CREATE` self-compose the entire localhost STREAM product from SAM protocol commands alone. The destination identity is shared via one `Arc<DestinationIdentity>` allocation between the destination runtime and the SAM bridge; the OS-CSPRNG-driven `SamLocalProductFabric` provides a signed LeaseSet2, outbound role, and inbound-tunnel factory; the per-destination runtime driver is spawned automatically; local peer LeaseSet2 routing is resolved through the SAM service directory (no test injection); spec-correct private `SESSION STATUS DESTINATION=` and public `NAMING LOOKUP NAME=ME` are implemented; byte-exact `STREAM STATUS RESULT=OK` and `DESTINATION=<peer-pub-b64>` raw-transition semantics are enforced; typed failure counters (`DeliverySweepCounters`) and terminal CLOSE/RESET cleanup replace silent packet drops. The four-test black-box evidence lives in `crates/i2pr-daemon/tests/sam_stream_self_composed.rs` and includes exact bidirectional 2 MiB transfer and same-read raw-byte checks.

[**Plan 150**](plans/150-m7-sam31-external-client-reproducible-final-closure.md) closes the localhost SAM-client layer. Its reproducible lane passes with independently implemented `i2psam` (`b80ecd487f7b8d1a743a1f40337b2eb0caaae6ac`) and the qualified pinned `i2plib.sam` substitute (`6edf51cd5d21cc745aa7e23cb98c582144884fa8`), including exact binary STREAM directions, SILENT transitions, private destinations, FORWARD, NAMING, and negative cases. The official `libsam3` snapshot (`7d6e658798baec31394c5685f9583343cc00900b`) is built and recorded but cannot consume i2pr's compact 608-character `PRIV` through its public 884-character-minimum API. `sam_independent_clients = at-least-two-passed`; Milestone 7 is closed for its localhost application scope, while router-to-router interoperability remains unclaimed and Milestone 8 planning is next.

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
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
```

Focused seams and the constrained-host lane are documented in [`AGENTS.md`](AGENTS.md).

## OpenCode skills

Loadable skill bundles under [`.opencode/skills/`](.opencode/skills/) cover the routine development seam ([`i2pr-local-dev`](.opencode/skills/i2pr-local-dev/SKILL.md)), documentation navigation ([`i2pr-architecture`](.opencode/skills/i2pr-architecture/SKILL.md)), the closed NTCP2 interop lane ([`i2pr-ntcp2-interop`](.opencode/skills/i2pr-ntcp2-interop/SKILL.md)), the Plan 046 rootless sandbox ([`i2pr-rootless-sandbox`](.opencode/skills/i2pr-rootless-sandbox/SKILL.md)), and the Plan 048–051 Multipass recovery guest ([`i2pr-multipass-recovery`](.opencode/skills/i2pr-multipass-recovery/SKILL.md)). Load the matching skill before touching its surface.

## License

No license selected yet. Do not copy code from I2P+, i2pd, Emissary, or other routers until license compatibility is reviewed. Specifications and observed behavior may be used for clean-room implementation.
