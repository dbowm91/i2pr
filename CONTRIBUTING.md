# Contributing to i2pr

`i2pr` is an experimental, clean-room Rust router project. Contributions must
preserve `GUARDRAILS.md`, the active plan, and the architecture decisions in
`docs/adr/`.

## Before changing code

Read, in order:

1. `README.md`.
2. `GUARDRAILS.md`.
3. The relevant roadmap or detailed plan under `plans/`.
4. Relevant ADRs and protocol dossiers.

Protocol changes require a plan covering acceptance criteria, limits, negative
tests, dependency changes, security implications, sources, and documentation.
Do not add empty future crates or claim interoperability without evidence.
The current common-structure subset in `i2pr-proto` is structural only: keep
signed byte regions intact, use the pinned source ledger, and leave freshness
policy, transport interpretation, and LeaseSet2-family behavior to their later
plans. Plan 013's concrete Ed25519/X25519 wrappers and private identity store
must remain outside `i2pr-proto`; update ADRs and the support ledger when
crypto/storage scope changes.

## Local quality checks

Run from the repository root:

```text
cargo fmt --all --check
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
bash scripts/check-dependency-direction.sh
cargo deny check advisories bans sources
```

The CI matrix covers Linux and macOS. Dependency downloads may require network
access in restricted environments; do not weaken checks to work around that.

Runtime supervision tests belong in `i2pr-runtime` and must use
`#[tokio::test(start_paused = true)]` or explicit `tokio::time::advance` for
deadlines and restart backoff. Exercise cancellation before and during waits,
readiness, panic classification, restart exhaustion, graceful shutdown, forced
abort, child-scope cleanup, and the zero-remaining-task report. Do not use
wall-clock sleeps or live sockets in this milestone. Run the focused lane with
`cargo test -p i2pr-runtime --all-targets` in addition to the workspace checks.
The corrective lane also covers forced manager abort with exact child-scope
drain, uncancelled `RequestedShutdown` classification, typed cleanup evidence,
and a deterministic repeated forced-cleanup test. Run the 100-iteration
focused test with `cargo test -p i2pr-runtime forced_child_cleanup_is_repeatably_joined -- --test-threads=1`.

Bounded communication and resource-governor tests must cover capacities of one,
exact offered load, and maximum-plus-one offered load. Test typed full,
deadline, cancellation, closure, response-drop, and resource-denial outcomes;
verify queue-held leases release on receive, drop, timeout, cancellation,
panic unwind, and supervisor teardown; and exercise atomic bundle denial with
no partial usage. Use deterministic Tokio time and explicit reproducibility
seeds where scheduling or accounting is exercised. Do not use wall-clock
sleeps, unbounded retry loops, or live network traffic for overload tests.

Run `bash scripts/check-fixture-manifest.sh` after changing committed fixture
bytes. The maintained fuzz workspace under `fuzz/` is intentionally outside
the production workspace and requires nightly `cargo-fuzz`; use
`bash scripts/fuzz-smoke.sh` for bounded local smoke runs. Fuzz-only
dependencies must not be added to production manifests.

Plan 023 deterministic simulation tests use `i2pr-testkit`'s fixed seed matrix:
the zero seed, the all-ones seed, and named regression seeds. Reproduce a
failure by recording the seed and scenario identifier, then compare the
privacy-safe `ReplayRecord` from two runs. Use manual clock advancement and
`run_until_idle(max_steps)`; do not add wall-clock sleeps, OS-random seeds,
real sockets, DNS, or public-network fault injection. The focused lane is:

```text
cargo test -p i2pr-testkit --all-targets
```

Plan 031 transport contract tests are runtime-neutral and remain below the
socket and wire-cryptography boundary:

```text
cargo test -p i2pr-transport --all-targets
cargo test -p i2pr-transport-ntcp2 --all-targets
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
```

Plan 032's `i2pr-transport-ntcp2` work is still runtime-neutral. Keep reviewed
primitive crates behind the protocol-specific wrappers; do not add a generic
Noise/provider API, Tokio, sockets, or filesystem access. Preserve consuming
transcript transitions, the retained SessionRequest cipher state used by
SessionConfirmed part one, checked nonce bounds, all-zero X25519 rejection,
and redacted secret owners. Persist NTCP2 static key plus IV material only
through `i2pr-storage`'s separate versioned create-only record.

When changing NTCP2 fixtures, run:

```text
cargo test -p i2pr-transport-ntcp2 --all-targets
cargo test -p i2pr-storage --all-targets
bash scripts/check-ntcp2-vectors.sh
```

Plan 033 handshake changes must also exercise strict message-1/2/3 lengths,
reserved options, padding bounds, timestamp boundaries, replay decisions,
RouterInfo signature/static-key binding, consuming state transitions, and
bounded simulated partial-I/O actions. Keep those tests in the pure NTCP2
crate or the deterministic testkit; do not add Tokio, sockets, wall-clock
sleeps, public-network traffic, or capability claims.

Plan 034 data-phase changes must exercise authenticate-before-parse behavior,
zero/minimum/maximum/maximum-plus-one lengths, strict block headers, unknown
and duplicate blocks, terminal ordering, tag mutation, counter exhaustion,
RouterInfo signature/static-key binding, and bounded I2NP handoff. Use the
deterministic testkit drivers for one-byte reads/writes, split lengths,
multi-frame buffering, truncation, cancellation, backpressure, and exact
buffer/lease teardown. Add a locally authored fixture row for every committed
vector or malformed corpus file and run the NTCP2 vector-manifest check.
The current specification has no data-phase rekey threshold; do not invent an
in-session rekey wire message. Counter exhaustion or static-key/IV rotation
requires a fresh Noise handshake until a later compatibility plan says more.

The focused Plan 034 lane is:

```text
cargo test -p i2pr-transport-ntcp2 --all-targets
cargo test -p i2pr-testkit --all-targets
bash scripts/check-ntcp2-vectors.sh
cargo +nightly check --manifest-path fuzz/Cargo.toml --offline --all-targets
```

The independent deterministic corpus is local crypto evidence only. Do not
describe it as Java I2P/i2pd interoperability or use it to advertise NTCP2;
that evidence belongs to the authorized later interoperability plan.

Transport code must pass bounded encoded-I2NP owners through explicit consuming
handoffs, retain only typed delivery and termination outcomes, and keep peer
references, addresses, keys, transcripts, and payload bytes out of default
debugging and snapshots. `i2pr-runtime` remains the sole production owner of
Tokio tasks, sockets, timers, channels, and wakeable cancellation; transport
crates must not grow async traits or plugin registries.

Plans 035 and 037 define the controlled runtime TCP lane. Keep address parsing
and protocol state machines synchronous; only `i2pr-runtime` may own TCP
sockets, async I/O, deadlines, replay caches, admission counters, bounded
channels, queued-frame accounting, and joined reader/writer children. An
accepted inbound stream must carry its pending admission owner through the
handshake; authenticated registration must transition to active-link ownership
without a gap. Actual frame reads and writes use the configured deadline and
cancellation policy, and queue entries release item/byte accounting on every
drop path. The general data-phase parser must remain separate from the stricter
SessionConfirmed parser. These are corrective integration requirements, not a
claim that the complete adapter or external interoperability exists.

Tests may bind loopback addresses and use paused Tokio time, but must never
contact public I2P peers. Exercise per-IP and IPv4/IPv6 subnet admission,
partial reads/writes, replay expiry/capacity, backoff and cancellation,
queue/byte denial, duplicate replacement with stale-close protection,
sibling-child teardown, parser sequence boundaries, and redacted diagnostics.
Runtime address observations are candidates for later policy only: they must
not mutate NetDB, RouterInfo, or publication state. Run the focused lane with:

```text
cargo test -p i2pr-runtime --all-targets
cargo test -p i2pr-transport --all-targets
cargo test -p i2pr-transport-ntcp2 --all-targets
cargo test -p i2pr-testkit --all-targets
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
```

The Plan 037 evidence boundary remains fail-closed: local self-integration and
testkit results are not Java I2P or i2pd evidence, and no daemon activation or
support advertisement follows from them. Mixed-router evidence requires the
authorized private-testnet procedure in `docs/private-testnet.md`.

### Plan 044 mixed-runner responsibilities

The mixed runner composes `I2prAdapter` with each reference adapter through
a strict launcher scenario renderer. Each directional scenario file under
`tests/integration/ntcp2/mixed-scenarios/` declares a unique direction
(`i2pr-to-reference` or `reference-to-i2pr`), one initiator and one responder,
and an IPv4 address family. The runner expands each entry into an independent
execution with its own namespace pair, firewall policy, startup order, and
evidence record.

The data-phase oracle uses a protocol-valid trigger supported by both pinned
references; it does not rely on an echo assumption. Evidence records carry
real counters for authenticated-link count, frames sent/received, I2NP
message aggregates, admission/replay counters, process lifecycle counters,
and cleanup disposition.

To add a new directional scenario:

1. Add an entry to `tests/integration/ntcp2/mixed-scenarios/manifest.toml`
   with a unique `id`, the correct `reference`, `direction`, `initiator`,
   and `responder`.
2. Ensure the corresponding scenario TOML file exists under
   `tests/integration/ntcp2/mixed-scenarios/`.
3. The strict launcher renderer must reject absolute paths, parent traversal,
   endpoints outside synthetic namespace ranges, mismatched address families,
   missing peer data for initiators, peer data for responders, and unknown
   fields.
4. The aggregate manifest must include the new scenario in its expected
   scenario set; missing records fail the gate.

Plan 038 adds a manual, Ubuntu-only harness contract for that procedure. Keep
the two phases separate: host preparation may install declared Ubuntu
packages, fetch pinned reference sources, and build/hash artifacts; scenario
execution must run in disposable Linux namespaces with only the scenario veth
pair, no default route, no DNS, and no public egress. Isolation is checked
before launch and cannot be disabled by a scenario option. The entry
points are:

```text
bash scripts/interop/ubuntu/check-host.sh --pre-install
bash scripts/interop/ubuntu/setup-host.sh
bash scripts/interop/ubuntu/check-host.sh --post-install
bash scripts/interop/build-references.sh
bash scripts/interop/build-references.sh --offline
bash scripts/interop/run-scenario.sh --scenario <id> --reference java_i2p --build-cache <path> --run-root <path>
bash scripts/interop/run-scenario.sh --scenario <id> --reference i2pd --build-cache <path> --run-root <path>
bash scripts/interop/run-matrix.sh --profile environment-smoke
```

The harness must distinguish environment smoke (reference startup and
cleanup), reference crosscheck (Java I2P versus i2pd, with no i2pr claim), and
i2pr mixed-router evidence (bounded authenticated runs between i2pr and each
reference in both directions). Sanitize before retention: keep only typed
outcomes, bounded run metadata, and hashes; delete raw addresses, identities,
RouterInfo, I2NP, keys, transcripts, logs, and arbitrary remote error text.
None of these profiles activates the normal daemon or justifies an NTCP2
support or capability claim without the conformance evidence requirements.

### Plan 043 build-system responsibilities

The privileged lane has explicit ordered gates:

```text
contract -> reference-build -> reference-offline-reuse -> environment-smoke
-> reference-crosscheck-ipv4 -> i2pr-handshake-smoke-ipv4 -> full-matrix
-> evidence-validation -> cleanup-verification
```

Ordinary CI owns the unprivileged Rust, static, unit, deterministic, fixture,
manifest, and boundary checks. The Ubuntu lane is manual and opt-in. Only the
preparation gate may install the exact lock-listed packages or fetch the full
locked reference sources; all later gates consume verified offline caches and
synthetic namespace-local links. A cache miss, digest mismatch, isolation
failure, or reference-control failure stops promotion.

Contributors must not use workflow inputs for shell fragments, source URLs,
revisions, endpoints, network IDs, or arbitrary paths. The reference control
must complete before i2pr/reference scenarios. A valid i2pr result requires all
four independent IPv4 directions, authenticated handshake and bounded
DeliveryStatus exchange, typed counters, sanitized finalization, and clean
state. A local launcher, loopback socket, reference-only pair, or listener
readiness event is not mixed-router evidence.

Evidence validation consumes an aggregate manifest and rejects missing or
unexpected passed records, placeholders, hash disagreement, incomplete
direction coverage, forbidden content, or non-clean cleanup. Retain only the
narrow sanitized JSON/hash allowlist. Cleanup must run on every privileged
failure path and at the end; the independent clean-host verifier is a required
terminal gate. A protocol pass cannot override cleanup failure.

Plan 043 promotion is manual first, scheduled only after repeated clean-
checkout and cache-reuse success, then a current successful run at Milestone 3
closure. Any trusted pull-request lane requires a separate decision and must
not expose privileged execution to forked or untrusted code. The current lane
has no mixed-router i2pr evidence and remains experimental/non-advertised.

### Plan 046 rootless sealed-namespace responsibilities

Plan 046 replaces the host-global namespace requirement for the primary
NTCP2 interoperability evidence path with a rootless, process-scoped
sandbox. The primary evidence topology is `rootless-sealed-single-netns`
with privilege model `unprivileged-userns`. The legacy
`privileged-dual-netns-veth` topology is preserved as an explicit later
qualification lane; it is never the default and never a silent fallback.

When changing rootless code paths:

1. Do not introduce `sudo`, `setcap`, setuid helpers, file capabilities,
   ambient host capabilities, `--privileged` containers, `--network host`
   containers, privileged sidecars, `ip netns add`, host link mutation,
   host route mutation, or host nftables mutation. The static boundary
   checker `scripts/check-rootless-interop-boundary.sh` enforces this on
   every change.
2. Verify the single-ID UID/GID mapping with `setgroups deny` and
   `no_new_privs`. Broader maps or missing denials are typed blockers.
3. Use only the synthetic `192.0.2.{1,2}/32` addresses (and optional
   `2001:db8:36::{1,2}/128`) inside the sandbox. No default route, no host
   interface, no forwarded port, no public-network path.
4. Extend the mixed-router evidence schema with `topology_kind`,
   `privilege_model`, `sandbox_attestation_sha256`, and
   `parent_network_state_unchanged`. A passed record that violates any of
   these is rejected.
5. Treat a typed probe blocker such as
   `blocked_unprivileged_user_namespace`, `blocked_loopback_unconfigured`,
   `blocked_synthetic_bind_failed`, or `blocked_external_connect_succeeded`
   as a hard stop. Never fall back to the privileged backend silently.
6. Keep the outer entrypoint (`scripts/interop/rootless-enter.sh`)
   shell-`eval`-free and allowlist-only. Never remove the
   `I2PR_INTEROP_ROOTLESS_INNER=1` requirement or the explicit
   topology-kind check before exec.
7. Add negative-path tests for any new topology, sandbox, attestation, or
   probe outcome. The 40 tests under `test_rootless_topology.py` are the
   starting point.
8. The rootless lane does not install system packages. It verifies
   dependencies and emits a typed blocker when the environment is
   incomplete. Preparation (network-enabled) and execution (offline)
   remain separate.

Run the full validation ladder before handoff (see `AGENTS.md`). Plan 046
does not advertise NTCP2 support and does not close Milestone 3 by
itself; do not describe the gate chain as passing or present reference-
only control records as i2pr mixed-router evidence.

Plan 024's integrated lane contains named clean-startup, bounded-overload,
restart-recovery, essential-failure, and simulated-link-fault scenarios plus
a fixed 32-seed replay matrix. Run it with paused Tokio time and the manual
clock; failures must retain the printed scenario and seed context. The
mechanical boundary lane is:

```text
bash scripts/check-runtime-boundaries.sh
```

CI also runs `cargo check --workspace --all-targets` and, on Linux, both the
runtime-boundary and fixture-manifest gates. The fixture gate enforces a
one-to-one mapping between committed `.hex` files and manifest rows.

Runtime snapshots are aggregate, eventually coherent observations assembled
without awaiting. Lower crates may emit the documented fixed-name tracing
events but must not install a subscriber. Event fields must stay within the
allowlist in `docs/security-model.md`; do not log health detail text, payloads,
identity/destination encodings, addresses, panic payloads, or dynamic
peer-derived labels.

Simulation assertions must include bounded pending deliveries and bytes,
receiver backpressure, cancellation/deadline outcomes, and teardown snapshots
for queued units, timers, and resource leases. Link leases are owned by the
live endpoint handles and must be dropped explicitly when a test expects zero
active links.

Committed protocol fixtures must be sanitized, locally authored or provenance-
recorded, free of private keys/live identities/addresses/destinations, and
listed with classification, expected type or error category, exact source
revision, generator/input, license note, SHA-256, and independence status.
Fixture-backed tests must consume the bytes. Secret-bearing protocol values
must use narrow non-cloneable, zeroizing owners with redacted `Debug`; memory
hygiene does not imply encrypted-protocol support.

## Security and testing

Treat all external input as hostile. Add malformed, boundary, cancellation,
cleanup, and resource-exhaustion tests alongside successful-path tests. Use the
manual clock and reproducibility seeds from `i2pr-testkit` for deterministic
state-machine tests. Public-network testing must be passive and ordinary;
stress, mutation, malformed-traffic, and adversarial tests belong only in an
authorized isolated testnet.

Report security issues privately to the project owner rather than publishing
exploit details in an issue or pull request. Treat router identity files and
backups as private key material; do not add private fixtures or print secret
bytes in tests and diagnostics.

Identity directories must be created with restrictive permissions from
inception. A post-create permission change is not an acceptable substitute;
when recursive creation cannot be made safe, require an existing secure
parent and document that policy.

## Dependencies, provenance, and commits

Keep dependencies focused, centralize workspace versions, review transitive
impact and unsafe-code exposure, and record why a new dependency is needed.
Do not copy implementation code or test vectors from another router until
license and provenance review is complete. The project license is intentionally
not selected yet.

Make focused commits that explain behavior and tests. Handoffs should list
changed files, commands and results, dependency changes, security-relevant
decisions, deviations, and remaining risks.
