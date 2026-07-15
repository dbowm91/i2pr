# Tooling — Deep Dive

Every script, fixture corpus, integration lane, and CI job that wraps
the workspace. The boundary contract is enforced here; if a script
rejects, fix the boundary, do not suppress the script.

Paths are relative to the workspace root.

## `scripts/` — Six guardrail shells

| Script | What it catches |
| --- | --- |
| `scripts/check-dependency-direction.sh` | Crate-layer DAG violations. Uses `cargo metadata` piped to a Python 3 JSON reader with an explicit allowlist map. |
| `scripts/check-runtime-boundaries.sh` | Grep-based audit: unbounded channels, wall-clock sleeps, raw `JoinHandle`s, `tokio::spawn` without an owner, `async fn` in transport contracts, Tokio deps in wrong crates, `std::net`/`std::fs` in transport, `i2pr-testkit` referenced by a production crate. |
| `scripts/check-fixture-manifest.sh` | Drift in the I2NP fixture corpus under `tests/fixtures/i2np/`. Validates manifest IDs, classification (`positive`/`negative`), provenance (`locally-authored`/`independently-produced`), all metadata fields, on-disk file existence, and SHA-256 hash matches. Rejects orphan `.hex` files (i.e. unlisted fixtures). |
| `scripts/check-ntcp2-vectors.sh` | Drift in the NTCP2 crypto vector corpus under `tests/fixtures/ntcp2/crypto/`. Verifies duplicate-free manifest, `positive`/`malformed` categories, 64-char hex hashes, path containment, file existence, and SHA-256 match. Additionally verifies `vectors.tsv` contains all 13 required NTCP2 crypto vector IDs. |
| `scripts/check-ntcp2-interoperability.sh` | Forbidden artifacts in the synthetic private NTCP2 interoperability lane. Checks for required disclaimer lines (`network_id`, `public_network = false`, `reseed = false`, etc.), exactly 8 `[[scenario]]` entries, and scans the committed `evidence/` directory for forbidden artifacts (`.pcap`, `.pcapng`, `router.identity`, `ntcp2.static.key`, private key headers). |
| `scripts/fuzz-smoke.sh` | Opt-in smoke run of all 22 fuzz targets for 32 iterations each at seed=1 (`-runs=32 -seed=1`). Requires `cargo-fuzz` + nightly. Disables LeakSanitizer (`LSAN_OPTIONS=detect_leaks=0`) for managed environments. |

**How they work**: `check-dependency-direction.sh` uses
`cargo metadata` + Python. The others use `rg` (ripgrep) for
pattern scanning and `sha256sum` / `find` for manifest integrity.
`fuzz-smoke.sh` delegates to `cargo fuzz run`.

## `tests/fixtures/i2np/` — I2NP wire fixture corpus

- `manifest.tsv` — 31 entries (15 positive, 16 negative) with id,
  path, classification, SHA-256, source (official I2NP spec),
  revision, generator, deterministic input description, expected
  decode/error, license (CC-BY), and provenance
  (`locally-authored`).
- **31 `.hex` files** — hand-crafted binary fixtures exercising
  DeliveryStatus, DatabaseLookup (none/legacy/ecies),
  DatabaseSearchReply, DatabaseStore (classic LeaseSet / compressed
  RouterInfo), TunnelData, TunnelGateway (nested),
  VariableTunnelBuild, ShortTunnelBuild, Garlic/Data
  deferred-length, plus 16 malformed variants (bad checksum,
  truncated header, oversized payload, trailing bytes, unknown
  type, invalid flags, excessive counts, zero IDs).
- **Verified by** `scripts/check-fixture-manifest.sh` on every CI
  run.

## `tests/fixtures/ntcp2/crypto/` — NTCP2 cryptographic vector corpus

- `manifest.tsv` — lists crypto test vectors with SHA-256 integrity.
- `vectors.tsv` — 13 named deterministic vectors covering X25519
  key exchange, protocol name hash, transcript initial/final hash,
  SessionRequest/SessionCreated/SessionConfirmed AEAD,
  ChaCha20-Poly1305 seal, AES-CBC ephemeral, and Split-KDF outputs.
- Hex files: `storage-static-key.hex`,
  `data-phase-frame.hex`, `data-phase-blocks.hex`,
  `data-phase-malformed.hex`.
- **Verified by** `scripts/check-ntcp2-vectors.sh` on every CI run.

## `tests/integration/ntcp2/` — synthetic interoperability lane (Plan 036)

- `manifest.toml` — defines a synthetic test network
  (`network_id = "synthetic-private-036"`), loopback-only, fixed
  clocks, disposable identities. Pins reference implementations:
  Java I2P 2.12.0 (rev `2800040`) and i2pd 2.60.0 (rev
  `f618e41`). Specifies exactly 8 scenarios:
  1. `java-ipv4-inbound-outbound` — authenticated handshake + I2NP
     exchange.
  2. `java-ipv6-inbound-outbound` — same, IPv6.
  3. `java-adversarial-and-resource` — boundary / oversized padding
     rejection.
  4. `java-duplicate-link-race` — deterministic winner/loser drain.
  5. `i2pd-ipv4-inbound-outbound` — same as java-ipv4.
  6. `i2pd-ipv6-inbound-outbound` — same as java-ipv6.
  7. `i2pd-adversarial-and-resource` — same as java-adversarial.
  8. `i2pd-duplicate-link-race` — same as java-duplicate-link.
- `evidence/` — currently contains only `README.md`. The README
  states that `i2pr` daemon activation is disabled; Java I2P and
  i2pd lanes are "recorded blockers, not skipped successes."
- `README.md` — explains this is manual / opt-in, requires an
  authorized external runner, and that
  `cargo test -p i2pr-testkit --all-targets` is the local
  substitute.
- **Verified by** `scripts/check-ntcp2-interoperability.sh` on
  every CI run.

### Plan 038 Ubuntu harness (planned, opt-in)

Plan 038 extends the manual lane with an Ubuntu-only, amd64-only harness. The
existing manifest and evidence preflight remain a repository boundary; they do
not install or launch reference routers. The planned host and build commands
are:

```text
bash scripts/interop/ubuntu/check-host.sh --pre-install
bash scripts/interop/ubuntu/setup-host.sh
bash scripts/interop/ubuntu/check-host.sh --post-install
bash scripts/interop/build-references.sh
bash scripts/interop/build-references.sh --offline
```

Reference builds are source-pinned and hashed during preparation. The runner
then uses a separate execution phase for each scenario. It creates one i2pr
namespace and one reference namespace, moves both ends of a veth pair out of
the host namespace, permits only the expected directly connected routes, and
rejects default routes, DNS, host bridges, and public egress before launch.
Route checks are primary; namespace-scoped nftables rules are defense in
depth. Execution has no dependency downloads, reseed, bootstrap, RouterInfo
publication, NetDB mutation, or public endpoint.

The scenario and launcher interfaces are:

```text
bash scripts/interop/run-scenario.sh --scenario <id> --reference java-i2p --build-cache <path> --run-root <path>
bash scripts/interop/run-scenario.sh --scenario <id> --reference i2pd --build-cache <path> --run-root <path>
bash scripts/interop/run-matrix.sh
i2pr-interop ntcp2 listen --scenario-config <path>
i2pr-interop ntcp2 dial --scenario-config <path>
i2pr-interop ntcp2 inspect --state-dir <path>
```

The evidence taxonomy is strict: environment smoke validates reference
startup/RouterInfo generation and cleanup only; reference crosscheck validates
Java I2P versus i2pd and makes no i2pr claim; i2pr mixed-router evidence
requires bounded authenticated runs between i2pr and each reference in both
directions. The full eight-scenario manifest remains gated on the positive
smoke profiles. Sanitize before retention and keep only typed outcomes,
bounded run metadata, and artifact/configuration hashes. Delete raw
addresses, peer identities, RouterInfo, I2NP, keys, transcripts, logs, and
arbitrary remote error text. Until the harness is implemented and produces
qualifying records, this lane remains a blocker and NTCP2 remains
experimental/non-advertised.

### Zero Rust integration test files

There are **zero `.rs` files** under `tests/`. All decode/encode
verification lives inside the crates (typically in
`#[cfg(test)]` modules). The `tests/` tree is purely fixture data
and the interoperability manifest — a deliberate separation.

## `fuzz/` — opt-in fuzz workspace

- **Separate workspace** (`fuzz/Cargo.toml` declares `[workspace]`
  with no members — standalone).
- **Package**: `i2pr-proto-fuzz`, edition 2021, `publish = false`.
- **Dependencies**: `i2pr-crypto`, `i2pr-proto`, `i2pr-storage`,
  `i2pr-transport-ntcp2`, `libfuzzer-sys 0.4`.
- **22 fuzz targets** in `fuzz/fuzz_targets/`, each declared as a
  `[[bin]]`:

| Target | What it fuzzes |
| --- | --- |
| `date` | Date codec |
| `date32` | 32-bit date codec |
| `hash` | Hash primitives |
| `mapping` | Mapping codec |
| `certificate` | Certificate codec |
| `key_certificate` | Key+Certificate codec |
| `key_and_cert` | KeyAndCert combined |
| `router_identity` | RouterIdentity codec |
| `destination` | Destination codec |
| `router_address` | RouterAddress codec |
| `router_info` | RouterInfo codec (1 MiB max) |
| `lease` | Lease codec |
| `lease_set` | LeaseSet codec |
| `i2np_standard` | I2NP standard message decode |
| `i2np_bodies` | I2NP body parsing |
| `i2np_short_ssu` | I2NP short SSU framing |
| `i2np_short_transport` | I2NP short transport framing |
| `ntcp2_transcript` | NTCP2 transcript hash |
| `ntcp2_storage` | NTCP2 static key decode |
| `ntcp2_handshake` | NTCP2 handshake state machine (full fuzz) |
| `ntcp2_blocks` | NTCP2 block parsing |
| `ntcp2_frames` | NTCP2 wire frame open/seal |

- `support.rs` — shared module defining `COMMON_MAX` (1 MiB) and
  `I2NP_MAX` (62,724) with a `within()` guard.

### How to drive

```bash
rustup toolchain install nightly
cargo install cargo-fuzz
RUSTUP_TOOLCHAIN=nightly cargo fuzz run --fuzz-dir fuzz ntcp2_handshake -- -runs=10000
bash scripts/fuzz-smoke.sh   # all 22 targets, 32 iterations each
```

### Corpus

- `fuzz/corpus/<target>/` directories with seed files; most include
  a `seed-oversized-shape`.
- `fuzz/corpus/metadata.toml` records provenance: all seeds
  locally authored, no third-party bytes.

## `.cargo/config.toml`

```toml
[term]
color = "auto"
```

Minimal. No rustflags, no target-dir overrides, no custom
subcommands, no hidden `-Z` flags. Deliberately clean.

## `.github/` — CI

### `.github/workflows/ci.yml` (single workflow, three jobs)

| Job | OS | Steps |
| --- | --- | --- |
| **Quality** | ubuntu-latest + macos-latest (matrix, fail-fast: false) | Checkout → Rust 1.95.0 + rustfmt + clippy → `cargo fmt --all --check` → `cargo check --workspace` → `cargo check --workspace --all-targets` → `cargo test --workspace` → `cargo clippy --workspace --all-targets --all-features -- -D warnings` → `cargo doc` (with `-D warnings`) → `check-dependency-direction.sh` (both OS) → `check-runtime-boundaries.sh` (Linux) → `check-fixture-manifest.sh` (Linux) → `check-ntcp2-vectors.sh` (Linux) → `check-ntcp2-interoperability.sh` (Linux) |
| **MSRV** | ubuntu-latest | Rust **1.85.0** → `cargo check --workspace --all-targets` |
| **Dependency policy** | ubuntu-latest | Rust 1.95.0 → `cargo-deny check advisories bans sources` |

Triggers: `on: push`, `on: pull_request` (all branches).

### `.github/dependabot.yml`

- Cargo ecosystem: weekly, max 5 open PRs.
- GitHub Actions: weekly, max 5 open PRs.

## `.codex/` and `.agents/`

- `.codex/` — empty directory (placeholder).
- `.agents/` — does not exist at the workspace root.

## Top-level `Cargo.toml` — workspace configuration

### Members (9 crates)

```
i2pr-crypto, i2pr-proto, i2pr-core, i2pr-daemon, i2pr-runtime,
i2pr-storage, i2pr-testkit, i2pr-transport, i2pr-transport-ntcp2
```

Resolver v2, edition 2024, MSRV 1.85, workspace version 0.1.0.

### Workspace dependencies (14, all default-features = false unless needed)

- **Crypto**: `aes 0.8.4`, `chacha20poly1305 0.10.1` (+alloc),
  `ed25519-dalek 2.2` (+std+zeroize), `hmac 0.12.1`,
  `sha2 0.10`, `siphasher 1.0.3`, `subtle 2.6`,
  `x25519-dalek 2.0.1` (+static_secrets+zeroize),
  `zeroize 1.8` (+derive).
- **Runtime**: `tokio 1.48` (+io-util+macros+net+rt+sync+time+test-util),
  `tokio-util 0.7` (+rt), `futures-util 0.3` (+std).
- **General**: `clap 4.5` (+derive+help+std+usage),
  `thiserror 2.0`, `tracing 0.1`,
  `tracing-subscriber 0.3` (+env-filter+fmt),
  `serde 1.0` (+derive), `toml 0.8`, `rand_chacha 0.9`,
  `rand_core 0.9`.

### Workspace lints

```toml
[workspace.lints.rust]
unsafe_code = "deny"
unexpected_cfgs = "deny"
unused_must_use = "warn"

[workspace.lints.clippy]
dbg_macro = "deny"
todo = "deny"
unimplemented = "deny"
```

### Profile overrides

```toml
[profile.dev]
overflow-checks = true

[profile.test]
overflow-checks = true

[profile.release]
panic = "unwind"
lto = false
```

Overflow checks are enabled in dev and test profiles. Release uses
unwinding panics and no LTO (fast builds over binary size).

## Distinctive design choices

1. **Zero Rust integration test files under `tests/`.** All
   verification lives inside the crates.
2. **Dual-toolchain CI.** Production builds use 1.95.0; MSRV
   verification runs 1.85.0 separately. The toolchain is pinned in
   `rust-toolchain.toml`.
3. **Python in the dependency-direction guard.** Uses Python 3 stdlib
   for JSON parsing — the only script that does so.
4. **`unsafe_code = "deny"` workspace-wide** combined with clippy
   denies on `dbg!`, `todo!`, `unimplemented!`. Very strict lint
   posture.
5. **Fuzz workspace fully isolated.** Declares its own `[workspace]`
   with no members, depends on production crates by path, and uses
   edition 2021 (not 2024).
6. **Interoperability is a documented blocker, not a skip.** The
   evidence directory README explicitly says Java I2P and i2pd
   lanes are "recorded blockers, not skipped successes."
7. **`LSAN_OPTIONS=detect_leaks=0` in fuzz-smoke.** Disabled
   because managed CI runs sanitizer binaries under ptrace, which
   triggers false LeakSanitizer aborts.
8. **Manifest-driven fixture integrity.** Both I2NP and NTCP2
   corpora use TSV manifests with SHA-256 hashes, classification,
   provenance, and independence tracking.
9. **Edition 2024 in production, 2021 in fuzz.** Likely because
   `libfuzzer-sys` / `cargo-fuzz` aren't edition-2024-compatible
   yet.
10. **Top-level `AGENTS.md`** is the canonical developer guide —
    read it before changing code, alongside `README.md`,
    `GUARDRAILS.md`, the applicable `plans/NNN-*.md`, and relevant
    `docs/adr/` records.

## Pre-handoff sequence (from `AGENTS.md`)

```text
cargo fmt --all --check
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh        # when I2NP fixture bytes change
bash scripts/check-ntcp2-vectors.sh           # when NTCP2 vector bytes change
bash scripts/check-ntcp2-interoperability.sh  # when ntcp2 evidence/manifest change
bash scripts/fuzz-smoke.sh                    # opt-in; requires cargo-fuzz + nightly
```

## Cross-references

- [Overview](overview.md)
- [`AGENTS.md`](../../AGENTS.md)
- [`CONTRIBUTING.md`](../../CONTRIBUTING.md)
- [`GUARDRAILS.md`](../../GUARDRAILS.md)
- Plan-of-record: latest active `plans/NNN-*.md`
