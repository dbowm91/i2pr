# Repository Guidelines

`i2pr` is an experimental Rust I2P router. **Not production-ready.** Do
not use for anonymity, privacy, censorship resistance, or any
security-sensitive workload. NTCP2 stays experimental and
non-advertised; the production daemon does **not** activate NTCP2.

## Read first

Always read these before changing code or answering questions about state:

1. `README.md` — current status (Milestone 6 local product gate
   closed via Plan 134; Milestone 7 SAM corrective authority is Plan 141,
   with Plan 142 next), build/test/lint commands, high-level architecture.
2. `GUARDRAILS.md` — non-negotiable engineering, security,
   interoperability, and collaboration constraints.
3. `CONTRIBUTING.md` — local quality checks, the runtime/testkit
   conventions, the rootless and Multipass evidence-lane contracts.
4. The active plan under `plans/` (entry point:
   [`plans/README.md`](plans/README.md); current corrective status:
   `plans/141-status.md`; next executable: Plan 142) and the relevant
   `docs/adr/` record. The [`docs/architecture/audit/`](docs/architecture/audit/)
   directory tracks doc-vs-source drift findings from the most recent audits.
5. `specs/support.toml` (mirrored to `docs/protocol-support.md`) for
   the live protocol support state. `specs/CONFORMANCE.md` defines
   what counts as evidence.

Do **not** trust prose that disagrees with a checked-in script or
executable test. Static boundary scripts (`scripts/check-*.sh`) are
the source of truth for non-negotiable invariants. The
[`i2pr-architecture`](.opencode/skills/i2pr-architecture/SKILL.md)
skill is the index for navigating the rest of the documentation.

## Workspace layout

14-crate Rust workspace under `crates/` plus one test-only binary
under `tools/`. Production crates depend upward through the runtime;
test crates and helpers stay on the side.

**Architecture index.** The full ownership map lives in
[`docs/architecture/overview.md`](docs/architecture/overview.md) and
the per-crate deep dives in
[`docs/architecture/i2pr-*.md`](docs/architecture/). The dependency
graph and allowlist live in
[`docs/architecture/dependency-graph.md`](docs/architecture/dependency-graph.md).
The cross-cutting surface (scripts, fixtures, integration lanes, CI,
fuzz) lives in
[`docs/architecture/tooling.md`](docs/architecture/tooling.md). The
NTCP2 interop apparatus lives in
[`docs/architecture/interop-apparatus.md`](docs/architecture/interop-apparatus.md).
Use the
[`i2pr-architecture`](.opencode/skills/i2pr-architecture/SKILL.md)
skill to navigate this surface and to audit doc-vs-source drift.

Quick reference (deep-dive per crate):

- `i2pr-proto` — bounded wire codecs, typed errors, no I/O.
- `i2pr-crypto` — Ed25519/X25519/AES/ChaCha20-Poly1305/HMAC/SipHash/HKDF
  wrappers over reviewed third-party crates.
- `i2pr-storage` — versioned private-identity persistence; NTCP2
  static-key/IV material lives in its own versioned record.
- `i2pr-core` — runtime-neutral service contracts.
- `i2pr-transport`, `i2pr-transport-ntcp2` — runtime-neutral link
  and NTCP2 codec crates. No Tokio, no sockets, no `async fn`.
- `i2pr-netdb`, `i2pr-netdb-persist` — RouterInfo validation, local
  NetDB store, bounded SU3 reseed ingestion, transport-neutral
  query/publication state machines.
- `i2pr-runtime` — **only** production owner of Tokio, sockets,
  timers, channels, and wakeable cancellation.
- `i2pr-daemon` — CLI and composition root. Does not activate NTCP2
  in the production service graph (default `default_ntcp2_enabled =
  false`).
- `i2pr-tunnel` — runtime-neutral exploratory pool, ECIES-X25519
  short tunnel-build cryptography, runtime-neutral tunnel data plane.
- `i2pr-client` — local destination runtime: identity, dedicated
  tunnel pools, signed Standard LeaseSet2 generation/lifecycle,
  ECIES-X25519-AEAD-Ratchet session layer, destination routing,
  I2P Streaming core (`StreamingManager` + `StreamingDestinationAdapter`).
- `i2pr-api` — application-protocol adapter (Plans 136–139 landed
  foundations): SAM 3.1 bounded line/command/reply parser, typed command
  surface, version negotiation, SAM Base64/private-destination codec,
  `SamLimits`, `SamSessionRegistry`, `LineReader`, `ServerConnectionState`,
  the runtime-neutral session dispatch state machine, bounded per-session
  `SamStreamRegistry`, loopback-only `STREAM FORWARD` validation, and local
  `NAMING LOOKUP` resolution. **Plan 142 supersedes the current SAM Base64
  and private-destination compatibility evidence; do not treat the existing
  RFC-4648 implementation as final.** The daemon-only SAM service
  (supervised Tokio listener, per-destination `StreamingManager` pool,
  transactional `SESSION CREATE`, partial STREAM CONNECT / ACCEPT bridge,
  ownership-bound forward registrations, and bounded raw forwarding bridge)
  lives in `i2pr-daemon`. **The live same-socket CONNECT/ACCEPT product bridge
  remains Plan 143 work.** `i2pr-api` depends only on `i2pr-client`,
  `i2pr-crypto`, and `i2pr-proto`.
- `i2pr-testkit` — deterministic simulation; **no production crate
  may depend on it.**
- `tools/i2pr-interop` — non-production test launcher. Must never
  activate `i2pr-daemon`.

## Hard boundaries (enforced by scripts)

These are checked on CI and will reject the change. Fix the boundary;
do not weaken the script.

- **Workspace dependency direction**
  (`scripts/check-dependency-direction.sh`):
  `i2pr-proto <- i2pr-crypto <- i2pr-storage`; `i2pr-core <-
  i2pr-transport <- i2pr-runtime <- i2pr-daemon`;
  `i2pr-transport-ntcp2` consumes `i2pr-crypto`/`i2pr-proto`/
  `i2pr-transport`; `i2pr-netdb` consumes `i2pr-crypto`/
  `i2pr-proto`; `i2pr-netdb-persist` adds `i2pr-storage`;
  `i2pr-client` consumes `i2pr-core`/`i2pr-crypto`/`i2pr-netdb`/
  `i2pr-proto`/`i2pr-tunnel`; **`i2pr-api` consumes `i2pr-client`/
  `i2pr-crypto`/`i2pr-proto`**; `i2pr-daemon` consumes
  `i2pr-api`/`i2pr-client`/`i2pr-crypto`/`i2pr-proto`/...
  (Plan 137). **No production crate depends on
  `i2pr-testkit`.**
- **Runtime boundaries** (`scripts/check-runtime-boundaries.sh`):
  no `unbounded_channel`, no `tokio::*`/`std::net`/`std::fs` in
  transport crates; transport contracts stay synchronous; only
  `i2pr-runtime` and `i2pr-testkit` may list `tokio`/`tokio-util`;
  every `tokio::spawn` keeps an explicit owner.
- **Fixture manifests** (`scripts/check-fixture-manifest.sh`):
  one-to-one mapping between committed `tests/fixtures/i2np/*.hex`
  files and `tests/fixtures/i2np/manifest.tsv`. Re-hash on change.
- **NTCP2 vector manifest** (`scripts/check-ntcp2-vectors.sh`):
  same one-to-one rule for `tests/fixtures/ntcp2/crypto/` plus the
  required vector rows in `vectors.tsv`.
- **NTCP2 interoperability evidence**
  (`scripts/check-ntcp2-interoperability.sh`): the support entry
  stays `status = "experimental"` / `advertised = false`; the daemon
  never registers `ntcp2-transport`; the i2pd direct driver stays
  test-only; the wrapper forbids reseed/SAM/I2CP/HTTP fallback.
- **Rootless interop**
  (`scripts/check-rootless-interop-boundary.sh`): the rootless
  topology may not gain `sudo`/`ip netns`/`nft`/`setcap`/`--privileged`/
  `--network host` or silent fallback to the privileged backend.
- **Multipass interop**
  (`scripts/check-multipass-interop-boundary.sh`): Multipass
  lifecycle scripts may not issue global `multipass purge` or
  change host policy.
- **Constrained-host lane**
  (`scripts/check-constrained-host-lane-boundary.sh` + focused
  `tests/integration/ntcp2/harness/test_execution_lane.py`).

Non-production crates (`i2pr-testkit`, `tests/integration/ntcp2/`,
`tools/i2pr-interop`, `fuzz/`) live outside the production
dependency graph and are checked in-place.

## Build, test, lint

Toolchain is pinned to Rust 1.95.0 via `rust-toolchain.toml`; MSRV
is 1.88 (verified by a dedicated CI job). Workspace edition is
2024; `max_width = 100` (`rustfmt.toml`). `cargo deny` checks
advisories, bans, and sources (the license allow-list is
intentionally empty until project-owner selection).

Run from the repo root, in this order, before handoff:

```text
cargo fmt --all --check
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh   # when I2NP fixture bytes change
bash scripts/check-ntcp2-vectors.sh      # when NTCP2 vector bytes change
bash scripts/check-ntcp2-interoperability.sh   # when ntcp2 evidence/manifest change
bash scripts/check-rootless-interop-boundary.sh   # when rootless files change
bash scripts/check-multipass-interop-boundary.sh  # when Multipass lifecycle files change
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py'
bash scripts/fuzz-smoke.sh               # opt-in; requires cargo-fuzz + nightly
```

CI also runs `cargo deny check advisories bans sources` and `cargo
test --locked` everywhere. Pass `--locked` when you reproduce CI
locally; `Cargo.lock` is authoritative.

### Focused test lanes

- Transport contract: `cargo test -p i2pr-transport --all-targets`,
  `cargo test -p i2pr-transport-ntcp2 --all-targets`.
- Runtime supervision: `cargo test -p i2pr-runtime --all-targets`.
  The forced-cleanup 100-iteration test must run serially:
  `cargo test -p i2pr-runtime forced_child_cleanup_is_repeatably_joined -- --test-threads=1`.
- Deterministic testkit: `cargo test -p i2pr-testkit --all-targets`.
- SAM 3.1 protocol foundation/corrections: `cargo test -p i2pr-api --all-targets`.
- SAM loopback product lane: use the current `i2pr-daemon` SAM integration
  tests; Plan 143 must add/identify the canonical real raw STREAM product test.
- Constrained-host lane:
  `python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py'`.

### Testing conventions that differ from defaults

- Runtime tests must use `#[tokio::test(start_paused = true)]` or the
  `i2pr-testkit` `ManualClock` with fixed seeds and bounded steps.
- **No wall-clock sleeps, no DNS, no public-network traffic** in
  tests. Runtime-owned socket tests may use loopback only; every
  other transport check goes through the testkit or an explicitly
  authorized private network.
- Tests for bounded queues and resource governors must cover
  capacity-of-one, exact offered load, and max-plus-one offered load;
  verify the typed full, deadline, cancellation, closure,
  response-drop, and resource-denial outcomes; confirm queue-held
  leases release on every drop path.
- Committed protocol fixtures must be sanitized, locally authored or
  provenance-recorded, free of private keys/live identities/
  addresses/destinations, and listed in `tests/fixtures/*/manifest.tsv`
  with classification, expected type/error, exact source revision,
  generator, license note, SHA-256, and independence status.
  Fixture-backed tests must consume the bytes.
- Secret-bearing protocol values live in narrow non-cloneable,
  zeroizing owners with redacted `Debug`. Memory hygiene does not
  imply encrypted-protocol support.
- Channel closure must be treated as a lifecycle event, never
  retried blindly.

## Coding conventions

- Workspace lints (`Cargo.toml`): `unsafe_code = "deny"`,
  `unexpected_cfgs = "deny"`, `unused_must_use = "warn"`; clippy
  denies `dbg_macro`, `todo`, `unimplemented`.
- `#![forbid(unsafe_code)]` is the default for protocol, crypto,
  routing, NetDB, tunnel, client, API, and service crates. Any
  required `unsafe` must be isolated, documented, tested, and
  reviewed separately.
- Treat all external input (network, SAM, I2CP, configuration,
  RouterInfo/LeaseSet reloads, reseed) as hostile: explicit
  bounds, reject unknown or trailing bytes, no validation side
  effects, always test the negative path.
- Codec errors are typed enums; never swallow codec results. Don't
  encode router policy into wire codecs.
- NTCP2 static key/IV material lives in the separate versioned
  `i2pr-storage` record — never derive from or overwrite the router
  identity record. The forbidden nonce `2^64 - 1` is never emitted.
- Use OS CSPRNG through reviewed interfaces for production
  cryptography; deterministic RNG is allowed only in tests,
  simulation, and explicitly marked reproducibility tools.
- Library crates avoid `anyhow` as a public error model. Use typed
  error enums with stable categories.
- Workspace dependency versions are centralized in `Cargo.toml`;
  duplicate major versions need a review. New dependencies must
  record purpose, alternatives, maintainer health, transitive
  impact, license, unsafe exposure, and whether they process
  untrusted input.
- Avoid global mutable state and unrestricted `Arc<RouterContext>`
  service locators. Each subsystem should receive narrow handles
  or capabilities.

## Architecture decisions

All architecture/security decisions live in `docs/adr/` (`0001`
through `0025`). Plan-of-record is the active `plans/NNN-*.md`
plus its closure record (`plans/NNN-status.md`). When you close a
milestone, leave an explicit closure document with commands,
results, and evidence. Plans retain their history in `plans/`;
treat per-plan narratives as audit records, not as live contracts.

## Protocol support and NTCP2

Every protocol surface is tracked in `specs/support.toml` (mirrored
to `docs/protocol-support.md`). Entries default to
`status = "experimental"` and `advertised = false`. Setting
`advertised = true` requires interoperability evidence per
`specs/CONFORMANCE.md`; namespace presence is not evidence. The
current public state:

- NTCP2: experimental, non-advertised; daemon NTCP2 activation is
  disabled and unenableable through the public surface. The NTCP2
  development interop result is `protocol-defect-localized` at
  `noise_authenticated`. No passed mixed-router NTCP2 result
  exists.
- Milestone 6 local product gate (destinations, garlic, LeaseSet2,
  streaming) closed locally via Plan 134; independent-router
  interoperability is **not** claimed and is tracked as external
  acceptance debt.
- The current product layer is SAM 3.1 under the Plan 141 corrective
  roadmap. Plan 137 loopback server/session lifecycle remains passed.
  Plan 136's SAM encoding/private-destination evidence is superseded by
  Plan 142; Plan 138's product STREAM acceptance is superseded by Plan 143;
  Plan 139 FORWARD/naming implementation remains landed but its real-byte
  acceptance is re-run in Plan 144. Plan 140 is the blocked audit record,
  not the next executable plan.

When you read a `plan_NNN`-style token in code or docs, treat the
newest explicit superseding status record as authoritative and the per-plan
narrative as historical context. The active development interop surface is
small and bounded; do not extend it without a new plan-of-record.

## Interop and external evidence lanes

Reference revisions are pinned in `tests/integration/ntcp2/manifest.toml`
and `tests/integration/ntcp2/references.lock.toml`:

- Java I2P 2.12.0 at `2800040deee9bb376567b671ef2e9c34cf3e30b6`
- i2pd 2.60.0 at `f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e`

Do not change a pinned revision without updating the lock file, the
manifest, and the static boundary check.

The evidence harness is restricted to `tests/integration/ntcp2/`,
`scripts/interop/`, and the corresponding GitHub Actions workflows
under `.github/workflows/`. The active lanes are:

- **Local conformance** — `i2pr-testkit`, fixture-driven, no
  network. This is the routine development path.
- **Host-loopback smoke** — Plan 086/099/100 development-only
  lane. Loopback processes on the host. Wrapper:
  `scripts/interop/run-minimal-i2pd-host-loopback-probe.py`.
- **Rootless sealed namespace** — Plan 046, with the
  `rootless-sealed-single-netns` topology; on this host the probe
  returns the typed blocker `blocked_unprivileged_user_namespace`.
- **Multipass recovery guest** — Plan 048/049/050/051. The host
  policy is never changed; the disposable guest is permissive by
  design.

Mixed-router results require sanitized typed JSON plus digests under
`target/interop/evidence/`; secret-bearing run roots are deleted. A
typed blocker, reference-only result, parser-only result, or testkit
result is **not** mixed-router evidence. Load the matching skill from
the table below before touching a lane.

## OpenCode skills

The repository ships loadable skill bundles under
[`.opencode/skills/`](.opencode/skills/). Load the matching skill
before touching the surface it covers.

| Skill | Covers |
| --- | --- |
| [`i2pr-local-dev`](.opencode/skills/i2pr-local-dev/SKILL.md) | Work on the local product path of the i2pr Rust I2P router — Milestone 6 (destinations, garlic, LS2, Streaming) and Milestone 7 SAM. Plan 141 is the current corrective authority; execute Plan 142 next. |
| [`i2pr-architecture`](.opencode/skills/i2pr-architecture/SKILL.md) | Navigate `docs/architecture/`, `docs/adr/`, `plans/`, and `specs/`; audit doc-vs-source drift. |
| [`i2pr-ntcp2-interop`](.opencode/skills/i2pr-ntcp2-interop/SKILL.md) | Host-side Plan 038 NTCP2 reference-router harness. The active development interop lane is **closed**; NTCP2 remains experimental and non-advertised. |
| [`i2pr-rootless-sandbox`](.opencode/skills/i2pr-rootless-sandbox/SKILL.md) | Plan 046 rootless sealed-namespace lane (host-side fallback). On this host returns the typed blocker `blocked_unprivileged_user_namespace`. |
| [`i2pr-multipass-recovery`](.opencode/skills/i2pr-multipass-recovery/SKILL.md) | Plan 048/049/050/051 Multipass recovery guest (canonical external lane). The host policy is never changed. |

The NTCP2 interop lane is closed; load `i2pr-local-dev` for routine
work and `i2pr-architecture` to navigate the documentation.

## Commits and pull requests

- Focused imperative subjects, e.g. `docs: streamline repository
  guidelines`, `transport: bound ntcp2 data frame owners`.
- PRs document scope, changed files, test commands and results,
  dependency or security decisions, deviations, and known
  limitations. Milestone closures attach the closure record with
  the evidence.
- Don't update git config, skip hooks, force-push, or amend
  someone else's commit. If a hook rejects, fix the issue and add
  a new commit.

## Directory orientation

- `crates/` — workspace crates.
- `tools/i2pr-interop/` — non-production interop launcher.
- `tests/fixtures/` — committed fixtures, governed by manifest
  scripts.
- `tests/integration/ntcp2/` — interop harnesses, scenarios,
  qualification, reference drivers, evidence bundles.
- `tests/integration/sam/` — lightweight Plan 142/144 independent-client
  provenance and localhost evidence; it does not replace the canonical Rust
  Plan 143 product lane.
- `fuzz/` — opt-in nightly fuzz workspace; not part of the
  production workspace.
- `docs/adr/` — architecture decision records.
- `docs/architecture/` — architecture deep dives per crate and
  topic.
- `docs/protocols/`, `docs/security-model.md`,
  `docs/private-testnet.md` — protocol, security, and private
  testnet references.
- `specs/` — support ledger, conformance, sources, protocol
  dossiers, implementation notes.
- `plans/` — plan-of-record and closure records (read the
  relevant status file before changing code).
- `scripts/` — static boundary and interop scripts.
- `.github/workflows/` — CI and the optional historical Plan 095 manual
  live-wire workflow (manual `workflow_dispatch` only; never on pull
  requests). Its retired checker is not part of the current local gate.
- `.opencode/skills/` — loadable skill definitions for OpenCode
  sessions.