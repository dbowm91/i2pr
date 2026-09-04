# i2pr

An experimental I2P router written in Rust. **Not production-ready.** Not suitable for anonymity, privacy, censorship resistance, or any security-sensitive workload. NTCP2 stays experimental and non-advertised.

## Status

The local Milestone 6 product (destinations, garlic, LeaseSet2, Streaming) remains closed under corrected local-correctness semantics via [**Plan 134**](plans/134-status.md). Independent-router interoperability is tracked as external acceptance debt.

Milestone 7 / SAM has several strong retained results:

- [**Plan 146**](plans/146-status.md) passed bidirectional SAM 3.1 private-destination reference requalification against pinned Java I2P/i2pd behavior.
- [**Plan 147**](plans/147-status.md) landed the dedicated same-socket raw STREAM owner, TCP↔Streaming byte pump, actual Streaming `Established` wait, OS-CSPRNG runtime path, and supervised ACK/retransmit driver.
- [**Plan 149**](plans/149-status.md) passed the self-composing localhost STREAM product. `SESSION CREATE` now builds the destination/LeaseSet2/bridge/local-delivery/runtime-driver composition before returning success, and the canonical black-box test drives the resulting path only through SAM TCP after listener startup.
- [**Plan 150**](plans/150-status.md) retains successful external-client core evidence with exact pinned `i2psam` and qualified pinned `i2plib.sam` client surfaces: both cross-client 2 MiB directions, private destinations, SILENT, NAMING, negative inputs, and a positive loopback FORWARD trajectory passed. Its original broad final-closure interpretation was superseded after audit found required sibling-stream, slow-peer, fault, full FORWARD lifecycle, and focused M6 regression rows were not all executed by the closing harness.

[**Plan 151**](plans/151-m7-sam31-final-acceptance-evidence-correction.md) closed the Milestone 7 final acceptance: synthetic `passed` bookkeeping removed, executable sibling-stream/backpressure/fault/CLOSE-RESET/FORWARD lifecycle acceptance green, Plan 127–134 regression floor rerun, and the hosted external lane passed on the closing head (see [`plans/151-status.md`](plans/151-status.md) for the closure record).

[**Plan 152**](plans/152-m6-session-streaming-robustness-corrective.md) is the narrow Milestone 6 corrective Plan 151 §17 required (receiver retention cap with ACK gating, coalesced duplicate ACKs, sender ECIES ratchet-key trimming; no wire change). Fixes landed with unit tests; the full workspace floor was green on the Plan 151 closing head with routine CI and the hosted SAM external lane passing (see [`plans/151-status.md`](plans/151-status.md) and [`plans/152-status.md`](plans/152-status.md) for the closure records).

Current classification:

```text
plan_151 = passed-m7-sam31-final-acceptance-evidence-correction
plan_152 = passed-m6-session-streaming-robustness-corrective
plan_153 = passed-post-m7-authority-and-ci-hygiene
plan_154 = registered-m8-ssu2-v2-roadmap
plan_155 = passed-m8-ssu2-v2-protocol-foundation-and-addresses
plan_156 = passed-m8-ssu2-v2-handshake-token-and-routerinfo
plan_157 = passed-m8-ssu2-v2-data-phase-reliability-and-fragmentation
plan_158 = passed-m8-ssu2-udp-runtime-and-local-session-product
plan_159 = passed-m8-ssu2-path-validation-publication-and-transport-selection
milestone7_local_product = passed-via-plan149
plan150_external_core_evidence = retained-passed
milestone7_sam_localhost = passed-via-plan151
milestone7_final_acceptance = closed
milestone6_interoperable = not-yet-claimed
next_executable_plan = 160
next_product_layer = milestone8-ssu2-v2
```

Milestone 8 roadmap is registered via [**Plan 154**](plans/154-status.md); Plan 153 has passed so Milestone 8 implementation begins at Plan 155.

[**Plan 155**](plans/155-status.md) passed the Milestone 8 SSU2 v2 protocol foundation: runtime-neutral `i2pr-transport-ssu2` (strict v2 RouterAddress/header/block primitives with fixture-backed vectors, no handshake, no UDP sockets), `TransportKind::Ssu2` integration, and the SSU2 source-authority refresh.

[**Plan 156**](plans/156-status.md) passed the Milestone 8 SSU2 v2 establishment protocol, still fully runtime-neutral (no UDP sockets, no runtime service): the Noise XK transcript (`Noise_XKchaobfse+hs1+hs2+hs3_25519_ChaChaPoly_SHA256`), ChaCha20 header protection, strict TokenRequest/Retry/SessionRequest/SessionCreated codecs with cheap prevalidation, the bounded one-use token lifecycle, RouterInfo fragmentation/validation/binding, replay/deadline state, and consuming initiator/responder machines reaching matching directional data keys. Six committed handshake vectors (one with raw-primitive independent derivation) pin the transcript.

[**Plan 157**](plans/157-status.md) passed the Milestone 8 SSU2 v2 data phase, still fully runtime-neutral (no UDP sockets, no runtime service): the authenticated `Ssu2Session` short-header packet path with the corrected two-step data-phase KDF, the bounded packet-number/replay window, strict ACK range interpretation with loop-free delayed/immediate scheduling, fresh (never ciphertext-replayed) retransmission with a conservative RTT/RTO/congestion controller, exact MTU-aware I2NP fragmentation with strict reassembly quotas and duplicate suppression, and termination/rekey/idle handling with privacy-safe counters. Seventeen deterministic fault trajectories plus unit boundary tests and two committed data-phase vectors pin the behavior. No socket/runtime interoperability is claimed; the next executable plan is 158.

[**Plan 158**](plans/158-status.md) passed the first Milestone 8 SSU2 UDP runtime: `i2pr-runtime::Ssu2RuntimeService` owns real UDP sockets and drives the Plan 156/157 state machines through a central bounded scheduler (one loop task per socket, no task/timer per packet), charges pending handshakes through shared transport resources with per-IP/subnet caps, promotes atomically through the generic `TransportManager` as `TransportKind::Ssu2`, admits outbound I2NP through the transport-neutral delivery contract, caches `NewToken` announcements for cached-token dials with stale-token Retry fallback, and proves the i2pr↔i2pr local session product over real localhost datagrams (tokenless/cached-token establishment, bidirectional fragmented I2NP, loss/ACK-loss/reorder/duplicate recovery, malformed-traffic boundedness, admission ceilings, graceful/abrupt/cancel baselines). The daemon `[ssu2]` surface stays disabled by default with no advertisement; path validation, publication, and selection belong to Plan 159.

The `[sam]` config section remains disabled by default and loopback-only when enabled. No localhost SAM result is router-to-router interoperability evidence.

[**Plan 159**](plans/159-status.md) passed Milestone 8 SSU2 path validation, publication policy, and transport selection without changing wire semantics or enabling production publication: the runtime-neutral `PathValidator` (bounded per-family candidates, OS-CSPRNG challenges, exact-proof promotion, minimum-MTU candidates), migration-safe congestion reset, the corroboration-gated router reachability policy (one observation can never publish `Reachable`), deterministic canonical publication snapshots (direct form needs explicit opt-in), and deterministic NTCP2/SSU2 reuse/dial/fallback selection through the generic manager — proven by sealed-packet trajectories, unit matrices, and real-UDP migration/spoof/round-trip tests. The daemon `[ssu2]` surface stays disabled by default with no advertisement; peer-test/relay roles belong to Plan 160 and independent interop to Plan 161.

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
    i2pr-transport-ssu2/     SSU2 v2 protocol (runtime-neutral) + one-shot NewToken queue (Plan 158) + path validation/publication (Plan 159)
   i2pr-runtime/             Tokio-owned supervision, cancellation, I/O (incl. SSU2 UDP runtime, Plan 158; path validation, Plan 159)
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