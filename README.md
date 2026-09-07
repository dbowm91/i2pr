# i2pr

An experimental I2P router written in Rust. **Not production-ready.** Not suitable for anonymity, privacy, censorship resistance, or any security-sensitive workload. NTCP2 stays experimental and non-advertised.

## Status

The local Milestone 6 product (destinations, garlic, LeaseSet2, Streaming) remains closed under corrected local-correctness semantics via [**Plan 134**](plans/134-status.md). Independent-router interoperability is tracked as external acceptance debt.

Milestone 7 / SAM has several strong retained results:

- [**Plan 146**](plans/146-status.md) passed bidirectional SAM 3.1 private-destination reference requalification against pinned Java I2P/i2pd behavior.
- [**Plan 147**](plans/147-status.md) landed the dedicated same-socket raw STREAM owner, TCP↔Streaming byte pump, actual Streaming `Established` wait, OS-CSPRNG runtime path, and supervised ACK/retransmit driver.
- [**Plan 149**](plans/149-status.md) passed the self-composing localhost STREAM product. `SESSION CREATE` builds the destination/LeaseSet2/bridge/local-delivery/runtime-driver composition before returning success, and the canonical black-box test drives the resulting path only through SAM TCP after listener startup.
- [**Plan 150**](plans/150-status.md) retains successful external-client core evidence with exact pinned `i2psam` and qualified pinned `i2plib.sam` client surfaces. Its broad final-closure interpretation was superseded by the stricter Plan 151 evidence gate.

[**Plan 151**](plans/151-status.md) closed Milestone 7 final localhost SAM acceptance with executable sibling-stream/backpressure/fault/CLOSE-RESET/FORWARD lifecycle evidence, focused M6 regressions, and hosted external-client evidence.

[**Plan 152**](plans/152-status.md) is the retained narrow Milestone 6 robustness corrective discovered by Plan 151: receiver-retention cap with ACK gating, coalesced duplicate ACKs, and sender ECIES ratchet-key trimming, without a wire-format change.

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
plan_160 = passed-m8-ssu2-peer-test-and-relay-reachability
plan_161 = passed-m8-ssu2-independent-ipv4-interop-and-final-closure
plan_162 = passed-m8-ssu2-external-test-lane-isolation-and-ci-restoration
plan_163 = registered-m9-i2cp-roadmap
plan_164 = passed-m9-i2cp-protocol-and-wire-foundation

milestone7_local_product = passed-via-plan149
plan150_external_core_evidence = retained-passed
milestone7_sam_localhost = passed-via-plan151
milestone7_final_acceptance = closed
milestone6_interoperable = not-yet-claimed
milestone8_ssu2_direction_a = passed-via-plan161
milestone8_ssu2_direction_b = passed-via-plan161
milestone8_ssu2_ledger = landed-via-plan161
milestone8_final_acceptance = closed-via-plan161
milestone9_planning_authority = plan163
milestone9_wire_foundation = passed-via-plan164
milestone9_final_acceptance = not-yet-closed
next_executable_plan = 165
next_product_layer = milestone9-i2cp
```

Milestone 8 is **closed** via [**Plan 161**](plans/161-status.md) within its bounded direct-interop scope. Directions A and B are genuinely proven against exact-pinned i2pd 2.61.0 (`635b013a612ff47278ef02acf8580a28e10e26c5`) over real loopback UDP, including authenticated session establishment, small and fragmented I2NP/DatabaseStore exchange with return DeliveryStatus traffic, cached-token behavior, malformed/resource rows, and the fail-closed external evidence lane. [**Plan 162**](plans/162-status.md) passed the narrow external-test lane correction. Public-network, NetDB/tunnel/destination, IPv6-external, PQ, SSU1, and Milestone 6 mixed-router interoperability remain outside that claim.

Milestone 9 / I2CP planning is now registered via [**Plan 163**](plans/163-status.md). [**Plan 164**](plans/164-status.md) passed the I2CP source/profile/wire foundation: pinned official I2CP sources with Java I2P 2.13.0 and go-i2cp references, the explicit M9 compatibility profile, runtime-neutral `i2pr-api::i2cp` bounded framing/message codecs with committed fixtures and a routine-CI vector checker, and no sockets or sessions. Execute [**Plan 165**](plans/165-m9-i2cp-connection-session-and-options.md) next, then Plans 166–170 in order. The M9 architecture reuses the existing destination product rather than creating an I2CP-specific router stack: runtime-neutral I2CP framing/session state lives in `i2pr-api`, TCP/Tokio listener ownership remains in `i2pr-daemon`, and `i2pr-client` gains an explicit client-owned destination capability. For I2CP, the external client proves Destination signing-key ownership with a signed SessionConfig and supplies a signed Standard LeaseSet2 plus required X25519 decryption key; i2pr must not require the client's Destination signing private key. I2CP remains experimental, disabled by default, and loopback-only throughout M9. Final Plan 170 targets independent Java I2P and Go I2CP clients with cross-client application traffic and fail-closed hosted evidence.

The `[sam]` config section remains disabled by default and loopback-only when enabled. No localhost SAM or I2CP result is router-to-router interoperability evidence.

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
  i2pr-transport-ssu2/      SSU2 v2 protocol (runtime-neutral), path validation/publication, peer-test/relay/introducers
  i2pr-runtime/             Tokio-owned supervision, cancellation, transport I/O
  i2pr-netdb/               RouterInfo + LeaseSet2 validation, store, lookup, publication
  i2pr-netdb-persist/       Persistent cache + bounded SU3 reseed ingestion
  i2pr-tunnel/              Tunnel identity, exploratory pool, ECIES-X25519 short-build, runtime-neutral data plane
  i2pr-client/              Destinations, ECIES-X25519-AEAD-Ratchet session layer, routing, I2P Streaming
  i2pr-api/                 Runtime-neutral application-protocol adapters (SAM 3.1 plus the M9 I2CP wire/profile foundation; no sockets)
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

Loadable skill bundles under [`.opencode/skills/`](.opencode/skills/) cover the routine development seam ([`i2pr-local-dev`](.opencode/skills/i2pr-local-dev/SKILL.md)), documentation navigation ([`i2pr-architecture`](.opencode/skills/i2pr-architecture/SKILL.md)), the closed NTCP2 interop lane ([`i2pr-ntcp2-interop`](.opencode/skills/i2pr-ntcp2-interop/SKILL.md)), the historical rootless sandbox ([`i2pr-rootless-sandbox`](.opencode/skills/i2pr-rootless-sandbox/SKILL.md)), and the historical Multipass recovery guest ([`i2pr-multipass-recovery`](.opencode/skills/i2pr-multipass-recovery/SKILL.md)). Load the matching skill before touching its surface.

## License

No license selected yet. Do not copy code from I2P+, i2pd, Emissary, or other routers until license compatibility is reviewed. Specifications and observed behavior may be used for clean-room implementation.
