# i2pr

An experimental I2P router written in Rust. **Not production-ready.** Not suitable for anonymity, privacy, censorship resistance, or security-sensitive workloads.

## Status

`i2pr` is under active development. The workspace builds and passes tests, but significant work remains before the router is functional on the I2P network.

**Implemented:**
- Bounded wire protocol codecs (`i2pr-proto`)
- Cryptographic wrappers: Ed25519, X25519, AES, ChaCha20-Poly1305, HMAC, SipHash, HKDF-SHA256 (`i2pr-crypto`)
- Versioned private-identity storage (`i2pr-storage`)
- Runtime-neutral transport and service contracts (`i2pr-transport`, `i2pr-transport-ntcp2`)
- Tokio-owned runtime with supervision, cancellation, and bounded channels (`i2pr-runtime`)
- Deterministic testkit with seeded randomness and fault injection (`i2pr-testkit`)
- RouterInfo validation, bounded local NetDB store, and local signed RouterInfo construction (`i2pr-netdb`)
- Persistent RouterInfo cache, SU3 reseed verification, and reseed ingestion (`i2pr-netdb-persist`)
- Transport-neutral lookup, store, and publication state machines (`i2pr-netdb`)
- Daemon bootstrap integration with `NetDbSeam` (`i2pr-daemon`)
- Exploratory tunnel substrate, pool, and reply-path provider (`i2pr-tunnel`)
- ECIES-X25519 short tunnel-build cryptography — locally conformant
  single-record `Noise_N_25519_ChaChaPoly_SHA256` implementation with
  exact 154-byte request plaintext, 218-byte encrypted envelope,
  202-byte reply plaintext, hop-own reply AEAD, derived
  `replyKey/layerKey/ivKey`, and the OBEP garlic continuation; the
  Plan 108 implementation was corrected by
  [`plans/109-short-build-record-and-noise-conformance-correction.md`](plans/109-short-build-record-and-noise-conformance-correction.md)
  and the conformance fixture is published in
  [`crates/i2pr-tunnel/src/conformance_fixtures.rs`](crates/i2pr-tunnel/src/conformance_fixtures.rs)
- Runtime-neutral `ShortBuildStateMachine`, success-only
  `ShortBuildRegistrar`, and deterministic `DeterministicResponder`
  peer simulator (`i2pr-tunnel`)
- CLI daemon with config validation, identity generation, and dry-run (`i2pr-daemon`)

**Not implemented:**
- Live NTCP2 or SSU2 transport (NTCP2 experimental, non-advertised)
- Complete randomized multi-record `ShortTunnelBuild` slot layout,
  fake records, and one-byte count payload framing (Plan 110 scope)
- Live mixed-router tunnel build execution (depends on the multi-record
  closure and a qualified router-to-router transport)
- NetDB lookup/publication over the network
- I2NP message handling and router dispatch
- Streaming, SAM, I2CP, garlic, LeaseSet management
- Client proxies (HTTP, SOCKS5)
- Any network-facing behavior

The NTCP2 development interoperability result is `protocol-defect-localized` at `noise_authenticated`. No passed mixed-router NTCP2 result exists.

## Workspace

```text
crates/
  i2pr-proto/               Wire types, codecs, constants, validation
  i2pr-crypto/              Protocol-specific cryptographic wrappers
  i2pr-storage/             Atomic persistence and migration support
  i2pr-core/                Shared contracts, lifecycle, budgets, health
  i2pr-transport/           Transport-neutral link management and selection
  i2pr-transport-ntcp2/     NTCP2 protocol implementation (no I/O)
  i2pr-runtime/             Tokio-owned supervision, cancellation, and I/O
  i2pr-netdb/               RouterInfo validation, NetDB store, lookup, publication
  i2pr-netdb-persist/       Persistent cache and SU3 reseed ingestion
  i2pr-tunnel/              Tunnel identity, exploratory pool, ECIES-X25519 short-build cryptography (implementation-landed, protocol-conformance reopened; see plans/108-conformance-amendment.md), runtime-neutral build state machine, reply-path provider
  i2pr-daemon/              CLI, configuration, composition, supervision
  i2pr-testkit/             Deterministic simulation and adversarial fixtures
tools/
  i2pr-interop/             Non-production interop launcher (test only)
```

## Building and testing

Requires Rust 1.95.0 (pinned via `rust-toolchain.toml`); MSRV is 1.85.

```text
cargo fmt --all --check
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
```

## Architecture

**Modular monolith** — one process composed from focused crates. Crate boundaries follow security boundaries and protocol ownership.

**Wire compatibility** — protocol codecs and crypto state machines are separate from router policy. Peer selection, transit, and resource allocation vary by profile without changing wire behavior.

**Explicit trust boundaries** — all network, client, and configuration inputs are untrusted until validated. Subsystems receive only the capabilities they require.

**Bounded execution** — queues, buffers, handshakes, sessions, tunnel builds, and API clients have explicit limits with deadlines, cancellation, and cleanup.

**Defensive Rust** — safe Rust by default; `unsafe` forbidden in protocol and crypto crates. Secret-bearing types avoid logging, cloning, and serialization.

**Testability** — deterministic clocks, seeded randomness, in-memory transports, and fault injection are first-class.

## MVP direction

The feature MVP includes: CLI router daemon, persistent identity, I2NP handling, NTCP2/SSU2 transport, NetDB client/floodfill, tunnel construction, destination/LeaseSet management, streaming, SAM/I2CP interfaces, HTTP/SOCKS5 proxies, and bounded resource accounting.

Development targets a smaller interoperable-router milestone before the complete MVP.

## License

No license selected yet. Do not copy code from I2P+, i2pd, Emissary, or other routers until license compatibility is reviewed. Specifications and observed behavior may be used for clean-room implementation.
