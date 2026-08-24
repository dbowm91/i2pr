# Repository Guidelines

`i2pr` is an experimental Rust I2P router. It is not production-ready and must
not be used for anonymity or security-sensitive workloads. Read `README.md`,
`GUARDRAILS.md`, `CONTRIBUTING.md`, the applicable `plans/` document, and
relevant `docs/adr/` records before changing code.

## Workspace Layout

Eleven-crate workspace under `crates/`:

- `i2pr-proto` — bounded wire codecs (crate-root façade, borrowed cursors, strict decoding, typed errors)
- `i2pr-crypto` — Ed25519/X25519/AES/ChaCha20-Poly1305/HMAC/SipHash wrappers
- `i2pr-storage` — versioned persistence; identity, NTCP2 static-key records, and Plan 104 raw-byte cache seam
- `i2pr-core` — runtime-neutral service contracts
- `i2pr-netdb` — runtime-neutral RouterInfo validation, local NetDB store, SU3/reseed verification, local signed RouterInfo construction, and transport-neutral query/publication state machines (Plan 103/104/105/119; consumes `i2pr-crypto`/`i2pr-proto` + reviewed third-party crates)
- `i2pr-netdb-persist` — composition owner for Plan 104 persistent cache and SU3 reseed ingestion
- `i2pr-transport` — runtime-neutral link/delivery contracts (no Tokio, no I/O)
- `i2pr-transport-ntcp2` — runtime-neutral NTCP2 handshake + data frames
- `i2pr-runtime` — **only** production owner of Tokio tasks, sockets, timers, channels, wakeable cancellation
- `i2pr-daemon` — composition root + CLI
- `i2pr-client` — Plan 120 local destination runtime: identity, dedicated tunnel pools, signed Standard LeaseSet2 generation and lifecycle, bounded local payload contracts, destination registry (consumes `i2pr-core`/`i2pr-crypto`/`i2pr-netdb`/`i2pr-proto`/`i2pr-tunnel`; never composes back into `i2pr-tunnel` or `i2pr-netdb`; never depends on `i2pr-daemon`); Plan 122 local destination routing / LeaseSet2 NetDB composition; Plan 123 minimal I2P Streaming core (`StreamingManager`, signed SYN/CLOSE/RESET, sequence/ACK/NACK, retransmit, congestion, send/receive windows, listener backlogs).
- `i2pr-testkit` — deterministic simulation; production crates must not depend on it

Fixtures: `tests/fixtures/i2np/` (manifest at `tests/fixtures/i2np/manifest.tsv`),
`tests/fixtures/ntcp2/crypto/` (manifest at `…/manifest.tsv`). Opt-in nightly
fuzz workspace at `fuzz/`.

## Hard Boundaries (enforced by scripts)

These are checked on CI and will reject the change:

- Dependency direction (`scripts/check-dependency-direction.sh`):
  `i2pr-proto <- i2pr-crypto <- i2pr-storage`; `i2pr-core <- i2pr-transport
  <- i2pr-runtime <- i2pr-daemon`;   `i2pr-transport-ntcp2` consumes
  `i2pr-crypto`/`i2pr-proto`/`i2pr-transport`; `i2pr-netdb` consumes
  `i2pr-crypto`/`i2pr-proto` + reviewed third-party crates (Plan 103/104/105);
  `i2pr-netdb-persist` consumes `i2pr-crypto`/`i2pr-netdb`/`i2pr-proto`/`i2pr-storage`
  (Plan 104); `i2pr-client` consumes `i2pr-core`/`i2pr-crypto`/
  `i2pr-netdb`/`i2pr-proto`/`i2pr-tunnel` (Plan 120); and `i2pr-runtime`
  may compose `i2pr-transport-ntcp2` for Plan 042. **No production crate
  may depend on `i2pr-testkit`.**
- Runtime boundaries (`scripts/check-runtime-boundaries.sh`):
  - No `unbounded_channel`, `UnboundedSender`, `UnboundedReceiver` in
    `i2pr-runtime`/`i2pr-testkit`/`i2pr-transport`/`i2pr-transport-ntcp2`.
  - No `tokio::*`, `std::net`, `std::fs`, `TcpStream`, `TcpListener`, etc.
    in `i2pr-transport`/`i2pr-transport-ntcp2`.
  - No `async fn`/`async_trait`/`i2pr-netdb|tunnel|client` in transport
    contracts (they stay synchronous).
  - Only `i2pr-runtime` and `i2pr-testkit` may list `tokio`/`tokio-util` deps.
  - `tokio::spawn` calls must keep an explicit owner (bound to `let`,
    `push(`, or `JoinSet`).
- NTCP2 interoperability (`scripts/check-ntcp2-interoperability.sh`):
  evidence must stay sanitized; the manifest under
  `tests/integration/ntcp2/manifest.toml` must list exactly eight bounded
  scenarios with the required disclaimer lines.
- Rootless interop boundary (`scripts/check-rootless-interop-boundary.sh`):
  rootless-owned files (`scripts/interop/rootless-enter.sh`,
  `scripts/interop/probe-rootless-sandbox.sh`,
  `tests/integration/ntcp2/harness/rootless_supervisor.py`,
  `tests/integration/ntcp2/harness/rootless_topology.py`,
  `tests/integration/ntcp2/harness/rootless_inner_runner.py`,
  `tests/integration/ntcp2/harness/interop_topology.py`, and
  `.github/workflows/ntcp2-interop-rootless.yml`) must contain no
  `sudo`, `ip netns`, `nft`, `setcap`, `--privileged`, `--network host`,
  or fallback to the privileged backend. The checker enforces the gate
  catalog and the sandbox-attestation requirement in the evidence module.

If a check fails, fix the boundary, don't suppress the script.

Plan 042 is the active runtime-owned NTCP2 wire-driver plan. Plan 044
composed the mixed-router execution model, directional scenario expansion,
strict launcher rendering, and the non-echo data-phase oracle. Plan 045
is the active mixed-router closure plan and supersedes Plan 044 for
closure purposes: it closes the ten Plan 045 defects (D1–D10) that
invalidate Plan 044's prior "implementation-complete locally" status.

Plan 046 is the active rootless sealed-namespace evidence lane. It
replaces the host-global namespace requirement with a rootless,
process-scoped user/network sandbox that an ordinary user can run. The
primary mixed-router evidence topology is `rootless-sealed-single-netns`
with privilege model `unprivileged-userns`. The legacy
`privileged-dual-netns-veth` topology is renamed, kept as an explicit
opt-in qualification lane, and never used as a silent fallback. The
plan introduces:

- a topology backend contract (`tests/integration/ntcp2/harness/interop_topology.py`)
  with `ProcessPlacement`, `InteropTopology`, and a topology
  registry;
- a rootless inner supervisor
  (`tests/integration/ntcp2/harness/rootless_supervisor.py`) that
  verifies single-ID UID/GID maps, `no_new_privs`, distinct user,
  network, mount, and PID namespaces, loopback readiness, synthetic
  address binding, and the absence of external routes;
- a rootless sealed topology
  (`tests/integration/ntcp2/harness/rootless_topology.py`) that the
  adapters consume through `select_topology` and `ProcessPlacement`;
- the no-escalation outer entrypoint (`scripts/interop/rootless-enter.sh`)
  and a typed sandbox capability probe
  (`scripts/interop/probe-rootless-sandbox.sh`);
- a sandbox attestation record and parent-network state equivalence
  requirement for every passed mixed-router record;
- the static rootless boundary checker
  (`scripts/check-rootless-interop-boundary.sh`);
- the no-escalation GitHub Actions workflow
  (`.github/workflows/ntcp2-interop-rootless.yml`);
- ADR 0017 and the reconciliation of every relevant design document.

Plan 046 closed with a typed host-level blocker. The closure record is
`plans/046-closure.md`. Plan 052 is the active Milestone 3 evidence
closure follow-up; Plan 053 is the active corrective pass for
integrating Plan 052 into the canonical execution path. The closure is the existence of a re-producable
typed probe blocker that any ordinary user can produce on this host. The
on-host evidence at
`target/interop/evidence/handshake-smoke-rootless--host-blocked/`
contains a kernel/sysctl/capability snapshot and two identical probe
attestations (host shell and `ssh i2ptest@localhost` shell) carrying
the canonical typed blocker
`blocked_unprivileged_user_namespace`. The lane remains runnable by an
ordinary user; it just returns a typed blocker on this particular
kernel configuration. Cross-host portability is deferred to
`plans/047-cross-host-rootless-lane-expansion.md`.

- D1: ``ref-gen``/``ref`` and ``i2pr-gen``/``i2pr`` share one disposable
  data directory so the live phase restarts from the identity that
  produced the exported RouterInfo.
- D2: the Rust launcher persists RouterInfo inside ``state_dir``; the
  mixed-runner exports the bytes from there and records real SHA-256
  digests.
- D3, D6: the strict launcher scenario schema allows the allowlisted
  optional fields ``data_phase_mode``,
  ``data_phase_required_peer_action``, ``data_phase_timeout_ms``,
  ``expected_observation``; the Rust launcher parses the same schema
  and dispatches typed ``DataPhaseMode`` variants.
- D4: the reference trigger performs the per-direction SAM v3 (Java) or
  HTTP JSON-RPC (i2pd) dial inside the disposable namespace.
- D5: the data-phase oracle records per-side observation code keyed by
  the i2pr launcher's authenticated-frame counters; no echo assumption
  is made.
- D7: the mixed-runner requires ``passed`` i2pr terminal,
  ``authenticated`` reference observation, and the oracle's per-side
  observation to be ``observed`` before marking a direction ``passed``.
- D8: the sanitized evidence record carries
  ``i2pr_router_info_sha256``, ``reference_router_info_sha256``,
  ``data_phase_mode``, and ``expected_observation`` typed fields.
- D9: ``run-matrix.sh`` continues to route the four directional mixed
  scenario IDs through ``mixed_runner.py``.
- D10: an unknown reference kind now fails closed with a typed
  ``unknown-reference-kind`` rejection.

Keep accepted inbound streams paired with their non-cloneable pending-handshake permit until
authentication or a terminal handshake outcome. Runtime link queue entries
must own their item/byte accounting and release it on write success, failure,
cancellation, receiver closure, or supervisor teardown. Reader and writer
children must use the configured cancellation-aware idle/read and write
deadlines; unrestricted socket I/O is not an accepted adapter path.

The Plan 042 driver belongs in `i2pr-runtime`: it translates bounded handshake
actions into cancellation-aware socket operations, retains replay/admission and
link leases through their owning terminal paths, and owns authenticated NTCP2
frame read/write children and queues. `i2pr-transport-ntcp2` remains pure and
runtime-neutral. `tools/i2pr-interop` is only a non-production composition seam
and must never activate `i2pr-daemon`.

The general NTCP2 data-phase parser may accept specification-permitted
repeated non-padding blocks and late Termination followed only by final
Padding. SessionConfirmed part-two parsing remains a separate strict parser.
Local self-handshakes, loopback sockets, vectors, and deterministic testkit
runs are not Java I2P/i2pd interoperability evidence. Keep the daemon disabled
and all NTCP2 support rows experimental/non-advertised until sanitized mixed-
router results, hashes, and run identifiers are committed.

The launcher status boundary is part of this plan: completed `listen` must
separate listener readiness from authenticated completion, `dial` must return
one terminal typed result, and `inspect` may return only redacted metadata.
The checkout now contains the listener/dial, handshake-to-link, and
DeliveryStatus smoke composition, plus the four directional mixed-scenario
definitions, the mixed-runner adapter composition, the strict launcher
renderer, and the non-echo data-phase oracle. State, handshake, data-phase,
timeout, and cleanup failures remain typed and fail closed. Plan 042's
selected smoke scope is the existing fixed-size DeliveryStatus message (I2NP
type 10), one valid outbound and one valid inbound message per direction. Its
12-byte body and 21-byte short-transport encoding are bounded local scope
only; reference acceptance and response behavior remain unverified.

Plan 038 defines the controlled evidence harness. It is Ubuntu-only and
amd64-only for the first closure. Keep preparation and execution as separate
security domains: preparation may use `apt` and network access only for the
declared packages and pinned reference source/artifacts; execution must use
disposable namespaces with only a veth peer, no default route, no DNS, and no
public egress. The host checker must fail before changing an unsupported host,
and isolation must be verified before any router starts. Do not add an option
that disables isolation.

The exact command interfaces are:

```text
bash scripts/interop/ubuntu/check-host.sh --pre-install
bash scripts/interop/ubuntu/setup-host.sh
bash scripts/interop/ubuntu/check-host.sh --post-install
bash scripts/interop/build-references.sh
bash scripts/interop/build-references.sh --offline
bash scripts/interop/run-scenario.sh --scenario <id> --reference java_i2p --build-cache <path> --run-root <path>
bash scripts/interop/run-scenario.sh --scenario <id> --reference i2pd --build-cache <path> --run-root <path>
bash scripts/interop/run-matrix.sh --profile environment-smoke
bash scripts/interop/run-matrix.sh --profile reference-crosscheck-ipv4
i2pr-interop ntcp2 listen --scenario-config <path>
i2pr-interop ntcp2 dial --scenario-config <path>
i2pr-interop ntcp2 inspect --state-dir <path>
```

Classify harness results precisely: environment smoke validates reference
startup and cleanup only; Plan 041's dedicated reference-pair profile runs
`reference-java-i2pd-ipv4` and `reference-i2pd-java-ipv4` with separate
`java-*`/`i2pd-*` namespaces, an explicit non-public network ID, staged
RouterInfo validation/import, and dual authenticated observations. A host,
cache, parser, or observation failure remains a typed blocker; it is never a
protocol pass. i2pr mixed-router evidence requires an authenticated bounded
run between i2pr and each reference in both directions. Keep only sanitized
typed results and artifact/configuration hashes under
`target/interop/evidence/`; secret-bearing run roots under
`target/interop/runs/<run-id>/` are deleted. Delete identities, keys,
RouterInfo, I2NP, raw addresses, transcripts, raw logs, and arbitrary remote
error text. These harness profiles do not enable the daemon or advertise NTCP2.

The current typed blockers are distinct: `blocked_host_contract` means the
Ubuntu/amd64/privilege/isolation prerequisite failed before a protocol run;
`i2pr-mixed-router-profile-not-wired` means the reference harness has not yet
connected the launcher to a reference adapter. Rejected scenarios/state and
typed authentication, timeout, or cleanup failures must stay visible.
Empty or reference-only evidence is not an i2pr interoperability result.

Plan 043 adds the build-system promotion contract. Its ordered gates are
`contract`, `reference-build`, `reference-offline-reuse`, `environment-smoke`,
`reference-crosscheck-ipv4`, `i2pr-handshake-smoke-ipv4`, `full-matrix`,
`evidence-validation`, and `cleanup-verification`. Preparation is the only
network-enabled trust domain; execution consumes verified offline caches and
namespace-local veth links. The reference control must pass before i2pr
profiles, and cleanup verification must run with an always-run policy and fail
the lane independently of protocol results.

The exact host contract is Ubuntu 24.04 amd64/x86_64, Bash 4+, UTF-8 locale,
non-interactive sudo when needed, Linux namespace/nftables support, and at
least 4 GiB free under `target/`. The declared package set and locked source,
IzPack, cache, and build-command inputs are authoritative in
`tests/integration/ntcp2/references.lock.toml`. Offline reuse must re-hash the
complete runtime tree and must never fetch on a miss. The aggregate evidence
manifest may reference only sanitized typed JSON and approved hashes; raw logs,
RouterInfo, identities, keys, endpoints, packet captures, payloads, private
paths, and secret-bearing run roots are forbidden.

Promotion is manual first, scheduled only after repeated clean-checkout and
cache-reuse success, then a current successful run at Milestone 3 closure.
Privileged execution is not automatically exposed to forked or untrusted pull
requests. The current checkout has not completed this lane and has no
mixed-router i2pr evidence; do not present workflow scaffolding or reference-
only control results as NTCP2 support.

## Plan 044 mixed-router boundaries

Plan 044 composes the mixed-router execution model with four directional
i2pr/reference IPv4 scenarios: `i2pr-to-java-ipv4`, `java-to-i2pr-ipv4`,
`i2pr-to-i2pd-ipv4`, and `i2pd-to-i2pr-ipv4`. Each direction has a unique
execution ID, one declared initiator and responder, one terminal typed result,
and one evidence record. No direction may mask another.

The mixed runner composes `I2prAdapter` with each reference adapter through
a strict launcher scenario renderer. The data-phase oracle does not rely on
an echo assumption; it uses a protocol-valid trigger supported by both pinned
references. The evidence schema carries real counters for authenticated-link
count, frames sent/received, I2NP message aggregates, admission/replay
counters, process lifecycle counters, and cleanup disposition.

Gate archival uses gate-specific staging to prevent cross-gate record
relabeling. The aggregate manifest must include exactly the expected records
for the selected profile; missing, extra, mislabeled, or zero-valued
records fail the gate.

## Build, Test, and Quality

Toolchain is pinned to Rust 1.95.0 (`rust-toolchain.toml`); MSRV is 1.88
(verified by a dedicated CI job). Workspace edition is 2024; `max_width = 100`.

Before handoff, run from the repo root, in this order:

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
bash scripts/check-multipass-interop-boundary.sh # when Multipass lifecycle files change
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py'
bash scripts/fuzz-smoke.sh               # opt-in, requires cargo-fuzz + nightly
```

Focused lanes:

- Transport: `cargo test -p i2pr-transport --all-targets` and
  `cargo test -p i2pr-transport-ntcp2 --all-targets`.
- Runtime supervision: `cargo test -p i2pr-runtime --all-targets`. The
  forced-cleanup 100-iteration test needs serial execution:
  `cargo test -p i2pr-runtime forced_child_cleanup_is_repeatably_joined -- --test-threads=1`.
- Deterministic testkit: `cargo test -p i2pr-testkit --all-targets`.

Runtime tests must use `#[tokio::test(start_paused = true)]` or `ManualClock`
with fixed seeds and bounded steps. **No wall-clock sleeps, no DNS, and no
public-network traffic in tests.** Runtime-owned socket lifecycle tests may
use loopback only; all other transport verification uses the testkit or an
explicitly authorized private network.

## Coding Conventions

- Workspace lints: `unsafe_code = "deny"`, `unexpected_cfgs = "deny"`,
  `unused_must_use = "warn"`; clippy denies `dbg_macro`, `todo`, `unimplemented`.
- `crate/secret` owners stay non-cloneable, non-`Debug`, and `zeroize::Zeroize`;
  the NTCP2 forbidden nonce `2^64 - 1` is never emitted.
- Treat configuration, protocol, and persisted data as hostile: explicit
  bounds, reject unknown or trailing bytes, no validation side effects, always
  test the negative path.
- Codec errors are typed; don't swallow codec results.
- NTCP2 static key/IV material lives in the separate versioned `i2pr-storage`
  record — never derive from or overwrite the router identity record.
- All architecture/security decisions belong in `docs/adr/` and `specs/`;
  the plan-of-record is the active `plans/NNN-*.md` plus its closure record.
  When you close a milestone, leave an explicit closure document with
  commands, results, and evidence.

## Support Ledger

Every protocol surface is tracked in `specs/support.toml` (mirrored to
`docs/protocol-support.md`). Entries default to `status = "experimental"` and
`advertised = false`. Setting `advertised = true` requires interoperability
evidence per `specs/CONFORMANCE.md` — namespace presence is not evidence.

## Commits and Pull Requests

- Focused imperative subjects, e.g. `docs: streamline repository guidelines`,
  `transport: bound ntcp2 data frame owners`.
- PRs document scope, changed files, test commands and results, dependency or
  security decisions, deviations, and known limitations. Milestone closures
  attach the closure record with that evidence.
- Don't update git config, skip hooks, force-push, or amend someone else's
  commit. If a hook rejects, fix the issue and add a new commit.

## Plan 049 Multipass lifecycle-owned rootless recovery lane

The current host remains the Plan 046 `host.apparmor-restrict-on` negative
baseline. Plan 048 uses only the disposable Multipass guest for the
`host.apparmor-restrict-off` recovery category; never change host AppArmor or
user-namespace policy. The canonical manifest is
`scripts/interop/multipass/environment.toml`, the canonical guest cache is
`/home/i2ptest/i2pr/target/interop/cache`, and the guest execution user is
`i2ptest` with no sudo or capabilities.

Preparation may use network access for cloud-init and verified input transfer.
Execution must follow `prepare-offline.sh`, pass `probe.sh`, then run the four
Plan 045 directions through `run-matrix.sh` as `i2ptest`. The reviewed
environment ID is distinct from the generated run ID and concrete instance
name/generation. `run-evidence-lane.sh --all` must reserve lifecycle state
atomically before launch and allocate a fresh collision-resistant name; the
legacy `i2pr-interop-rootless` name is never authoritative.

Record the host baseline separately from the guest probe. The host
`blocked_unprivileged_user_namespace` outcome is informational for this lane;
the guest `rootless_sandbox_available` outcome is required after provisioning
and immediately before any router process. Do not use host mounts, arbitrary
guest commands, privileged containers, or silent fallback to the privileged
topology. Export only the validated sanitized bundle with `export-evidence.sh`;
destroying an owned guest must preserve
`target/interop/evidence/multipass/<run-id>/`.

Adoption, resume, recreation, and destruction are explicit operations:
`--adopt-owned`, `--resume-owned`, `--recreate-owned`, `--destroy-owned`, and
`--inspect`. They require a cryptographically linked host/guest ownership
contract and validated lifecycle state. Name-only matches, unowned collisions,
unknown/deleted-but-unpurged state, and contract mismatches are typed blockers;
no normal path may issue a global `multipass purge` or implicitly mutate an
existing instance. Lifecycle locks serialize per-run/per-instance state
changes, and generation-bound snapshots/evidence cannot be mixed.

The Multipass layer has its own static/simulated tests in
`tests/integration/ntcp2/harness/test_multipass.py`. A missing Multipass
daemon, guest policy mismatch, failed rootless probe, offline-enforcement
failure, cache/source mismatch, cleanup failure, or evidence-validation
failure is a typed blocker, never an interoperability pass. Plan 049 does not
advance `specs/support.toml` or close Milestone 3. A pre-router blocker is
written as sanitized environment-blocker evidence and can never satisfy
protocol conformance.

## Plan 050 Multipass cloud-init recovery and guest-probe pass

The host cloud-init failure surface is now classified into a sanitized
typed taxonomy in `scripts/interop/multipass/cloud_init_status.py`
(`blocked_cloud_init_post_verify_failure`,
`blocked_cloud_init_service_failure`, `blocked_cloud_init_boot_timeout`,
`blocked_cloud_init_status_unparseable`,
`blocked_cloud_init_user_incomplete`,
`blocked_cloud_init_phase_missing`). The compatibility alias
`blocked_cloud_init_failed` is retained only for transition consumers.
`cloud-init-status.sh` captures `cloud-init status --long`, the four
canonical services, and the boot-finished marker, classifies, and emits
sanitized JSON.

The base cloud-init unit no longer installs `rustup` or any host
toolchain inside the guest; it installs the declared system packages,
writes `provisioning.json`, drops a `base-packages.complete` phase
marker, and exposes `/usr/local/sbin/i2pr-multipass-verify-base`. The
host `verify-base.sh` command runs that script via `multipass exec`,
parses the JSON, writes a sanitized `multipass-base-verify` record, and
verifies the ownership contract file ownership/mode before any router
work.

`run-evidence-lane.sh --guest-probe-only` runs create-adopt +
cloud-init-status + verify-base + probe and emits a
`multipass-guest-probe-only` record. The flag forbids router launch,
cache transfer, and `run-matrix.sh` execution. The selective-purge
remediation in `selective-purge.sh` confirms the instance is in
`Deleted` state and the ownership contract matches
`environment_manifest_sha256` before any `multipass purge <instance>`;
unowned collisions, unsupported client versions, or missing manifests
return typed blockers without mutating global Multipass state. The
static boundary check `check-multipass-interop-boundary.sh` enforces
the new artifacts, sanitized taxonomy, phase markers, absence of
`rustup` in cloud-init, absence of `eval`, and absence of any global
`multipass purge` form in normal paths.

## Plan 052 Milestone 3 evidence closure follow-up

Plan 052 is the corrective execution and evidence-closure plan for
Milestone 3. Milestone 3 remains open: NTCP2 stays experimental and
non-advertised.

### Source provenance is single-source and fail-closed

One exact clean source commit, computed by `git rev-parse HEAD`, drives
every artifact. The canonical record lives in
`target/interop/evidence/<run-id>/run-identity.json` under schema
`i2pr-interop-run-identity-v1`. Every direction, attestation, trigger,
observation, cleanup, and aggregate record carries
`run_identity_sha256` and is cross-checked by
`tests/integration/ntcp2/harness/run_identity.py:cross_check`.
Short SHAs, dirty trees, archive/manifest mismatches, and non-finalized
run identities are typed blockers.

### Diagnostics mode

The prior `I2PR_INTEROP_DUMP_RUN_LOGS` switch is replaced by the
tri-state `I2PR_INTEROP_DIAGNOSTICS=off|sanitized|raw-local` env var
(`tests/integration/ntcp2/harness/mixed_runner.py:_diagnostics_mode`).
The default is `off`. `raw-local` is forbidden under any export root;
the probe raises `raw-local-diagnostics-forbidden-under-export-root`.

### Receiver-side observation schema v2

Per-side observations use `i2pr-ntcp2-direction-observation-v2` with
bounded levels (`process_started`, `listener_ready`, `tcp_connected`,
`ntcp2_authenticated`, `frame_emitted`,
`frame_authenticated_and_decrypted`, `i2np_message_decoded`,
`terminal_clean`). The directional predicate requires both sides to
observe `ntcp2_authenticated`, the sender to emit the bounded data
frame, the receiver to report `frame_authenticated_and_decrypted`
AND `i2np_message_decoded`. Handshake-only markers cannot pass.

### Reference observation catalog

Per-reference observation markers are bound to the pinned revisions in
`tests/integration/ntcp2/reference-observation-catalog.md`. Updating a
marker requires updating the document, the matching adapter, and the
locked revision line. `SessionConfirmed sent` /
`SessionConfirmed from` / `NTCP2 connection established` are
handshake-only and never satisfy the data phase.

### Atomic evidence bundles

Each Milestone 3 run produces
`target/interop/evidence/milestone-3/<run-id>/` containing
`run-identity.json`, the `environment/` block, the per-direction
`attestations/`, `directions/`, `triggers/`, `observations/`, and
`cleanup/` records, a `diagnostics/sanitized-summary.json`, and the
sanitized manifest. `tests/integration/ntcp2/harness/evidence_bundle.py`
handles staging, hashing, and atomic host export. An interrupted
export leaves a typed incomplete staging directory and never overwrites
a valid prior bundle.

### Java startup probe

The standalone probe at
`tests/integration/ntcp2/harness/java_startup_probe.py` isolates Java
startup from i2pr and NTCP2. It supports `--reference-install`,
`--data-dir`, `--data-state {empty,config-only,fresh-unique-seed,
initialized-snapshot}`, `--launcher {runplain,wrapper}`, `--namespace
{outer,rootless}`, `--sequence {single,generate-live}`, `--attempts`,
and `--output`. The probe never opens an NTCP2 peer connection and
never asserts an interoperability result.

### Reference-initiated trigger contracts

The source-inspection record required by Plan 052 F2 lives at
`tests/integration/ntcp2/reference-trigger-contracts.md`. Until both
Java and i2pd helpers are committed and source-locked to their pinned
revisions, the two reference-initiated directions remain typed
blockers.

### Boundary checks

The new opt-in `RUN_IDENTITY_BIND_FIELDS` suffix on the existing
evidence record (`tests/integration/ntcp2/harness/evidence.py`)
coexists with the prior `MULTIPASS_RECORD_FIELDS` and base
`RECORD_FIELDS`. The static boundary checkers (`check-*.sh`) are
unchanged and continue to pass. New tests live under
`test_plan052.py`, `test_evidence_bundle.py`, and
`test_java_startup_probe.py` and must stay green alongside the existing
`test_harness.py`, `test_multipass.py`, and
`test_rootless_topology.py`.

## Plan 053 evidence-pipeline integration

Plan 053 is the active corrective pass for integrating Plan 052 evidence into
the canonical rootless/Multipass path. Use
`tests/integration/ntcp2/harness/plan052_pipeline.py` as the single owner for
measured run identity creation, freeze checks, bound per-direction artifact
classes, diagnostic finalization, and atomic export. Pass
`--run-id`, `--run-identity`, `--bundle-staging`, and
`--evidence-profile milestone-3-v2` explicitly through every launcher boundary;
do not infer them from the working directory.

Every primary direction must write exactly one attestation, direction,
trigger, observation-v2, and cleanup record even when blocked or rejected.
Missing reference receiver markers are typed `not-observed` rejections. Never
write legacy unbound direction records in Plan 053 mode, collapse a bounded
launcher responder reason, or add an export acknowledgement inside a finalized
bundle. A local complete result is
`diagnostic-complete-not-certificate`, never a Milestone 3 certificate.

Required focused checks are:

```text
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan053.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_evidence_bundle.py'
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-rootless-interop-boundary.sh
bash scripts/check-multipass-interop-boundary.sh
```

## Plan 054 Java startup and reference-observation qualification

Plan 054 is the active local qualification pass for the Plan 052
directional predicate. It introduces the 16-cell Java startup matrix
(`tests/integration/ntcp2/harness/java_matrix.py`), the frozen
template lifecycle (`scripts/interop/java-prepare-template.py` and
the `seeded-clone` data state), and the machine-readable reference
observation catalog
(`tests/integration/ntcp2/reference-observation-catalog.toml`). The
Java and i2pd adapters expose `collect_observation(role, run_id,
correlation, log_cursor, catalog)` and return finalized
`i2pr-ntcp2-direction-observation-v2` records. `mixed_runner.py` no
longer hardcodes `reference-receiver-marker-not-source-locked`; the
Plan 052 predicate now consumes the live observation. The
`plan052_pipeline._build_observation` accepts the live `i2pr_observation`
and `reference_observation` records; the synthetic builder remains as
the typed fallback for blocked and rejected directions.

Do not run a complete external matrix or qualification on a host that
lacks the pinned Java 2.12.0 and i2pd 2.60.0 references. The
Plan 046 host is the negative baseline; the Plan 048/049 Multipass
recovery lane is the canonical external path. A local
`diagnostic-complete-not-certificate` bundle is not Milestone 3
closure. NTCP2 remains experimental and non-advertised.

Required focused checks are:

```text
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan054.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan053.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_evidence_bundle.py'
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-rootless-interop-boundary.sh
bash scripts/check-multipass-interop-boundary.sh
```

## Plan 055 reference-initiated NTCP2 trigger and topology qualification pass

Plan 055 is the qualification pass for the two reference-initiated
directions (`java-to-i2pr-ipv4` and `i2pd-to-i2pr-ipv4`). It owns:

- The locked machine-readable trigger record schema
  `i2pr-reference-trigger-v3` in
  `tests/integration/ntcp2/harness/trigger_record.py` with the
  bounded `TriggerHelperKind` and `TriggerOutcome` enumerations.
- The source-inspection record for each pinned reference under
  `tests/integration/ntcp2/reference-trigger-contracts.md`. The
  Plan 055 B5 decision for i2pd is
  `i2pd-direct-helper-selected`; the Plan 055 C5 decision for Java
  is
  `java-direct-helper-rejected-global-context-not-isolatable`.
- ADR 0021 (`docs/adr/0021-minimal-java-support-topology.md`)
  governing the optional `java-minimal-support-topology` fallback
  that may be implemented only after the ADR is approved.
- The Plan 055 trigger schema is the only allowlisted trigger
  schema in `tests/integration/ntcp2/harness/evidence_bundle.py`;
  legacy `v1`/`v2` trigger records are still readable but never
  emitted by the Plan 052/053 pipeline.
- The Plan 052/053 pipeline (`plan052_pipeline.write_direction_artifacts`)
  binds the trigger record digest, correlation nonce, target
  RouterInfo hash, and run identity into the direction record
  (Plan 055 E2). A successful trigger outcome may not mask a
  rejected direction; the bounded responder reason from the Rust
  launcher is preserved (Plan 055 E3).

The Plan 055 helpers themselves (i2pd direct connect, Java minimal
support topology) are external paths and may only run inside the
Plan 046 rootless sealed-namespace lane or the Plan 048/049
Multipass recovery lane. On this host (the Plan 046
`apparmor_restrict_on` negative baseline) the helpers cannot be
exercised, so the directions remain typed blockers until Plan 056
produces two complete reproducible bundles. NTCP2 remains
experimental and non-advertised.

## Plan 056 two-bundle Milestone 3 certificate verifier

Plan 056 closed with a typed host-environment blocker (the Plan 046
`apparmor_restrict_on` negative baseline plus the Plan 051 host
resource constraints). The plan delivered the full implementation
surface required for any future Milestone 3 certificate:

- `tests/integration/ntcp2/harness/verify_milestone3_certificate.py`
  — the canonical two-bundle verifier. Schema
  `i2pr-milestone3-certificate-v1`. Re-verifies each bundle via the
  existing `evidence_bundle.verify_bundle` helper, then enforces the
  cross-bundle provenance, direction-predicate, and independence
  rules. CLI exits `0` only when `verified == true`, `3` on a
  denied certificate, `2` on a structural failure.
- `tests/integration/ntcp2/harness/test_plan056.py` — the
  certificate verification test matrix (positive + 16 negative
  fixtures).
- `scripts/interop/plan056_drive_bundles.py` — the local-evidence
  driver that constructs two independent Plan 052 diagnostic bundles
  from typed synthetic blocked-direction inputs and runs the
  certificate verifier against them. The driver does not exercise
  a mixed-router NTCP2 connection; it exercises the verifier. The
  two bundles are produced locally under the ignored
  `target/interop/evidence/plan056/` working directory. The only
  tracked repository footprint is the bounded local-diagnostic
  receipt at `tests/integration/ntcp2/evidence-receipts/plan056-local-diagnostic.json`
  with `artifact_storage = local-untracked`.
- `plans/056-candidate.md` and `plans/056-closure.md` — the frozen
  candidate SHA and the closure record. Plan 058 retired the
  candidate; the historical fields are preserved verbatim as an
  audit record.

The Plan 056 implementation surface is mandatory. Any change that
removes or weakens the verifier, the test matrix, or the static
boundary checks must be re-justified in a new plan-of-record and
must not silently weaken the Milestone 3 evidence gate.

## Plan 058 record and candidate integrity closure pass

Plan 058 retired the Plan 056 candidate and superseded the
Plan 057 follow-up plan. Plan 058 is the only path that creates
the corrected candidate record and supersession markers. The plan
delivered:

- `tests/integration/ntcp2/harness/candidate_record.py` — the
  candidate record integrity validator. Schema
  `i2pr-interop-candidate-v1`. Refuses records with multiple
  authoritative SHAs, retired candidates consumed by execution
  tooling, candidates frozen before the implementation floor, and
  `committed` evidence claims that name ignored diagnostics.
- `tests/integration/ntcp2/harness/test_plan058.py` — the
  candidate record, supersession, and execution-lane regression
  matrix (positive + 14 negative fixtures).
- Tracker markers in `plans/056-candidate.md` (retired),
  `plans/057-cross-host-milestone-3-external-evidence-run.md`
  (superseded), and `docs/adr/0021-minimal-java-support-topology.md`
  (Rejected by Plan 058 repository maintainer decision; the ADR
  forbids the Java support topology under the current four-direction
  contract).
- `scripts/check-ntcp2-interoperability.sh` extended to enforce
  the candidate record integrity invariants, the supersession
  markers, and the ADR decision marker.
- `plans/058-status.md` — the closure record with exact
  commands and results.

### Plan 058 execution lanes

Plan 058 documents two alternative execution lanes for any
future Milestone 3 evidence run:

- Lane A (direct-host): the execution host itself must report
  `rootless_sandbox_available`. Multipass is not required.
- Lane B (guest): the outer host may continue to report
  `blocked_unprivileged_user_namespace` (the Plan 046 negative
  baseline). The Multipass recovery guest must report
  `rootless_sandbox_available`. The outer-host baseline does not
  reject a valid guest lane.

The two lanes are alternatives. Exactly one lane is selected for a
candidate. A certificate may not combine Run A from one lane and
Run B from another.

### Plan 058 supersession of Plan 057

Plan 057 is no longer active execution authority. The plan was
superseded because it inherited the stale Plan 056 candidate and
required missing helpers and topology artifacts while forbidding
source edits. The Plan 058 record and candidate integrity closure
pass split the Plan 057 responsibilities into three new plans:

| Plan 057 responsibility | New owner |
| --- | --- |
| record/candidate correction | Plan 058 |
| i2pd direct helper | Plan 059 |
| Java topology and ADR decision | Plan 059 |
| receiver marker qualification | Plan 059 |
| new candidate freeze | Plan 060 |
| two external runs and certificate | Plan 060 |

Until Plan 060 produces two passing bundles from a fresh
implementation-floor candidate, Milestone 3 stays open and NTCP2
stays experimental and non-advertised.

## Plan 059 reference-side implementation and live qualification closure pass

Plan 059 implements the i2pd direct helper, the per-reference
observation qualification receipts, and the canonical pipeline
live-mode wiring that Plans 055-057 deferred. ADR 0021 was
Rejected by the Plan 058 record and candidate integrity closure
pass, so Plan 059 closes with the typed blocker
`blocked_java_support_topology_rejected` and the
`java-to-i2pr-ipv4` direction remains blocked for the pinned
Java I2P 2.12.0 revision. Plan 060 cannot start under the
current four-direction contract until either a future pinned
Java revision is adopted or the closure contract is revised
through a new ADR.

The plan delivered:

- `tests/integration/ntcp2/reference-drivers/i2pd_direct_connect/`
  — the i2pd direct helper. `i2pd_direct_connect.cpp` is the
  production helper that links against the pinned i2pd 2.60.0
  libraries and exercises the documented
  `Transports::SendMessage` seam. `i2pd_direct_connect.py` is
  the bounded local Python driver used by the Plan 059 tests
  when the C++ helper cannot be built. `CMakeLists.txt` is the
  build contract. `source-lock.json` records the pinned revision,
  the helper build inputs, and the locked constraints. `README.md`
  documents the helper interface and the eight Plan 055 B4
  controls.
- `tests/integration/ntcp2/reference-observation-qualification/`
  — the per-reference qualification receipts. `i2pd-2.60.0.json`
  carries the blocker `blocked_unprivileged_user_namespace`;
  `java_i2p-2.12.0.json` carries the blocker
  `blocked_java_support_topology_rejected` because ADR 0021 is
  Rejected. `summary.json` is the typed-absence summary. The
  receipts mark every semantic level as `qualified = false` until
  the Plan 046 rootless sealed-namespace lane or the Plan 048/049
  Multipass recovery lane exercises the runtime controls.
- `tests/integration/ntcp2/harness/plan059.py` — the Plan 059
  helper module that loads the source-lock record, the
  qualification receipts, and the qualification summary, and
  exposes `i2pd_helper_invocation` for the test matrix.
- `tests/integration/ntcp2/harness/plan052_pipeline.py` — the
  canonical Plan 052/053 pipeline now accepts a `live_mode` flag
  and binds helper, source, catalog, and qualification-receipt
  digests into the direction record. Live mode rejects the
  synthetic fallback for passed reference-initiated directions
  and refuses to mask i2pr terminal failures or cleanup failures.
- `tests/integration/ntcp2/harness/test_plan059.py` — the Plan 059
  test matrix (36 cases: i2pd helper, Java support-topology gate,
  receiver observations, Java startup gate, pipeline live mode).
- `scripts/check-ntcp2-interoperability.sh` extended to enforce the
  Plan 059 artifacts, the test matrix coverage, the canonical
  pipeline live-mode enforcement, and the ADR 0021 rejection.
- `plans/059-status.md` — the closure record.

### Plan 059 Plan 055-058 supersession of Plan 057

| Plan 057 responsibility | New owner |
| --- | --- |
| i2pd direct helper | Plan 059 |
| Java topology and ADR decision | Plan 059 |
| receiver marker qualification | Plan 059 |

The Java support topology remains forbidden because ADR 0021 is
Rejected; the `java-to-i2pr-ipv4` direction is a typed blocker
under the current four-direction contract.

Required focused checks are:

```text
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan059.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan058.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan056.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan055.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan054.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan053.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_evidence_bundle.py'
python3 scripts/interop/plan056_drive_bundles.py --repo-root . \
    --run-a-id <plan056-a-id> --run-b-id <plan056-b-id> \
    --evidence-root target/interop/evidence/plan056
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-rootless-interop-boundary.sh
bash scripts/check-multipass-interop-boundary.sh
```

## Plan 060 fresh-candidate and two-run Milestone 3 certificate closure pass

Plan 060 is the execution-only pass that cuts one fresh candidate
after Plan 058 and Plan 059 close, selects exactly one execution
lane (direct-host or guest), runs the four primary IPv4 mixed-router
directions twice on independent mutable state, and produces a
verified Milestone 3 certificate over the two sanitized bundles.

The plan cannot start under the current four-direction contract
until either a future pinned Java revision is adopted or the
closure contract is revised through a new ADR (because ADR 0021 is
Rejected by Plan 058). The host in the Plan 046
`apparmor_restrict_on` negative baseline cannot exercise the Plan
046 sealed-namespace lane; the Plan 048/049 Multipass recovery
lane is the canonical external path but cannot complete on this
constrained host (per Plan 051). Plan 060 therefore closes on
this host with the typed environment blocker
`blocked_execution_lane_unavailable`; the candidate is
`declared-not-executable` on this host.

The plan delivered:

- `tests/integration/ntcp2/harness/plan060.py` — the Plan 060
  helper module. Exports `plan060_typed_blocker() ->
  "blocked_execution_lane_unavailable"`, `plan060_close_status()
  -> "declared-not-executable"`, `execution_lane_lock(...)` for
  the Plan 058 two-lane contract, `candidate_record_digests()`
  for the bounded digest table, `freeze_readiness_report()` for
  the freeze-readiness checklist,
  `assert_plan060_freeze_invariants()` for the typed blocker
  enforcement, and `plan060_two_bundle_independence(...)` for the
  cross-run independence rules.
- `tests/integration/ntcp2/harness/test_plan060.py` — the Plan 060
  test matrix (35 cases across the Plan 060 surface).
- `scripts/check-ntcp2-interoperability.sh` extended to enforce
  the Plan 060 artifacts, the Plan 060 test matrix coverage, and
  the candidate/closure marker invariants.
- `plans/060-candidate.md` — the Plan 060 candidate record. Status
  `declared-not-executable`. Implements the executed source
  commit, the implementation floor, the bounded digest table,
  the lane lock, the typed blockers, and the schema marker.
- `plans/060-closure.md` — the Plan 060 closure record with the
  typed blocker and the close-status.

### Plan 060 execution lanes

The Plan 060 plan-of-record inherits the Plan 058 two-lane
contract: Lane A (direct-host, requires `rootless_sandbox_available`
on the execution host) and Lane B (guest, the outer host may
continue to report `blocked_unprivileged_user_namespace` but the
Multipass recovery guest must report `rootless_sandbox_available`).
Exactly one lane is selected per candidate; a certificate may not
combine Run A from one lane with Run B from another.

On this host the lane lock is `lane_kind = guest`,
`outer_host_baseline = blocked_unprivileged_user_namespace`,
`guest_probe_outcome = blocked_execution_lane_unavailable`. The
Plan 046 direct-host probe and the Plan 048/049 Multipass guest
probe both return typed blockers on this host. Plan 060 therefore
closes with the typed environment blocker
`blocked_execution_lane_unavailable` and refuses to advance to a
two-run certificate.

### Plan 060 freeze-readiness invariants

`plan060.freeze_readiness_report()` produces the bounded checklist:

```text
plan058_candidate_record_validator
plan058_test_matrix
plan056_candidate_retired
plan057_superseded
adr_0021_rejected
plan059_helper_source_lock
plan059_cpp_helper
plan059_python_driver
plan059_cmake_contract
plan059_i2pd_qualification_receipt
plan059_java_qualification_receipt
plan059_qualification_summary
plan059_test_matrix
plan059_canonical_pipeline_live_mode
plan059_typed_blocker_marker
plan060_test_matrix
plan060_helper_module
plan060_typed_blocker_marker
plan060_close_status_marker
execution_lane_available
```

Every item must be `True` for the candidate to advance. On this
host the `execution_lane_available` row is `False` and the
checklist reports `blocked_execution_lane_unavailable` plus any
missing prerequisites. `assert_plan060_freeze_invariants` raises
`Plan060Error` listing every failing invariant.

### Plan 060 supersession of Plan 057

Plan 057 is no longer active execution authority. Plan 058
superseded it; Plan 060 inherits the supersession. The Plan 057
document is preserved verbatim under `## Original plan
(preserved verbatim)` and every original section is suffixed
`(original)`.

| Plan 057 responsibility | New owner |
| --- | --- |
| record/candidate correction | Plan 058 (closed) |
| i2pd direct helper | Plan 059 (closed) |
| Java topology and ADR decision | Plan 059 (closed; ADR 0021 Rejected) |
| receiver marker qualification | Plan 059 (closed; receipts carry `qualified = false`) |
| new candidate freeze | Plan 060 (closed with `declared-not-executable` on this host) |
| two external runs and certificate | Plan 060 (blocked by `blocked_execution_lane_unavailable`) |

Until the Plan 046 rootless sealed-namespace lane or the Plan
048/049 Multipass recovery lane becomes runnable on a host with
the resources Plan 051 required, Milestone 3 stays open and NTCP2
stays experimental and non-advertised. A future pinned Java
revision that exposes a transport-only direct seam may trigger an
ADR re-issue that supersedes the ADR 0021 rejection and unblocks
the `java-to-i2pr-ipv4` direction.

### Plan 060 focused checks

```text
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan060.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan059.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan058.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_evidence_bundle.py'
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-rootless-interop-boundary.sh
bash scripts/check-multipass-interop-boundary.sh
```

## Plan 062 NTCP2 evidence-contract and architecture correction

Plan 062 is the evidence-contract and architecture correction pass
that supersedes the Plan 060 execution authority. Plan 062:

- lands `docs/adr/0022-direct-reference-router-ntcp2-interop-drivers.md`
  (Accepted) replacing the rejected Java-support-topology premise
  with two-process direct transport drivers for Java I2P and i2pd;
- retires the Plan 060 candidate (`plans/060-candidate.md` is now
  `retired`; `plans/060-closure.md` carries the explicit Plan 062
  supersession marker);
- introduces the Plan 062 v4 trigger schema
  (`tests/integration/ntcp2/harness/reference_trigger_v4.py`,
  schema `i2pr-reference-trigger-v4`) which replaces the 40-hex
  SHA-1 Router Hash width with the 64-hex SHA-256 Router Hash and
  binds the per-run DeliveryStatus `message_id`
  (`1..=0xffffffff`);
- introduces the Plan 062 reference-event v1 schema
  (`tests/integration/ntcp2/harness/reference_event.py`, schema
  `i2pr-reference-event-v1`) which records per-driver structured
  events (`process_started`, `listener_ready`,
  `router_info_exported`, `peer_router_info_validated`,
  `tcp_connected`, `ntcp2_authenticated`, `frame_emitted`,
  `frame_authenticated_and_decrypted`, `i2np_message_decoded`,
  `terminal_clean`, `terminal_rejected`) with strict
  per-process sequence ordering and exact DeliveryStatus message
  ID correlation for data-phase events;
- introduces the Plan 062 v3 observation schema
  (`tests/integration/ntcp2/harness/observation_v3.py`, schema
  `i2pr-ntcp2-direction-observation-v3`) which adds the mandatory
  correlation fields `delivery_status_message_id`,
  `peer_router_hash_sha256`, `local_router_hash_sha256`, and
  `source_event_sha256` and rejects generic-phrase-only sources;
- records the source-locked API surface for both references in
  `tests/integration/ntcp2/reference-drivers/source-verification.md`;
- leaves `trigger_record.py` (v3) and `observation.py` (v2) in place
  as the bounded historical-reader path; v3 trigger records and v2
  observation records remain readable but cannot contribute to a
  new passing bundle.

The future candidate implementation floor is Plan 065 closure or
later; any candidate frozen before that floor is rejected by the
Plan 062 retire-P060 invariant in
`scripts/check-ntcp2-interoperability.sh`. The Plan 060 helper
module, test matrix, and freeze-readiness checks are preserved as
an audit record and remain mandatory for any change that would
re-enable Plan 060 as active execution authority.

Plan 062 does not implement the Java or i2pd drivers; the Plan 063
Java driver and Plan 064 i2pd driver plans implement the
source-locked drivers after Plan 062 closes.

## Plan 063 Java I2P stripped-router direct NTCP2 driver

Plan 063 implements the source-locked Java I2P 2.12.0 stripped-router
direct NTCP2 driver. The driver is **test-only**, source-locked to
revision `2800040deee9bb376567b671ef2e9c34cf3e30b6`, and never
becomes a production dependency of `i2pr-daemon`. It uses the
upstream embedded `net.i2p.router.Router` and `RouterContext` with
the pinned dummy facades (`DummyNetworkDatabaseFacade`,
`DummyClientManagerFacade`, `DummyPeerManagerFacade`,
`DummyTunnelManagerFacade`), the real NTCP/NTCP2 transport
(`net.i2p.router.transport.ntcp.NTCPTransport`), the real outbound
message pool, and the real inbound message pool. Plan 063 does not
patch NTCP2 cryptography, the Noise handshake, framing, or
RouterInfo signature verification.

Plan 063 lands:

- `tests/integration/ntcp2/reference-drivers/java/src/JavaNtcp2InteropDriver.java`
  — the source-locked Java driver with strict config validation,
  bounded `inspect`/`listen`/`dial` modes, real `OutNetMessage`
  DeliveryStatus submission, and structured `i2pr-reference-event-v1`
  emission.
- `tests/integration/ntcp2/reference-drivers/java/source-lock.json`
  — the source-lock record
  (`i2pr-java-helper-source-lock-v1`) binding the pinned Java
  revision, the helper source path, and the locked constraints.
- `tests/integration/ntcp2/reference-drivers/java/classpath-manifest.json`
  — the runtime classpath binding every pinned jar in
  `target/interop/cache/java_i2p/<tree>/lib/` to its purpose.
- `tests/integration/ntcp2/reference-drivers/java/build-manifest.schema.json`
  — the build-manifest schema
  (`i2pr-java-helper-build-manifest-v1`).
- `tests/integration/ntcp2/reference-drivers/java/build-driver.sh`
  and `run-driver.sh` — the offline build and runtime seams.
- `tests/integration/ntcp2/harness/java_direct_driver.py` — the
  Python harness adapter that binds every helper invocation into a
  Plan 062 v4 trigger record (`i2pr-reference-trigger-v4`) and
  validates the Plan 063 strict driver config contract.
- `tests/integration/ntcp2/harness/test_java_direct_driver.py` and
  `test_java_direct_control.py` — the Plan 063 test matrix
  covering the source-verification contract, strict config
  contract, Python harness adapter, structured event contract, and
  the local inspect-mode round-trip where the pinned Java cache is
  available.
- `tests/integration/ntcp2/qualification/java-direct-driver.json` —
  the Plan 063 qualification receipt
  (`i2pr-java-direct-driver-qualification-v1`). On this host the
  receipt records the typed host-environment blocker
  (`blocked_unprivileged_user_namespace`); the 10/10 fresh-state
  qualification remains to be produced in the Plan 046 rootless
  sealed-namespace lane or the Plan 048/049 Multipass recovery lane.

`tests/integration/ntcp2/reference-drivers/source-verification.md`
gains the Plan 063 topology contract section. The repository
remains NTCP2-experimental and non-advertised; Milestone 3 stays
open until Plan 065 closes with one complete four-direction live
diagnostic bundle and Plan 066 produces a verified Milestone 3
certificate. Plan 063 does not wire the Java driver into the
canonical primary `mixed_runner.py`; that wiring belongs to Plan
065.

### Plan 063 focused checks

```text
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_java_direct_driver.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_java_direct_control.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan062.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_reference_trigger_v4.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_reference_event.py'
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-rootless-interop-boundary.sh
bash scripts/check-multipass-interop-boundary.sh
```

## Plan 064 i2pd direct NTCP2 driver and observer correction

Plan 064 replaces the partial Plan 059 i2pd direct connect helper
with a correctly initialized, dual-mode, source-locked i2pd 2.60.0
NTCP2 interoperability driver. The driver is **test-only** and
never becomes a production dependency of `i2pr-daemon`. Plan 064
explicitly eliminates the eight documented Plan 064 defects
(`D1`–`D8`): 64-hex SHA-256 Router Hash, NTCP2-address
static-key binding, source-verified pinned initialization, real
`CreateDeliveryStatusMsg` dispatch, bounded `SendMessage`
asynchronous semantics, sealed-topology reserved-range disable,
exact post-AEAD receive correlation, and measured provenance
for every helper input.

Plan 064 lands:

- `tests/integration/ntcp2/reference-drivers/i2pd/src/i2pd_ntcp2_interop_driver.cpp`
  — the source-locked C++ driver with strict config validation,
  bounded `inspect` / `listen` / `dial` modes, real
  `CreateDeliveryStatusMsg` submission, and structured
  `i2pr-reference-event-v1` emission.
- `tests/integration/ntcp2/reference-drivers/i2pd/src/interop_observer.h`
  and `interop_observer.cpp` — the compile-time-gated passive
  observer API and sink.
- `tests/integration/ntcp2/reference-drivers/i2pd/patches/i2pd-2.60.0-interop-observer.patch`
  — the minimal observer patch that activates the post-AEAD
  receive seam and the successful frame-write send seam.
- `tests/integration/ntcp2/reference-drivers/i2pd/source-lock.json`
  — the source-lock record
  (`i2pr-i2pd-direct-driver-source-lock-v1`) binding the pinned
  i2pd revision `f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e`, the
  helper source path, the build contract, and the locked
  constraints.
- `tests/integration/ntcp2/reference-drivers/i2pd/build-manifest.schema.json`
  — the build-manifest schema
  (`i2pr-i2pd-direct-driver-build-manifest-v1`) that requires
  measured digests for the i2pd source tree, the observer patch,
  the helper source and binaries, the linked library manifest,
  the CMake version, and the compiler version.
- `tests/integration/ntcp2/reference-drivers/i2pd/CMakeLists.txt`,
  `build-driver.sh`, `run-driver.sh`, and `README.md` — the
  offline build contract, the runtime seam, and the driver
  README.
- `tests/integration/ntcp2/harness/i2pd_direct_driver.py` — the
  Python harness adapter that binds every helper invocation into
  a Plan 062 v4 trigger record (`i2pr-reference-trigger-v4`) and
  validates the Plan 064 strict driver config contract. The
  adapter never reaches inside the C++ helper state and never
  synthesises a passing record.
- `tests/integration/ntcp2/harness/test_i2pd_direct_driver.py`
  and `test_i2pd_direct_control.py` — the Plan 064 test matrices
  covering the source-verification contract, strict config
  contract, Python harness adapter, structured event contract,
  observer compile-time gating, the Plan 059 supersedure, and the
  typed host blocker.
- `tests/integration/ntcp2/qualification/i2pd-direct-driver.json`
  — the Plan 064 qualification receipt
  (`i2pr-i2pd-direct-driver-qualification-v1`). On this host the
  receipt records the typed host-environment blocker
  (`blocked_unprivileged_user_namespace`); the 10/10 fresh-state
  qualification remains to be produced in the Plan 046 rootless
  sealed-namespace lane or the Plan 048/049 Multipass recovery
  lane.

The Plan 064 source-verification record addition lives in
`tests/integration/ntcp2/reference-drivers/source-verification.md`
under the Plan 064 i2pd topology contract section. The Plan 059
helper at `tests/integration/ntcp2/reference-drivers/i2pd_direct_connect/`
is replaced by a fail-closed compatibility stub with the explicit
Plan 064 supersedure marker; the original source-lock record is
preserved verbatim as the bounded historical-reader path.

### Plan 064 focused checks

```text
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_i2pd_direct_driver.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_i2pd_direct_control.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan062.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_reference_trigger_v4.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_reference_event.py'
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-rootless-interop-boundary.sh
bash scripts/check-multipass-interop-boundary.sh
```

### Plan 062 focused checks

```text
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan062.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_reference_trigger_v4.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_reference_event.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_observation_v3.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_evidence_bundle.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan056.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan060.py'
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-rootless-interop-boundary.sh
bash scripts/check-multipass-interop-boundary.sh
```

## Plan 065 NTCP2 canonical integration and live qualification

Plan 065 wires the corrected Java and i2pd direct drivers into the
canonical four-direction mixed-router lane, enforces the exact
DeliveryStatus correlation on the i2pr side, and produces one complete
four-direction live diagnostic bundle from a clean implementation
commit. Plan 065 establishes the implementation floor from which Plan
066 may cut a candidate.

The plan delivered:

- `tools/i2pr-interop/src/scenario.rs` — the strict scenario schema
  bumped to `i2pr-launcher-scenario-v2` with the per-run
  DeliveryStatus `message_id`, the 64-lowercase-hex expected sender
  and receiver Router Hashes, the `reference_driver_mode` field, and
  the `run_identity_sha256` field. Legacy schema 1 records are
  rejected by the strict parser.
- `tools/i2pr-interop/src/main.rs` — the i2pr sender uses the
  scenario-owned message ID and verifies the round-trip envelope
  message ID and the DeliveryStatus payload message ID before frame
  emission. The i2pr receiver requires the exact envelope and payload
  message ID, rejects duplicates, and emits the bounded Plan 065
  typed failure categories
  (`SenderDeliveryStatusMessageIdZero`,
  `SenderRouterIdentityMismatch`,
  `SenderDeliveryStatusConstructionFailed`,
  `SenderFrameQueueAmbiguous`, `SenderFrameWriteFailed`,
  `SenderMultiplePrimaryDeliveryStatusEmitted`,
  `SenderCancellationObserved`, `ReceiverFrameReadFailed`,
  `ReceiverFrameAuthenticationFailed`, `ReceiverI2npDecodeFailed`,
  `ReceiverDeliveryStatusMissing`,
  `ReceiverDeliveryStatusIdMismatch`,
  `ReceiverDeliveryStatusDuplicate`,
  `ReceiverPeerIdentityMismatch`,
  `ReceiverDeliveryStatusTimestampInvalid`). The hard-coded
  `0x0420_0001` DeliveryStatus authority is removed.
- `tools/i2pr-interop/src/status.rs` — the status counter carries
  the per-run DeliveryStatus `message_id` and the expected peer
  Router Hash. The typed failure categories are added to the bounded
  `StatusReason` allowlist.
- `tests/integration/ntcp2/harness/launcher_protocol.py` — the
  Python strict scenario schema mirrors the Rust schema with the same
  v2 marker, the same required primary fields, the same 64-hex Router
  Hash contract, and the same `reference_driver_mode` allowlist. The
  status line parser enforces the new counters.
- `tests/integration/ntcp2/harness/launcher_renderer.py` — the
  strict renderer requires the per-run DeliveryStatus `message_id`,
  the expected sender and receiver Router Hashes, the
  `reference_driver_mode`, and the `run_identity_sha256` for every
  primary direction. The renderer rejects SAM, HTTP, support-topology,
  and synthetic-fallback helpers for any primary direction.
- `tests/integration/ntcp2/harness/mixed_runner.py` — the canonical
  mixed-runner wires the new scenario primary fields through
  `render_and_validate` for both the i2pr initiator and responder
  paths. The `_plan065_primary_fields` helper derives the
  DeliveryStatus `message_id` from the run identity and the
  correlation nonce; the `_reference_driver_mode_for` helper returns
  the source-locked driver mode for a reference kind. The runner
  refuses to fall back to SAM, HTTP, support-topology, or synthetic
  helpers for a primary direction.
- `tests/integration/ntcp2/harness/test_plan065.py` — the Plan 065
  test matrix covering scenario v2 acceptance and rejection (zero
  message ID, 40-hex Router Hash, unknown reference driver mode,
  direction-helper mismatch), DeliveryStatus message ID derivation
  uniqueness, status counter contract (correlation counters,
  invalid message ID, invalid peer Router Hash), reference trigger
  v4 correlation, observation v3 correlation, pass predicate exact
  message ID and Router Hash correlation, support-router rejection,
  Plan 060 candidate retirement, and the Plan 066 implementation
  floor marker.

### Plan 065 focused checks

```text
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan065.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_reference_trigger_v4.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_reference_event.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_observation_v3.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_evidence_bundle.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_java_direct_driver.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_i2pd_direct_driver.py'
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-rootless-interop-boundary.sh
bash scripts/check-multipass-interop-boundary.sh
```

## Plan 066 fresh-candidate and authoritative NTCP2 two-run closure pass

Plan 066 is the execution-only pass that cuts one fresh candidate
descended from the Plan 065 implementation floor, selects exactly
one execution lane (direct-host or guest), runs the four primary
IPv4 mixed-router directions twice on independent mutable state,
and produces a verified Milestone 3 certificate over the two
sanitized bundles.

The plan cannot start under the current four-direction contract
until either a future pinned Java revision is adopted or the
closure contract is revised through a new ADR (because ADR 0021 is
Rejected by Plan 058). The host in the Plan 046
`apparmor_restrict_on` negative baseline cannot exercise the Plan
046 sealed-namespace lane; the Plan 048/049 Multipass recovery
lane is the canonical external path but cannot complete on this
constrained host (per Plan 051). Plan 066 therefore closes on
this host with the typed environment blocker
`blocked_execution_lane_unavailable`; the candidate is
`declared-not-executable` on this host.

The plan delivered:

- `tests/integration/ntcp2/harness/plan066.py` — the Plan 066
  helper module. Exports `plan066_typed_blocker() ->
  "blocked_execution_lane_unavailable"`, `plan066_close_status()
  -> "declared-not-executable"`, `plan066_execution_lane_lock(...)`
  for the Plan 058/060 two-lane contract,
  `plan066_candidate_record_digests()` for the bounded 23-row
  digest table, `plan066_freeze_readiness_report()` for the
  freeze-readiness checklist,
  `assert_plan066_freeze_invariants()` for the typed blocker
  enforcement, `plan066_directional_record(...)` for the per-
  direction record skeleton, `plan066_two_bundle_independence(...)`
  for the cross-run independence rules, and
  `plan066_finalized_bundle_marker()` for the bundle mutation
  guard.
- `tests/integration/ntcp2/harness/test_plan066.py` — the Plan 066
  test matrix (41 cases covering the 30 enumerated Plan 066
  Phase 12 cases plus the typed-blocker, freeze-readiness,
  helper-contract, and Plan 065 plan-of-record helpers).
- `scripts/check-ntcp2-interoperability.sh` extended to enforce
  the Plan 066 artifacts, the Plan 066 test matrix coverage, and
  the candidate/closure marker invariants.
- `plans/066-candidate.md` — the Plan 066 candidate record. Status
  `declared-not-executable`. Implements the executed source
  commit (the Plan 065 implementation floor), the bounded 23-row
  digest table, the lane lock, the typed blockers, and the schema
  marker.
- `plans/066-closure.md` — the Plan 066 closure record with the
  typed blocker and the close-status.

### Plan 066 execution lanes

The Plan 066 plan-of-record inherits the Plan 058/060 two-lane
contract: Lane A (direct-host, requires `rootless_sandbox_available`
on the execution host) and Lane B (guest, the outer host may
continue to report `blocked_unprivileged_user_namespace` but the
Multipass recovery guest must report `rootless_sandbox_available`).
Exactly one lane is selected per candidate; a certificate may not
combine Run A from one lane with Run B from another.

On this host the lane lock is `lane_kind = guest`,
`outer_host_baseline = blocked_unprivileged_user_namespace`,
`guest_probe_outcome = blocked_execution_lane_unavailable`. The
Plan 046 direct-host probe and the Plan 048/049 Multipass guest
probe both return typed blockers on this host. Plan 066 therefore
closes with the typed environment blocker
`blocked_execution_lane_unavailable` and refuses to advance to a
two-run certificate.

### Plan 066 freeze-readiness invariants

`plan066.plan066_freeze_readiness_report()` produces the bounded
checklist:

```text
plan062_v4_trigger_schema
plan062_reference_event_schema
plan062_v3_observation_schema
plan062_source_verification_record
adr_0022_accepted
plan063_java_driver_artifacts
plan064_i2pd_driver_artifacts
plan065_strict_scenario_schema_v2
plan065_directional_predicate_contract
plan065_canonical_mixed_runner
plan060_helper_module_present
plan060_typed_blocker_marker
plan060_candidate_retired
plan056_candidate_retired
plan057_superseded
adr_0021_rejected
plan059_typed_blocker_marker
plan065_test_matrix_present
plan066_helper_module_present
plan066_test_matrix_present
execution_lane_available
```

Every item must be `True` for the candidate to advance. On this
host the `execution_lane_available` row is `False` and the
checklist reports `blocked_execution_lane_unavailable` plus any
missing prerequisites. `assert_plan066_freeze_invariants` raises
`Plan066Error` listing every failing invariant.

### Plan 066 supersession

Plan 066 supersedes the Plan 060 two-run certificate authority
(Plan 060 was retired by Plan 062). Plan 066 cannot start under
the current four-direction contract until either a future pinned
Java revision is adopted or the closure contract is revised
through a new ADR (because ADR 0021 is Rejected by Plan 058).
Future candidates must descend from the Plan 065 implementation
floor or later, must use the Plan 062 v4 trigger schema, the
Plan 062 reference-event v1 schema, the Plan 062 v3 observation
schema, and the 64-hex SHA-256 Router Hash contract.

The Plan 066 implementation surface is mandatory regardless of
close outcome. Any change that removes or weakens the Plan 066
helper module, the Plan 066 test matrix, the static boundary
checker extension, or the freeze-readiness invariants must be
re-justified in a new plan-of-record and must not silently weaken
the Milestone 3 evidence gate.

### Plan 066 focused checks

```text
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan066.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan065.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_reference_trigger_v4.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_reference_event.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_observation_v3.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_evidence_bundle.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_java_direct_driver.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_i2pd_direct_driver.py'
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-ntcp2-vectors.sh
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-rootless-interop-boundary.sh
bash scripts/check-multipass-interop-boundary.sh
cargo fmt --all --check
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
```

## Plan 067 Milestone 3 staged interoperability corrective roadmap

Plan 067 is the **active** Milestone 3 corrective roadmap. Plan 067
supersedes Plan 066 as the active execution authority. Plan 066
remains an immutable historical record of the unavailable
release-qualification lane on the constrained host. Plan 067
corrects the active planning premise that release-grade isolation
and two-run certification are prerequisites for the first external
protocol test, and it corrects the stale active use of the ADR 0021
Java support-topology rejection after ADR 0022 accepted the direct
Java stripped-router driver.

Plan 067 separates NTCP2 interoperability evidence into four bounded
tiers:

- **Level 0 — local conformance.** Deterministic local protocol and
  runtime ownership.
- **Level 1 — external loopback smoke.** Two real processes on the
  host loopback. i2pd is the primary initial validator. Emissary is
  conditional. No rootless namespace, no Multipass, no candidate
  freeze, no two-bundle certificate, no reviewer record, no Java I2P
  required.
- **Level 2 — repeated development interoperability.** Both
  directions against the primary independent validator (pinned i2pd
  2.60.0), three fresh-state repetitions per direction, exact
  message and identity correlation, bounded negative controls.
- **Level 3 — release qualification.** Java I2P 2.12.0 and i2pd
  2.60.0, isolated no-public-egress lane, reproducible
  source/reference provenance, exact authenticated data-phase
  message correlation, independent fresh state, sanitized durable
  evidence.

Java and i2pd remain required for release qualification. NTCP2 stays
experimental and non-advertised. Later design and development work
may continue after Level 2 without treating that continuation as
release qualification.

## Plan 068 staged evidence and Milestone 3 authority correction

Plan 068 implements the staged-evidence and authority correction
that Plan 067 proposes. Plan 068 is the only path that produces the
Plan 030/066 active-status correction, the Plan 066 supersession
notice, the evidence-tier constants and tests, the smoke and
development-validation record schemas and tests, the
static-check-scope simplification for Plans 069-073, and the
documentation propagation.

Plan 068 delivers:

- `docs/adr/0023-staged-ntcp2-interoperability-evidence.md`
  (Accepted). ADR 0023 separates evidence into four bounded tiers
  and forbids lower-tier promotion into release bundles. ADR 0023
  does not supersede ADR 0022's direct-driver decision.
- `tests/integration/ntcp2/harness/evidence_tier.py` — the
  evidence-tier constants and tier-separation rules. The release
  bundle validators refuse every record whose tier is missing or
  lower than `release-qualification`.
- `tests/integration/ntcp2/harness/loopback_smoke_record.py` — the
  Level 1 smoke record schema
  (`i2pr-ntcp2-loopback-smoke-v1`). A passed record requires every
  positive boolean, `cleanup_clean = true`, and `network_audit`
  not equal to `not-run`. Raw payload, private key, Noise state, and
  full RouterInfo bytes are forbidden.
- `tests/integration/ntcp2/harness/development_validation.py` —
  the Level 2 development-validation summary schema
  (`i2pr-ntcp2-development-validation-v1`). A passed summary
  requires three fresh-state passes per direction, four named
  negative controls reporting `rejected`, `cleanup_passed = true`,
  and an explicit network audit per direction.
- The Plan 068 test matrices (`test_evidence_tier.py`,
  `test_loopback_smoke_record.py`,
  `test_development_validation.py`).
- `scripts/check-ntcp2-interoperability.sh` extended to enforce
  the new schema modules, the new test matrices, the ADR 0023
  acceptance marker, and the release-bundle smoke/development
  rejection. The historical plan surfaces (Plan 055/056/058/059/060
  /062/063/064/065/066 freeze-readiness invariants) remain intact.

Plan 068 also removes the stale
`blocked_java_support_topology_rejected` interpretation from the
active Java path: ADR 0021 remains Rejected and the Java support
topology remains forbidden, but the ADR 0022 direct Java driver is
the active Java architecture. Java may still be unavailable because
of host/runtime/build defects, but not because ADR 0021 forbids the
already accepted replacement architecture.

The focused closure baseline for Plans 069-073 is the touched-code
test suite plus `cargo fmt --all --check`, `cargo check --workspace
--all-targets`, `cargo test --workspace`,
`scripts/check-dependency-direction.sh`, and
`scripts/check-runtime-boundaries.sh`. Full historical harness
matrices, rootless checks, and Multipass checks remain available
for explicit integration checkpoints but are not required for
Level 1 or Level 2 closures.

Required focused checks for Plan 068:

```text
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_evidence_tier.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_loopback_smoke_record.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_development_validation.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan068.py'
```

## Plan 106 daemon NetDB/bootstrap integration (closed)

Plan 106 is the final executable plan in the current Milestone 4
foundation sequence. The Plan 106 closure record lives at
`plans/106-status.md`. The required focused checks for Plan 106
are the touched-code test suite plus the workspace-standard
formatted/lint/compile commands and the Plan 068 static-boundary
posture:

```text
cargo +1.95.0 fmt --all --check
cargo +1.95.0 check --locked --workspace --all-targets
cargo +1.95.0 test --locked --workspace
cargo +1.95.0 test --locked -p i2pr-daemon
cargo +1.95.0 test --locked -p i2pr-netdb
cargo +1.95.0 test --locked -p i2pr-storage
cargo +1.95.0 test --locked -p i2pr-runtime
cargo +1.95.0 clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo +1.95.0 doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-ntcp2-vectors.sh
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-rootless-interop-boundary.sh
bash scripts/check-multipass-interop-boundary.sh
git diff --check
```

The Plan 046 rootless checker reports a pre-existing baseline
failure (the `rootless_supervisor.py` file was retired by the
Plan 099 harness-reduction commit). Plan 106 does not modify any
rootless-owned file and the baseline failure is unrelated to
Plan 106.

## Plan 107 Milestone 5 exploratory tunnel substrate (closed)

Plan 107 is the active Milestone 5 implementation plan. It lands the
runtime-neutral substrate required by the NetDB seam to flip from
`ExploratoryPathStatus::BlockedExploratoryTunnelUnavailable` to
`Available` once a real inbound exploratory tunnel is registered.
The plan delivers a new crate `i2pr-tunnel` and wires
`NetDbSeam` to a typed `i2pr_netdb::ReplyPathProvider` trait that
the new crate's `ExploratoryPoolReplyPathProvider` adapter
implements.

Plan 107 lands:

- `crates/i2pr-tunnel/src/identity.rs` — bounded typed tunnel
  identity (`TunnelId`, `TunnelDirection`, `TunnelRole`,
  `TunnelLifetime`, `TunnelState`, `TunnelPeer`).
- `crates/i2pr-tunnel/src/config.rs` — bounded
  `ExploratoryPoolConfig` with hard ceilings
  (`max_inbound ≤ 8`, `max_outbound ≤ 8`,
  `length_hops ∈ [1, 8]`, `lifetime_seconds ≤ 1 800`,
  `build_concurrency ≤ 4`, `failure_threshold ≤ 16`).
- `crates/i2pr-tunnel/src/pool.rs` — deterministic
  `ExploratoryPool` with bounded replacement, expiry, and failure
  accounting; `select_inbound_reply_path` selector returns the
  oldest valid inbound tunnel.
- `crates/i2pr-tunnel/src/build.rs` — `BuildRecordLayout`
  (Short/Variable) over the existing
  `i2pr_proto::DeferredBuildRecords` codec plus the
  `BuildRequestKind` enumeration.
- `crates/i2pr-tunnel/src/build_crypto.rs` — `BuildCryptography`
  trait, `LayerKeys` zeroizing wrapper, and the
  `NoBuildCryptography` default that always returns
  `Unavailable`. The live ECIES-X25519 primitive lands in Plan
  108+.
- `crates/i2pr-tunnel/src/provider.rs` —
  `ExploratoryPoolReplyPathProvider` adapter that implements
  `i2pr_netdb::ReplyPathProvider` and exposes
  `has_inbound_tunnel()` + `provide_reply_path()`.
- `crates/i2pr-netdb/src/lookup_id.rs` — the new
  `ReplyPathProvider` trait plus a `RouterInfoLookup::accept_reply_path`
  method that records a path on the active lookup.
- `crates/i2pr-daemon/src/netdb_seam.rs` —
  `NetDbSeam::set_reply_path_provider` / `clear_reply_path_provider`
  / `path_status` consult the injected provider;
  `NetDbSeam::begin_lookup` consumes the path through
  `accept_reply_path`.
- `docs/architecture/i2pr-tunnel.md` — the new crate's deep dive.
- `docs/architecture/overview.md`, `docs/architecture/i2pr-netdb.md`,
  `docs/architecture/i2pr-daemon.md`, `docs/protocol-support.md`,
  `specs/support.toml` — updated to reflect Milestone 5 progress.

Plan 107 does **not** activate NTCP2, advertise tunnels, or run a
live mixed-router build. The exploratory pool is filled through an
injected `TunnelRegistrar`; the production registrar that performs
real builds is Plan 108+ scope. The Plan 046 rootless checker
continues to report the same pre-existing baseline failure
unrelated to Plan 107.

### Plan 107 focused checks

```text
cargo +1.95.0 fmt --all --check
cargo +1.95.0 check --locked --workspace --all-targets
cargo +1.95.0 test --locked --workspace
cargo +1.95.0 clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo +1.95.0 doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-ntcp2-vectors.sh
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-multipass-interop-boundary.sh
git diff --check
```

## Plan 108 ECIES-X25519 short tunnel-build construction (corrected by Plan 109)

Plan 108 is the Milestone 5 implementation plan that landed the
local construction gap from Plan 107. Plan 108's
`implementation-landed-protocol-conformance-reopened` status has
been superseded by **Plan 109** (`passed-record-and-noise-conformance`).

The Plan 108 architecture and structure are retained as the
foundation Plan 109 corrected in place. The corrected surface is:

- `EciesX25519BuildCryptography` (Noise-N) — Plan 108 replaced the
  Plan 108 custom `ECIES-X25519-Build-Session-v1` KDF path with the
  literal Noise-N transcript
  (`Noise_N_25519_ChaChaPoly_SHA256`), null-prologue MixHash,
  peer-static MixHash, ephemeral MixHash, `es` MixKey, AD = current
  `h`, nonce = 0, and post-AEAD ciphertext MixHash;
- `short_record` — Plan 109 replaced the Plan 108 byte-41 role
  flag with the canonical byte-40 role flag (`0x80` / `0x40` /
  `0x00`), replaced the Plan 108 `LayerEncryptionType::EciesAeadOnly
  (0x05)` with `LayerEncryptionType::Aes (0)`, replaced the
  millisecond `u64` time/expiration fields with `u32` minutes-since-
  epoch and a mandatory 600-second window, and replaced the
  custom one-byte Mapping-length field with the canonical two-byte
  I2P `Mapping`;
- `short_record::ShortResponseCode` — Plan 109 replaced the Plan 108
  `Rejected (1)` with `BandwidthRejected (30)`; the Plan 108 byte
  1 response code is rejected at decode time;
- `build_crypto::LayerKeys` — Plan 109 rewrote the wrapped reply /
  layer / iv keys and added `derive_layer_keys` for the SMTunnel
  KDF chain; the planar `HopCryptoSeed` seed and the
  `ECIES-X25519-Request-Key` / `ECIES-X25519-Reply-Key` custom seeds
  were removed in favor of the canonical post-`MixKey` `ck`-based
  derivations;
- `build_crypto::SealedShortRequest` — Plan 109 replaced the Plan
  108 `ephemeral || nonce || AEAD body` envelope with the canonical
  218-byte `truncated_hop_hash (16) || ephemeral_pub (32) ||
  ciphertext (154) || tag (16)` envelope; the `ephemeral_pub` is
  X25519 little-endian; the responder checks the 16-byte hash
  prefix before DH; the Plan 108 `32 + 16 + 170` envelope is
  rejected fail-closed by the static regression test
  `plan108_envelope_layout_rejected`;
- `build_crypto::ValidatedRecordSlot` — Plan 109 replaced the Plan
  108 fresh-reply X25519 exchange with hop-own reply ChaChaPoly
  using the derived `replyKey`, the caller-supplied `RecordSlot
  (0..=7)` encoded as a 12-byte nonce (zero in the first 11 bytes
  and the slot at offset 11), and the saved post-request `h` as the
  AEAD associated data;
- `conformance_fixtures::ReferenceFixture` (new) — Plan 109 added
  the canonical single-record conformance fixture that verifies
  the production primitive against an independent reference Noise-N
  + HKDF chain in the same crate.

The full Plan 108 → Plan 109 supersession matrix lives in
`plans/109-short-build-record-and-noise-conformance-correction.md`.
The closure record (`plans/109-status.md`) carries the recorded
commit SHA, test counts, and dependency changes.

Do **not** treat any of the following Plan 108 claims as
protocol-conformance evidence. They are listed here for reference
only and are unreachable through the current public API:

- Plan 108 short request plaintext field layout;
- low-order role flag encoding (`0x01` / `0x02`);
- layer encryption type `0x05`;
- custom millisecond time/expiration wire fields;
- custom request-key seed and nonce derivation;
- custom `ECIES-X25519-Build-Session-v1` KDF path;
- request envelope `ephemeral || nonce || AEAD body`;
- empty request AEAD associated data;
- fresh reply X25519 exchange;
- Plan 108 reply plaintext fields/response code `1`;
- concatenated-record message representation as a complete STBM payload;
- creator/responder self-round-trip tests as independent conformance proof.

Plan 109 does **not** activate NTCP2, advertise tunnels, run a
live mixed-router build, or claim a network-facing
interoperability result. The next executable plan is **Plan
110** ([`plans/110-short-build-multirecord-preprocessing-and-conformance-closure.md`](plans/110-short-build-multirecord-preprocessing-and-conformance-closure.md)).

### Plan 109 focused checks

```text
cargo +1.95.0 fmt --all --check
cargo +1.95.0 check --locked --workspace --all-targets
cargo +1.95.0 test --locked --workspace
cargo +1.95.0 clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo +1.95.0 doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-ntcp2-vectors.sh
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-multipass-interop-boundary.sh
git diff --check
```

## Plan 109-110 Plan 108 short-build protocol conformance corrective roadmap (active)

The Plan 109-110 corrective roadmap remains the **active** Milestone 5
authority for short tunnel-build construction. Plan 109 closed as
`passed-record-and-noise-conformance`; the multi-record
implementation surface is Plan 110 scope:

- **Plan 109** (closed `passed-record-and-noise-conformance`) —
  exact 154-byte short request plaintext, exact 218-byte
  encrypted request record, literal Noise-N transcript and KDF,
  exact 202-byte reply plaintext, exact 218-byte hop-own reply
  AEAD, short-record KDF derivation, and independent one-record
  fixtures. See
  [`plans/109-short-build-record-and-noise-conformance-correction.md`](plans/109-short-build-record-and-noise-conformance-correction.md)
  and the closure record
  [`plans/109-status.md`](plans/109-status.md).
- **Plan 110** — randomized record slots, fake records, raw
  ChaCha20 preprocessing/postprocessing of other records, exact
  one-byte-count STBM/OTBRM payload framing, multi-hop
  deterministic simulation, and independent multi-hop fixtures.
  See [`plans/110-short-build-multirecord-preprocessing-and-conformance-closure.md`](plans/110-short-build-multirecord-preprocessing-and-conformance-closure.md).

Neither plan activates NTCP2, performs live mixed-router
validation, requires public I2P access, requires Docker/Multipass
isolation, or adds a new Python interoperability framework. After
Plan 110 closes, a separate narrow external-delivery checkpoint
must select the smallest available qualified delivery lane and
send one independent build.

### Plan 109/110/111/112/113/114/115 current authoritative state

```text
plan_108                         = superseded-local-architecture-retained-wire-crypto-corrected
plan_109                         = superseded-by-plan111-corrected
plan_110                         = superseded-by-plan111-corrected
plan_111                         = passed-final-local-short-build-conformance
plan_112                         = passed-outbound-pre-delivery-closure
plan_113                         = passed-inbound-reference-reconciliation
plan_114                         = passed-terminal-routing-chain-correction
plan_115                         = passed-emissary-q0-construction-and-obep-reply-only
plan_115_q0                      = passed-construction-and-obep-reply-only
short_build_record_format        = locally-conformant-fixed-vectors
short_build_noise_state          = locally-conformant-fixed-vectors
short_build_reply_crypto         = locally-conformant-fixed-vectors
short_build_derived_keys         = locally-conformant-fixed-vectors
short_build_multirecord_processing = locally-conformant-fixed-vectors
complete_stbm_payload            = locally-conformant-fixed-vectors
outbound_short_build             = locally-conformant-pre-delivery
inbound_short_build              = locally-reference-compatible-spec-text-discrepancy
intermediate_next_tunnel_chain   = validated
outbound_terminal_reply_router   = explicit-and-serialized
inbound_terminal_creator_router  = explicit-and-serialized
high_level_outbound_e2e          = strict-established
high_level_inbound_e2e           = strict-established
qualified_external_delivery      = unblocked-next-checkpoint
live_mixed_router_build          = blocked-on-qualified-delivery
normal_daemon_ntcp2              = disabled-and-unenableable
ntcp2                            = experimental-non-advertised
```

Plan 111 corrects the remaining Plan 109/110 defects against the
current official I2P Tunnel Creation Specification:

- Noise-N null-prologue `h = SHA256(h0)` is now applied before
  the peer-static `MixHash` (Plan 109 missed the null prologue).
- The request `es` derivation is a single
  `HKDF(ck, sharedSecret, "", 64)` that produces both the new
  chaining key and the request AEAD key, replacing the prior
  second-HKDF split (Plan 109 ran a second HKDF).
- Record-slot nonce/IV construction places the slot byte at
  offset **4** of the 12-byte nonce; bytes 0..3 and 5..11 are
  zero. Plan 109 placed the slot byte at offset 11.
- OBEP `RGarlicKeyAndTag` produces an 8-byte tag (Plan 109 used
  16 bytes).
- Per-hop `receive_tunnel` and `next_tunnel` identifiers are
  explicit independent `TunnelId` fields; Plan 109 derived the
  next tunnel id from the next router hash.
- `MessageHopProcessor::process_hop` decodes the role from the
  authenticated request plaintext rather than flattening to
  participant.
- An independent reference computation produces a frozen
  `fixed_vectors` module that the conformance tests assert the
  production primitive against. The frozen constants were
  generated once during the implementation pass and never
  recomputed; the production primitive must continue to match
  them or fail the conformance tests.

Plan 111 left the inbound creator-key wording unresolved. Plan 113
bounded the source review and selected Policy B: the final
specification still does not define a concrete plaintext offset or
option, while pinned Java I2P and i2pd agree on the deployed
originator fake. The real request remains fixed fields +
Mapping/padding; exactly one fake carries
`hash16 || fresh X25519 pub32 || random remainder` and is checked by
the creator after reply processing. This is
`reference-compatible-spec-text-discrepancy`, not strict final-spec
text conformance for that one semantic. The high-level path requires
an explicit inbound `originator_hash`; outbound paths remain
unchanged. Evidence is recorded in
`specs/references/short-build-inbound-creator-key.md` and
`plans/113-status.md`.

## Plan 111 short-build final local conformance correction (closed)

Plan 111 is the Milestone 5 short-build correction pass that
supersedes Plan 109/110's `passed-*` conformance claims and
reopens the local conformance gate against the current official
I2P Tunnel Creation Specification. Plan 111 closes
`passed-final-local-short-build-conformance` for the outbound
path; the inbound creator-ephemeral plaintext path remains
`blocked-inbound-layout-ambiguity`. Plan 111 does **not**
perform the external-delivery checkpoint; a future plan
consumes the byte-correct count-prefixed STBM payload and
selects the smallest available qualified delivery lane.

Required focused checks:

```text
cargo +1.95.0 test --locked -p i2pr-tunnel --all-targets
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
```

## Plan 112 outbound pre-delivery closure (closed)

Plan 112 is the Milestone 5 outbound pre-delivery closure pass
that tightens the Plan 111 outbound construction with six
deterministic local defect fixes and one provenance defect
fix, **without** advancing to a qualified external delivery lane.
Plan 112 closes
`passed-outbound-pre-delivery-closure`. The Plan 112 architecture
surface travels with the `i2pr-tunnel` crate unchanged; only the
fixes below land on this commit.

The six deterministic defects fixed by Plan 112:

- **Random post-Mapping padding.** Plan 111 left the post-`Mapping`
  padding bytes zero. Plan 112 routes the plaintext encoder
  through a caller-injected CSPRNG (`&mut R: RngCore + CryptoRng`)
  with a deterministic zero-padded path for fixtures; production
  surfaces fail closed with
  `ShortBuildError::RandomnessUnavailable` when no CSPRNG is
  injected.
- **Direction/role topology validation.** Plan 109/110 accepted
  outbound and inbound paths that violated the canonical I2P
  role ordering. Plan 112 `ShortBuildPath::validate` enforces:
  outbound paths may not contain `IBGW` and may only contain
  `OBEP` at the final hop, with `Participant` hops preceding the
  final OBEP; inbound paths must place `IBGW` at the first hop,
  may not contain `OBEP`, and place `Participant` hops after the
  first IBGW.
- **Inbound gate handoff.** Plan 112 retained the inbound gate
  while Plan 113 performed the bounded standards/reference
  reconciliation.
- **`HopCryptoContext::ephemeral_public()` accessor.** Plan 108
  exposed `ephemeral_public()` for diagnostic access. Plan 112
  deletes the accessor; the field remains private and the
  `Debug` impl retains the same label string.
- **STBM/OTBRM payload contract exactness.** Plan 110 used a
  private `encode_short_tunnel_build_payload_from_count`
  helper. Plan 112 renames it to the explicit public
  `encode_count_prefixed_short_payload` and adds a matching
  public `validate_count_prefixed_short_payload` helper. The
  helpers reject zero record counts, record counts above 8, and
  any payload whose length does not equal
  `1 + count * 218` bytes. `decode_short_tunnel_build_payload`
  remains as a thin wrapper for symmetry.
- **Frozen vector provenance.** Plan 111 asserted the production
  primitive against the frozen `fixed_vectors` module, but the
  frozen constants were generated from the same production code
  path under audit. Plan 112 adds the Rust-only reference
  provenance test under
  `crates/i2pr-tunnel/tests/plan111_reference_vectors.rs` that
  re-derives the same frozen bytes from a pure-Rust path built
  only on `x25519-dalek`, `sha2`, `chacha20poly1305`, and
  `i2pr_crypto::hkdf_sha256_extract_and_expand`, without
  consulting the frozen module. The test asserts the production
  `seal_short_request`, `open_short_request`, and
  `derive_layer_keys` reproductions match byte-for-byte and that
  re-encryption of the sealed envelope produces identical bytes.

Plan 112 does **not** activate NTCP2, perform any live
mixed-router validation, require public I2P access, or relax
the Plan 046 rootless or the Plan 048/049 Multipass gates. Plan 113
now owns the inbound reference-compatible policy. The closure record is
[`plans/112-status.md`](plans/112-status.md).

Required focused checks:

```text
cargo +1.95.0 test --locked -p i2pr-tunnel --all-targets
cargo +1.95.0 fmt --all --check
cargo +1.95.0 check --locked --workspace --all-targets
cargo +1.95.0 clippy --locked --workspace --all-targets --all-features -- -D warnings
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-ntcp2-vectors.sh
git diff --check
```

## Plan 113 inbound short-build specification/reference reconciliation (closed)

Plan 113 closes as `passed-inbound-reference-reconciliation` with Policy B,
`reference-compatible-spec-text-discrepancy`. The final specification mentions
a creator ephemeral key in inbound plaintext but does not define an offset,
Mapping key, or option flag. Pinned Java I2P and i2pd instead agree on the
deployed representation: the real request remains fixed fields +
Mapping/padding, and exactly one originator fake contains
`hash16 || fresh X25519 pub32 || random remainder`.

The high-level `ShortBuildPath` requires an explicit `originator_hash` for
inbound paths. The shared validator enforces `IBGW` first and `Participant`
thereafter with no `OBEP`; the builder emits exactly one originator fake and
the creator verifies its integrity after all replies are postprocessed. No
plaintext padding bytes are reinterpreted and no private field is invented.
Outbound construction remains unchanged. This is not strict final-spec text
conformance for the unresolved creator-key sentence. Evidence and closure are
[`specs/references/short-build-inbound-creator-key.md`](specs/references/short-build-inbound-creator-key.md)
and [`plans/113-status.md`](plans/113-status.md).

The later inbound STBM delivery rule remains out of scope; Plan 113 adds no
dispatcher, transport, NTCP2 activation, SSU2, or public-network behavior.

## Plan 114 short-build terminal routing and tunnel-chain correction (closed)

Plan 114 is the Milestone 5 routing-metadata correction pass
that supersedes the previous "qualified external delivery is
immediately unblocked" handoff statement after a post-Plan-113
audit found four high-level routing/composition defects in
`ShortBuildPath -> build_hop_specs`:

1. the terminal real hop's `next_router_hash` fell back to the
   terminal hop's own router hash;
2. outbound `ShortBuildPath` could not explicitly represent the
   OBEP reply-router identity;
3. intermediate `next_tunnel` IDs were not required to equal the
   following hop's `receive_tunnel` ID;
4. the high-level E2E success test was permissive enough to
   accept `InvalidReply`.

Plan 114 closes all four without altering the cryptography,
multi-record preprocessing, reply processor, inbound
originator-fake policy, or the count-prefixed STBM/OTBRM codec.
It lands:

- `ShortBuildPath::outbound_reply_router: Option<Hash>` with
  direction-specific terminal-routing validation
  (`MissingOutboundReplyRouter` for outbound, cross-direction
  `InvalidPath` for inbound);
- a shared `validate_routing_chain` helper that enforces
  `hops[i].next_tunnel == hops[i+1].receive_tunnel` at both the
  high-level `ShortBuildPath::validate()` boundary and the
  public lower-level `prepare_short_build_message()` entry point
  so the invariant cannot be bypassed by constructing
  `MultiRecordHopSpec` values directly;
- direction-specific terminal `next_router_hash` derivation
  (`outbound_reply_router` for outbound, `originator_hash` for
  inbound) with no terminal self-hash fallback;
- `outbound_decrypted_request_plaintext_matches_configured_path`
  and
  `inbound_decrypted_request_plaintext_matches_configured_path`
  that drive each real hop through `MessageHopProcessor::process_hop`,
  open the terminal hop's request, and assert exact routing
  fields;
- the strict
  `strict_outbound_two_hop_trajectory_deterministic_established`
  and
  `strict_inbound_two_hop_trajectory_deterministic_established`
  E2E trajectories that replace the prior permissive
  `InvalidReply OR Established` acceptance.

Plan 114 preserves all Plan 111/112/113 invariants including the
`INBOUND_SHORT_BUILD_POLICY = "reference-compatible-spec-text-discrepancy"`
value, the random-padding CSPRNG path, the count-prefixed STBM
framing, the Plan 111 frozen fixed vectors, and the inbound
originator-fake construction. Evidence and closure are
[`plans/114-status.md`](plans/114-status.md).

The next executable action is the smallest available qualified
independent-router delivery checkpoint, which must consume the
byte-correct count-prefixed STBM payload and the explicit
direction-specific terminal routing metadata. It must not
restart the historical broad interoperability harness program.

## Plan 115 qualified independent short-build consumption and external-delivery checkpoint (closed)

Plan 115 is the Milestone 5 independent-evidence checkpoint that
separates tunnel-protocol interoperability from transport
interoperability. The plan defines three evidence tiers — Q0
(independent native short-build consumer), Q1 (authenticated
transport delivery), and Q2 (reply round-trip to `Established`) —
and explicitly anticipates a Branch E closure when no bounded
independent consumer seam can be reached without substantial
internal surgery.

Plan 115 lands the canonical production I2NP bridge in
[`crates/i2pr-tunnel/src/bridge.rs`](crates/i2pr-tunnel/src/bridge.rs):
the new `ShortBuildI2npBridge` consumes a
`ShortBuildAction::Deliver`, validates the
`1 + count * 218` count-prefixed STBM body, splits the count byte
from the raw records, builds
`DeferredBuildRecords::new(count, 218, raw_records)`, wraps it in
`I2npBody::ShortTunnelBuild`, encodes with the requested standard
or short-transport I2NP header, and round-trips through the
standard-header decoder to assert the recovered body equals the
original count-prefixed payload exactly. The bridge never
double-prefixes the count byte, never mutates, reorders, or
regenerates records, and never logs raw record bytes through
`Debug`.

Q0 construction + native OBEP reply: passed locally against pinned
Emissary. The original Branch E closure
(`blocked-no-bounded-independent-consumer-seam`) is superseded by
the Q0 completion; see [`plans/115-status.md`](plans/115-status.md).

The historical Branch E closure recorded these findings (superseded
by the Q0 completion):

The expected future action is the Plan 060 / Plan 066 / Plan 116
sequence with a qualified external delivery lane; until then,
Milestone 3 and Milestone 5 mixed-router exits remain blocked on
a live exploratory transport path. Evidence and closure are
[`plans/115-status.md`](plans/115-status.md) and
[`plans/115-handoff.md`](plans/115-handoff.md).

## Plan 116 local tunnel data plane

Plan 116 is the closed Milestone 5 implementation plan that lands
the runtime-neutral tunnel data plane the established material
needs to ferry decrypted `TunnelData` payloads between hops. The
Plan 116 completion/correction pass
([`plans/116-completion-correction.md`](plans/116-completion-correction.md),
inventory defects `C1`–`C16`) closed Plan 116 as
`passed-local-tunnel-data-plane`. The Plan 116 final closure pass
([`plans/116-final-closure.md`](plans/116-final-closure.md),
inventory defects `F1`–`F5`) closed Plan 116 as
`passed-final-local-closure`. The Plan 116 terminal cleanup pass
([`plans/116-terminal-cleanup.md`](plans/116-terminal-cleanup.md),
 inventory defects `T1`–`T4`) corrects the four remaining closure
 defects — duplicate-accounting classification (`T1`),
 first-fragment delivery metadata in duplicate identity (`T2`),
 out-of-order full role-level fragmented trajectory (`T3`), and
 status / handoff / evidence authority synchronization (`T4`) — and
 keeps Plan 116 closed as `passed-final-local-closure`. Plan 117
 is closed per Plan 118 as
 `closed-for-progression-with-evidence-gap`; see
 [`plans/117-status.md`](plans/117-status.md) and
 [`plans/118-planning-authority-cleanup-and-plan117-disposition.md`](plans/118-planning-authority-cleanup-and-plan117-disposition.md).
  Plan 119 closed as `passed-leaseset2-protocol-foundation` per
  [`plans/119-status.md`](plans/119-status.md); the ordinary
  online-signed published Standard LeaseSet2 carrier is wired into
  `i2pr-proto` and `i2pr-netdb` and `DatabaseStoreData::LeaseSet2`
  replaces the type-3 `Deferred` payload for the ordinary subset.
   Plan 120 closed as `passed-destination-lifecycle-and-pools` per
   [`plans/120-status.md`](plans/120-status.md) and lands the first
   `i2pr-client` destination runtime: local destination identity
   (independent Ed25519 signing + X25519 static keys, non-`Clone`,
   non-`Debug` secrets), destination-specific tunnel pools that
   consume real one-shot `EstablishedMaterial`, local Standard
   LeaseSet2 construction and signing with self-validation through
   `i2pr-netdb`, LeaseSet2 lifecycle with bounded
   rotation/withdrawal, bounded local payload contracts, and a
   router-local destination registry.
Plan 121 closed as `passed-ecies-destination-session-layer`
    per [`plans/121-status.md`](plans/121-status.md): the
    `curve25519-elligator2 = 0.1.0-alpha.2` primitive audit
    (Plan 121 §2 / §12), the wrapped ECIES primitives in
    `i2pr-crypto`, the bounded Garlic payload block codec in
    `i2pr-proto`, and the bounded destination-context
    `EciesSessionManager` in `i2pr-client`. The
    `plan_121_deterministic_local_trajectory` integration test
    drives the two-destination NS → NSR → Existing Session
    trajectory with exact-once payload delivery, tag ratchet
    advancement, and replay rejection. Plan 122 closed as
    `passed-corrected-local-destination-routing` per
    [`plans/122-status.md`](plans/122-status.md) and
    [`plans/124-status.md`](plans/124-status.md): it composes the
    Plan 119 LeaseSet2 NetDB surface, the Plan 120 destination
    runtime, the Plan 121 ECIES Garlic session layer, and the
    Plan 116 tunnel data plane into a complete local destination
    routing pipeline. The Plan 122 surface lands the
    `handle_database_store_lease_set2` ingestion path and the
    `LookupResult::LeaseSet2Success` variant on the NetDB
    lookup engine (`i2pr-netdb`), the dedicated
    `begin_lease_set2_lookup` / `ingest_lease_set2_response` /
    `cancel_lease_set2_lookup` path on the daemon `NetDbSeam`
    (`i2pr-daemon`), the bounded `LeaseSelector` /
    `LeaseSelectionPolicy` selector (`i2pr-client`), the typed
    `OutboundRequest` builder, the `compose_outbound_delivery`
    planner, the `DestinationRouting` cache, and the
    `DestinationDispatcher` inbound surface (`i2pr-client`).
    The `plan_122_two_destination_local_composition` integration
    test exercises Phase A/B/C/F/H without touching sockets,
    DNS, or any external I2P reference.

    Plan 124 closed as `passed-plan122-corrective-closure` and
    corrected the Plan 122 composition defect where
    `compose_outbound_delivery` retained an ECIES Garlic envelope
    but fed the plaintext inner I2NP `Data` envelope into the
    outbound tunnel role. The corrected composition wraps the
    encrypted envelope in an `I2npBody::Garlic` carrier and feeds
    the standard-encoded I2NP Garlic message bytes into the
    outbound tunnel data plane. `OutboundDeliveryPlan` exposes
    `garlic_i2np_bytes: Vec<u8>` as the canonical carrier. The
    eleven Plan 124 deterministic tests in
    `crates/i2pr-client/tests/plan124_trajectory.rs` cover
    Phases A, B, C, D (existing-session carrier), E (ciphertext
    isolation, unregister atomically drops ownership), F (stale
    lease), and G (tampered / malformed / non-Garlic fault paths).
    The Plan 124 master trajectory
    `plan_124_trajectory_a_to_b_carries_garlic_through_obep`
    drives two destination identities through every tunnel role,
    the canonical `authenticated-router-link-bypassed-local-seam`,
    and the dispatcher to surface the exact application payload.
    The next executable plan is **Plan 125** (Streaming protocol-6
    framing correction + reply round-trip) under the Milestone
    6 router-construction roadmap in
    [`plans/118-123-milestone6-router-construction-roadmap.md`](plans/118-123-milestone6-router-construction-roadmap.md).

Plan 116 lands:

- `crates/i2pr-tunnel/src/layer.rs` — AES-256 ECB block encryptor
  and decryptor, AES-256 CBC encrypt/decrypt wrappers, the
  `TunnelLayerTransform` participant forward and creator/inbound
  inverse transforms, and the `DuplicateWindow` bounded
  exact-match replay window. The creator inverse uses
  `ECB-DEC / CBC-DEC / ECB-DEC` and the participant forward uses
  `ECB-ENC / CBC-ENC / ECB-ENC` per the official I2P Tunnel
  Message Specification.
- `crates/i2pr-tunnel/src/data.rs` — `TunnelPayloadHeader`,
  `DeliveryInstruction`, `FragmentDelivery`,
  `TunnelFragment::{Unfragmented, First, FollowOn}`, the
  `TunnelMessageBuilder` with single-cell and complete-message →
  first + follow-on fragmentation, the `TunnelMessageParser`
  that verifies `SHA256(post_zero_record_bytes || IV)[0..4]` and
  rejects every reserved delivery-type / delay / extended-options
  flag, CSPRNG-sourced fresh IVs and nonzero padding bytes
  with a bounded per-byte retry ceiling, and the canonical
  `unfragmented_overhead` / `fragmented_first_overhead` /
  `FOLLOW_ON_OVERHEAD` constants that drive the boundary tests.
- `crates/i2pr-tunnel/src/fragment.rs` — the bounded
  `BoundedReassembler` with hard concurrent-message, per-message
  byte, and **aggregate retained-byte** ceilings, caller-driven
  expiry, and pre-insertion capacity checks that roll back any
  state that exceeded the bound. Conflicting duplicates invalidate
  only the affected partial message. The reassembler exposes
  `ReassembledFragment { message, delivery }` and
  `insert_with_delivery` so the first-fragment delivery
  instruction survives reassembly. The terminal cleanup pass
  classifies insertions through `PartialMessage::classify()` into
  `FragmentInsertDisposition::{Inserted { added_bytes },
  ExactDuplicate}` so exact duplicates are pure no-ops for
  memory, expiry, and aggregate budget; first-fragment delivery
  metadata participates in duplicate identity via
  `ConflictingFirstMetadata` and follow-on delivery metadata is
  rejected with `UnexpectedFollowOnDeliveryInstruction`.
- `crates/i2pr-tunnel/src/established.rs` —
  `EstablishedHop`/`EstablishedTunnel` secret-material ownership
  with `Option<EstablishedNextHop>` next-hop state (no
  `u32::MAX` / zero-hash sentinels), `[IBGW, Participant*]`
  inbound remote-hop ordering, single-shot
  `EstablishedTunnel::into_extracted` material extraction, and
  `EstablishedMaterial` that owns the per-hop `LayerKeys`.
- `crates/i2pr-tunnel/src/pool.rs` — `TunnelEntry` pairs each
  `TunnelRegistration` with its `EstablishedMaterial`. Real
  `register_*_with_material` insertion paths replace the legacy
  metadata-only path; pool replies use the first remote IBGW
  router hash and receive tunnel id (not the creator tunnel id).
  The legacy `register_inbound`, `register_outbound`, and the
  internal `build_placeholder_established` helper are now
  `#[cfg(test)]`. Production callers must use
  `register_*_with_material`.
- `crates/i2pr-tunnel/src/roles.rs` — runtime-neutral
  outbound gateway / participant / endpoint / inbound gateway /
  participant / local-endpoint composition with CSPRNG-injection
  via `R: CryptoRng + RngCore` on the production send paths. The
  OBEP applies the **final forward participant layer** (not the
  creator inverse), parses every record, feeds every fragment to
  the reassembler, and rejects the completed message with
  `TunnelRoleError::UnspecifiedDeliveryInstruction` when the
  reassembler returns no delivery. The local endpoint applies the
  reverse-path creator inverse over the inbound remote hops and
  requires `DeliveryInstruction::Local` on the retained delivery.
  Multi-cell forward/receive seams
  (`OutboundGatewayRole::forward_cells`,
  `InboundGatewayRole::process_cells`) carry large standard I2NP
  messages across multiple TunnelData cells on both the outbound
  and the inbound side. The terminal cleanup pass adds the
  `outbound_to_inbound_fragmented_out_of_order_trajectory_exact_bytes`
  proof that delivers inbound TunnelData cells to the local
  endpoint with at least one follow-on before the first fragment.
- `crates/i2pr-tunnel/src/short_state.rs` —
  `ShortBuildRegistrar::admit_material` performs the real
  pool insertion (the legacy placeholder `slot(0)` is gone for
  the canonical material API). The canonical registrar surface is
  `admit_established_machine(&mut machine, now_seconds)`. The
  legacy `admit(&ShortBuildOutcome, slot, now_seconds)` fails
  closed with `EstablishedMaterialRequired` for `Established`
  outcomes and `NotEstablished` otherwise.
- `crates/i2pr-tunnel/src/short.rs` —
  `ShortBuildStateMachine::take_established_material(&mut self, established_at_seconds: u64)`
  produces the canonical `EstablishedMaterial` directly from a
  successful `StatePhase::Established` machine. The second call
  returns `EstablishedMaterialAlreadyTaken`. `HopCryptoContext::take_layer_keys`
  performs a zeroing `mem::replace` swap into the owned
  `LayerKeys`.

The crate compiles cleanly with `cargo check -p i2pr-tunnel` and
the workspace passes `cargo check --workspace --all-targets`,
`cargo clippy --workspace --all-targets -- -D warnings`, and
`cargo doc --workspace --no-deps` with `-D warnings`. The
`i2pr-tunnel` lib test suite is fully green
(`cargo test --locked -p i2pr-tunnel --lib` reports 240 passing
tests and zero `Plan 116 provisional scaffolding` `#[ignore]`
markers). The Plan 116 integration tests under
 `crates/i2pr-tunnel/tests/plan111_reference_vectors.rs` add 5
 passing reference-vector assertions (245 total tests across all
 targets). Plan 117's local Phase G composition is retained, but its
 corrected native-reference test is pending a decision on the pinned
 Emissary reply-layout defect. Status authority lives in
 [`plans/117-status.md`](plans/117-status.md).

## Plan 117 exploratory NetDB composition (closed for progression with evidence gap)

Plan 117 is the Milestone 5 implementation plan that lands
the local composition between the Plan 116 exploratory tunnel
substrate and the Plan 105/106 NetDB state machines.

The Plan 117 status authority is [`plans/117-status.md`](plans/117-status.md).
The current state is `closed-for-progression-with-evidence-gap`
per Plan 118 Phase B (Outcome 2). The Plan 118 plan-of-record
is [`plans/118-planning-authority-cleanup-and-plan117-disposition.md`](plans/118-planning-authority-cleanup-and-plan117-disposition.md);
the source floor is `99374cf498227cf8ab1c4ec6ec4216b5d4d2e08e`.

The corrective closure plan [`plans/117-corrective-closure.md`](plans/117-corrective-closure.md)
corrected the four routing/framing/activation/readiness defects (C1–C4)
and ran the terminal closure checkpoints (G, H, I, J):

- **Phase C1–C4**: corrected routing identity, outer TunnelData short-transport
  framing, pool metadata-retaining one-shot activation, registry-derived
  readiness (`crates/i2pr-daemon/src/{outbound_lookup,netdb_seam}.rs`,
  `crates/i2pr-tunnel/src/{pool,data_plane_registry}.rs`).
- **Phase G**: terminal all-i2pr production-seam lookup+publication
  trajectory using real `EstablishedMaterial` derived from successful
  short-build paths through the canonical `TunnelEntry` / `EstablishedTunnel`
  pool (`crates/i2pr-daemon/tests/netdb_integration.rs` `mod plan117_phase_g`).
- **Phase H**: historical parser compatibility remains
  `passed-emissary-wire-format-compatibility` at
  `h_emissary_database_lookup_parsed`. The corrected temporary test now runs
  inside pinned `emissary-core`'s own `#[cfg(test)]` build and reaches native
  OBEP admission plus reply AEAD opening, but strict i2pr reply Mapping
  decoding rejects the pinned reference's request-prefixed reply plaintext.
  Native publication, lookup, and inbound return evidence is therefore not
  claimed; do not relax the i2pr parser to accept that reference-side layout.
- **Phase I**: authenticated transport lane is
  `deferred-host-lane-unavailable` — the host is the Plan 046
  `apparmor_restrict_on` negative baseline; the constrained-host lane
  (Plan 077) and the Multipass recovery lane (Plan 048/049) cannot
  complete a TCP authentication probe on this host.
- **Phase J**: synchronized closure authority across `plans/117-status.md`,
  `plans/117-handoff.md`,
  `plans/115-117-external-delivery-to-live-netdb-roadmap.md`,
  `README.md`, `AGENTS.md`, `docs/architecture/i2pr-daemon.md`,
  `docs/architecture/i2pr-netdb.md`, and `specs/support.toml`.

The Plan 117 composition never enables NTCP2/SSU2, never opens a
public-network socket, and never advertises a tunnel. The
production transport adapter still owns the NTCP2/SSU2 surface; no
new transport code lands in Plan 117.

### Plan 118 terminal disposition

Plan 118 Phase B1 inspected Emissary history newer than the pinned
revision `9b43484a21d5a1291c4881cdae62a36c527f8c0f` for the native
short-build reply construction used by the Plan 117 test. The
remote master HEAD still equals the pinned revision; no newer
usable Emissary revision emits the normative short-build reply
plaintext layout that i2pr already expects. The defect is
reproducible and localized to the pinned Emissary revision.

Therefore, per Plan 118 Phase B Outcome 2:

```text
plan_117_local_composition           = passed-all-i2pr-production-seam-netdb
plan_117_native_reference            = blocked-reference-defect
plan_117_external_transport          = deferred-host-lane-unavailable
plan_117                             = closed-for-progression-with-evidence-gap
plan_119                             = passed-leaseset2-protocol-foundation
plan_120                             = passed-destination-lifecycle-and-pools
plan_121                             = passed-ecies-destination-session-layer
plan_122                             = passed-corrected-local-destination-routing
plan_123                             = provisional-awaiting-plan125-correction
plan_124                             = passed-plan122-corrective-closure
router_construction                  = may-continue
next_router_construction_plan        = Plan 125 (Streaming protocol-6 framing correction + reply round-trip)
```

Plan 119 closed as `passed-leaseset2-protocol-foundation` per
[`plans/119-status.md`](plans/119-status.md). The ordinary
online-signed published Standard LeaseSet2 carrier is wired into
`i2pr-proto` (40-byte `Lease2`, `LeaseSet2Header`,
`LeaseSet2EncryptionKey`, canonical `Mapping` options, signature
domain `0x03 || signed_bytes`) and into `i2pr-netdb`
(`ValidatedLeaseSet2`, `LeaseSet2Store`, `DestinationHash`,
`LookupKind::LeaseSet2`). `DatabaseStoreData::LeaseSet2` replaces
the type-3 `Deferred` payload for the ordinary subset; types 5/7
remain explicitly deferred. EncryptedLeaseSet, MetaLeaseSet, blinded,
offline-signing, leased, and PQ-hybrid variants remain future work
tracked by the Milestone 6 roadmap. The next executable plan is
**Plan 120** (destination lifecycle and dedicated tunnel pools)
under the Milestone 6 router-construction roadmap in
[`plans/118-123-milestone6-router-construction-roadmap.md`](plans/118-123-milestone6-router-construction-roadmap.md).
The Plan 117 evidence gap is tracked separately under the
external acceptance debt ledger in the same roadmap. Router
construction is not blocked on the unavailable authenticated
external transport lane; the current `closed-for-progression` state
keeps the native criterion visible and does not promote
parser-only or reference-only results to interoperability
evidence.

Plan 123 remains `provisional-awaiting-plan125-correction` per
[`plans/123-status.md`](plans/123-status.md) after the Plan 124
audit reopened the lower Plan 122 composition defect. The wire
codec lives in
[`crates/i2pr-proto/src/streaming/`](crates/i2pr-proto/src/streaming/)
(`StreamingPacket`, `StreamingPacketBuilder`, `StreamingFlags`,
`validate_syn_policy`, `encode_syn_replay_binding`, `verify_syn_replay_binding`,
`build_signature_preimage`, signed-SYN/CLOSE/RESET option region with
Ed25519 signatures, and the protocol-6 `ClientPayload` envelope
with zlib compression + SHA-256 + CRC32). The streaming runtime
lives in [`crates/i2pr-client/src/streaming/`](crates/i2pr-client/src/streaming/)
(`StreamingManager` synchronous, Tokio-free, deterministic-clock,
per-destination outbound and inbound connection tables, listener
backlogs, send/receive window and congestion policies,
retransmit/timeout policy, typed event surface, `connect`,
`listen`, `accept`, `send_data`, `send_close`, `send_reset`,
`process_inbound_packet`, `process_inbound_envelope`,
`drain_outbound`, `lookup_outbound`, `lookup_inbound`, and
`get_connection`/`get_connection_mut`). Plan 123 wires into
Plan 122's `compose_outbound_delivery` for outbound composition
and Plan 122's `DestinationDispatcher` for inbound routing; the
layer never owns sockets, timers, or DNS. Sixteen deterministic
integration tests in
[`crates/i2pr-client/tests/plan123_trajectory.rs`](crates/i2pr-client/tests/plan123_trajectory.rs)
cover the signed-SYN round trip, canonical preimage signature
verification, SYN replay binding rejection,
MAX_PACKET_SIZE_INCLUDED policy, corrupt-signature rejection, the
full two-destination SYN → data → CLOSE trajectory, loss recovery
via retransmit, duplicate packet idempotence, RESET termination,
send window backpressure, connection table ceiling, and signed
CLOSE / signed RESET packet shapes. Plan 123 must not be restored
to `passed-minimal-streaming-core` until Plan 125 corrects the
`ClientPayload` framing (canonical RFC 1952 gzip, no SHA-256
prefix, no custom compressed-length prefix) and the
connection-establishment state machine (no optimistic local
Established before peer SYN response). The next executable plan
is **Plan 125** (Streaming protocol-6 framing correction +
reply round-trip) per the Milestone 6 router-construction
roadmap in
[`plans/118-123-milestone6-router-construction-roadmap.md`](plans/118-123-milestone6-router-construction-roadmap.md).
external acceptance debt ledger in the same roadmap. Router
construction is not blocked on the unavailable authenticated
external transport lane; the current `closed-for-progression` state
keeps the native criterion visible and does not promote
parser-only or reference-only results to interoperability
evidence.

## Plan 096 Plan 095 CI workflow correctness and pre-dispatch closure

## Plan 096 Plan 095 CI workflow correctness and pre-dispatch closure

Plan 096 is the active workflow correctness and pre-dispatch
closure pass over the Plan 095 manual GitHub Actions lane. The
plan delivers four demonstrated workflow corrections and the
static regression surface that proves the corrections on the
post-correction workflow and rejects the pre-correction workflow
on the pre-correction source. Plan 096 does **not** dispatch a
Plan 095 live run; it only restores execution correctness so
the next manual dispatch can produce a usable evidence pair.

The four demonstrated workflow corrections:

1. The i2pr Cargo invocation uses an explicit
   `--manifest-path "${GITHUB_WORKSPACE}/Cargo.toml"` and an
   explicit `--target-dir` variable. The downstream binary is
   copied from the explicit target dir and is asserted regular,
   executable, and non-symlink before hashing.
2. The instrumented and control sanitized evidence trees are
   disjoint from the disposable run roots. The plan moves them
   to `target/interop/plan095-evidence/instrumented` and
   `target/interop/plan095-evidence/control`. The
   `delete-raw-run-state` steps delete only the disposable run
   root and explicitly assert the sanitized tree still exists
   after the destructive operation.
3. Every embedded Python heredoc in the workflow is audited for
   missing imports; the known control validator now imports
   `os` so the `os.environ` reference resolves.
4. The i2pd source digest uses `git -C i2pd ls-files -z` over
   the pinned tracked tree. The pinned revision equality and
   the worktree-dirty check are asserted before the digest is
   computed.

Plan 096 lands:

- `tests/integration/ntcp2/harness/test_plan096.py` — the
  static regression matrix (36 cases) that rejects the
  pre-correction workflow on each defect and exercises the
  dependency graph, the fail-closed live-attempt semantics, the
  disjoint build/evidence artifact trust boundaries, and the
  Plan 095/088/079/072 gate preservation.
- `scripts/check-plan095-workflow.sh` — the pre-dispatch audit
  script. It is invoked by the static boundary checker
  `scripts/check-ntcp2-interoperability.sh` before the rest of
  the static surface.
- Status authority corrections in `plans/087-status.md` and
  `plans/088-status.md`. The `plan_095` token becomes
  `ci-live-wire-lane-corrected-awaiting-one-authoritative-run`,
  the new `plan_096` token is
  `passed-pre-dispatch-workflow-correction`, and the Plan 079 /
  Plan 072 gates remain blocked / inactive.
- Documentation propagation in `README.md`, this file, the
  `i2pr-ntcp2-interop` skill, and
  `docs/architecture/interop-apparatus.md`.

After Plan 096 lands, the next executable action is exactly
**one manual dispatch** of
`.github/workflows/ntcp2-interop-host-loopback-development.yml`
at the clean Plan 096 correction commit. Plan 088 remains
blocked until the Plan 095 instrumented and control forward
records pass. Plan 079 remains blocked pending the Plan 088
two-way pass. Plan 072 remains inactive pending the Plan 088
ambiguity decision. NTCP2 stays experimental and
non-advertised.

Required focused checks for Plan 096:

```text
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan096.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan095.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan094.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan093.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan083_runner.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_i2pd_direct_driver.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_i2pd_direct_control.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan088.py'
bash scripts/check-plan095-workflow.sh
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-ntcp2-vectors.sh
bash scripts/check-rootless-interop-boundary.sh
bash scripts/check-multipass-interop-boundary.sh
bash scripts/check-ntcp2-loopback-smoke-boundary.sh
cargo +1.95.0 fmt --all --check
cargo +1.95.0 check --locked --workspace --all-targets
cargo +1.95.0 test --locked --workspace
cargo +1.95.0 clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo +1.95.0 doc --locked --workspace --no-deps
git diff --check
```

## Plan 097 Plan 095 artifact-path and cleanup corrective pass

Plan 097 is the active narrow corrective pass over the Plan 095
GitHub Actions workflow. The plan closes two workflow defects
that remained after Plan 096:

- **Artifact-path ownership (Defect A).** The `build-i2pr-interop`
  step wrote the i2pr binary to a CWD-relative `output/i2pr-interop`
  while the `hash-i2pr-build-manifest` and `verify-build-artifacts`
  steps consumed from `${BUILD_DIR}/output/i2pr-interop` after a
  step-local `cd "$BUILD_DIR"`. Producer and consumer identities
  did not match; the manifest would have hashed a file that did
  not yet exist at the consumer's path.
- **Disposable run-root cleanup (Defect B).** The cleanup used
  `find $RUN_ROOT -mindepth 1 -delete` (descendant-only) plus
  `test ! -e "$RUN_ROOT" || true` (suppressed absence
  assertion). The root directory could survive cleanup while
  the job claimed the cleanup is clean.

Plan 097 corrects both defects:

1. The i2pr artifact now travels through one canonical absolute
   `BUILD_OUTPUT="$BUILD_DIR/output"` path used by every
   producer, verifier, manifest generator, artifact uploader, and
   live consumer. No step relies on inherited step working
   directory to establish artifact identity.
2. The disposable run-root cleanup uses strict `rm -rf --`
   after an exact `case "$PLAN095_RUN_ROOT" in` path guard, and
   the post-cleanup absence assertion is unsuppressed.

Plan 097 lands:

- `tests/integration/ntcp2/harness/test_plan097.py` — the Plan
  097 regression matrix (45 cases) that rejects the pre-Plan-097
  workflow on the two defects and exercises the canonical
  absolute `$BUILD_OUTPUT` path identity, the exact path guard
  before `rm -rf`, the unsuppressed absence assertion, and the
  synthetic mutation tests that prove the regression surface
  catches the prior defective semantics on synthetic fixtures.
- `scripts/check-plan095-workflow.sh` — extended to reject both
  Plan 097 defects. The audit now fails closed when the
  workflow writes to a relative `output/i2pr-interop` destination,
  relies on a relative output directory, omits the canonical
  `$BUILD_OUTPUT` variable, omits the cleanup path guard, or
  retains the suppressed absence assertion.
- Status authority corrections in `plans/087-status.md` and
  `plans/088-status.md`. The new `plan_097` token is
  `passed-artifact-path-and-cleanup-correction`. Plan 087
  remains open pending Plan 095 CI forward evidence pair. Plan
  088 remains blocked pending Plan 095 CI closure.
- Documentation propagation in `README.md`, this file, the
  `i2pr-ntcp2-interop` skill, and
  `docs/architecture/interop-apparatus.md`.

After Plan 097 lands, the current status of the active sequence
is:

```text
plan_093 = implementation-landed-closure-incomplete
plan_094 = implementation-landed-live-closure-environment-blocked
plan_095 = ci-live-wire-lane-corrected-awaiting-one-authoritative-run
plan_096 = passed-pre-dispatch-workflow-correction
plan_097 = passed-artifact-path-and-cleanup-correction
plan_087 = open-pending-plan095-ci-forward-evidence-pair
plan_088 = blocked-pending-plan095-ci-closure
plan_079 = blocked-pending-plan088-two-way-pass
plan_072 = inactive-pending-plan088-ambiguity
ntcp2    = experimental-non-advertised
```

Plan 095 remains the single next executable plan; exactly one
manual Plan 095 GitHub Actions dispatch follows the Plan 097
correction commit. Plan 097 does **not** mark Plan 095 or
Plan 087 passed.

## Plan 098 Plan 095 runner/provenance boundary corrective pass

Plan 098 is the active narrow corrective pass over the runner
and wrapper provenance surfaces that the first authoritative
Plan 095 manual CI dispatch exposed on 2026-08-10. The
authoritative run advanced through the contract, build, and
live runner launch phases but failed closed before any TCP or
NTCP2 wire activity. The runner reconstructed a non-authoritative
`repo_root / target / debug / i2pr-interop` path instead of
using the canonical absolute artifact path supplied by the
wrapper; the runner therefore returned
`pre-protocol-preparation-failed` before launching `i2pr-interop
ntcp2 prepare`. That result must **not** be interpreted as a
wire-level NTCP2 failure.

Plan 098 corrects the runner/provenance ownership boundary and
the adjacent provenance defects in a single coherent pass:

- **Defect A**: the runner reconstructed the i2pr binary path
  instead of consuming the explicit caller-supplied path.
- **Defect B**: the reverse and preflight runners reproduced
  the same path reconstruction defect.
- **Defect C**: the runner aliased a single generic manifest
  digest into both the i2pr and i2pd build-manifest fields.
- **Defect D**: the wrapper used the instrumented build
  manifest regardless of the requested role.
- **Defect E**: the forward runner hard-coded
  `attempt_kind=instrumented` in the underlying record.
- **Defect F**: `build-driver.sh` used a recursive
  `find -type f` digest that drifts from the workflow's
  canonical `git ls-files` algorithm.
- **Defect G**: the final gate checked nonzeroness but not full
  provenance equivalence against the downloaded artifacts.
- **Defect H**: `plans/088-status.md` misclassified the August 10
  pre-protocol rejection as a wire-level result.

Plan 098 lands:

- `tests/integration/ntcp2/harness/plan083_runner.py`,
  `tests/integration/ntcp2/harness/plan084_runner.py`, and
  `tests/integration/ntcp2/harness/preflight_runner.py` —
  every runner now accepts an explicit `i2pr_binary: Path`
  argument, measures the file bytes against the supplied
  SHA-256, and fails closed with a typed
  `pre-protocol-preparation-failed` rejection when the path is
  missing, not a regular file, or its measured digest does not
  match. The `i2pr_build_manifest_sha256` and
  `i2pd_build_manifest_sha256` fields are independently
  measured; the runner no longer aliases a generic manifest
  digest into both artifact classes. The `attempt_kind`
  argument reaches the underlying record instead of being
  hard-coded.
- `scripts/interop/run-minimal-i2pd-host-loopback-probe.py`
  — the wrapper now threads the exact caller-supplied
  `--i2pr-binary` path to every runner, validates the
  `--attempt-kind` against the i2pd driver binary filename
  and the role-specific build manifest, and measures the
  canonical tracked-tree digest via `_canonical_tracked_tree_digest`
  (which must stay in lock-step with the workflow and the
  build script).
- `tests/integration/ntcp2/reference-drivers/i2pd/build-driver.sh`
  — the source-tree digest is now computed from
  `git ls-files -z` (excluding the `.git` administrative
  tree) so the build manifest digest matches the workflow
  digest byte-for-byte.
- `.github/workflows/ntcp2-interop-host-loopback-development.yml`
  — the live jobs thread `--attempt-kind instrumented` and
  `--attempt-kind control` explicitly, and the validate-gate
  job enforces exact provenance equivalence between the
  sanitized records and the actual downloaded artifacts and
  role-specific manifests.
- `tests/integration/ntcp2/harness/test_plan098.py` — the
  Plan 098 regression matrix (15 cases) covering the
  runner-boundary ownership contract, the wrapper path
  threading, the distinct i2pr/i2pd manifests, the
  role/manifest binding, the canonical tracked-tree
  digest, and the workflow final-gate checks.
- `scripts/check-plan095-workflow.sh` and
  `scripts/check-ntcp2-interoperability.sh` — extended to
  enforce the Plan 098 ownership invariants: the runner must
  accept an explicit `i2pr_binary` path, the wrapper must
  expose `--attempt-kind`, `build-driver.sh` must use the
  canonical tracked-source identity, and the workflow final
  gate must validate role-specific digests.
- Status authority corrections in `plans/087-status.md` and
  `plans/088-status.md`. The new `plan_098` token is
  `passed-runner-provenance-boundary-correction`. The August
  10 result is reclassified as a pre-protocol runner/provenance
  failure with no TCP/NTCP2 wire conclusion.
- Documentation propagation in `README.md`, this file, the
  `i2pr-ntcp2-interop` skill, and
  `docs/architecture/interop-apparatus.md`.

After Plan 098 lands, the current status of the active sequence
is:

```text
plan_093 = implementation-landed-closure-incomplete
plan_094 = implementation-landed-live-closure-environment-blocked
plan_095 = active-runner-provenance-corrected-awaiting-authoritative-rerun
plan_096 = passed-pre-dispatch-workflow-correction
plan_097 = passed-artifact-path-and-cleanup-correction
plan_098 = passed-runner-provenance-boundary-correction
plan_087 = open-pending-plan095-ci-forward-evidence-pair
plan_088 = blocked-pending-plan095-ci-closure
plan_079 = blocked-pending-plan088-two-way-pass
plan_072 = inactive-pending-plan088-ambiguity
ntcp2    = experimental-non-advertised
```

Plan 095 remains the single next executable plan; exactly one
manual Plan 095 GitHub Actions dispatch follows the Plan 098
correction commit. Plan 098 does **not** mark Plan 095 or
Plan 087 passed.

Required focused checks for Plan 098:

```text
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan098.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan097.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan096.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan095.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan094.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan093.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan083_runner.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_i2pd_direct_driver.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_i2pd_direct_control.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan088.py'
bash scripts/check-plan095-workflow.sh
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-ntcp2-vectors.sh
bash scripts/check-rootless-interop-boundary.sh
bash scripts/check-multipass-interop-boundary.sh
bash scripts/check-ntcp2-loopback-smoke-boundary.sh
cargo +1.95.0 fmt --all --check
cargo +1.95.0 check --locked --workspace --all-targets
cargo +1.95.0 test --locked --workspace
cargo +1.95.0 clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo +1.95.0 doc --locked --workspace --no-deps
git diff --check
```

Required focused checks for Plan 097:

```text
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan097.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan096.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan095.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan094.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan093.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan083_runner.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_i2pd_direct_driver.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_i2pd_direct_control.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan088.py'
bash scripts/check-plan095-workflow.sh
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-ntcp2-vectors.sh
bash scripts/check-rootless-interop-boundary.sh
bash scripts/check-multipass-interop-boundary.sh
bash scripts/check-ntcp2-loopback-smoke-boundary.sh
cargo +1.95.0 fmt --all --check
cargo +1.95.0 check --locked --workspace --all-targets
cargo +1.95.0 test --locked --workspace
cargo +1.95.0 clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo +1.95.0 doc --locked --workspace --no-deps
git diff --check
```

## Plan 074 real-driver and constrained-host corrective roadmap (historical)

Plan 074 is historical execution authority. Plan 085 supersedes its active sequence with **Plan 082 (implemented) → Plan 083 (implemented, execution pending) → Plan 084 (implemented, execution pending) → Plan 085 → Plan 086 → Plan 087 → Plan 088 → Plan 079 (blocked)**. Plans 075, 076, 077, and 080 are closed prerequisites or historical lane records. The Plan 084 historical `lane-invalidated` closure is reclassified as "runner implementation completed; required reverse wire execution never occurred" and the active development decision now lives in `plans/088-status.md`.

The corrected repository state is:

```text
plan_068_staged_evidence = implemented
plan_069_runner_scaffolding = historical
real_i2pd_driver = implemented
real_i2pd_library_linkage = present
real_reference_process_in_plan069_runner = corrected_by_plan075
real_mixed_router_attempts = 0
current_rootless_namespace_lane = unavailable
multipass_lane = qualified
plan_082_state_preparation = implemented
plan_083_forward_probe = implemented_schema_and_runner
plan_084_reverse_probe = implemented_schema_and_runner
plan_084_development_decision = lane-invalidated
support = experimental
advertised = false
normal_daemon_activation = disabled
```

The constrained-host lane decision and Plan 077 capability probe remain historical records. Do not treat capability probing or the pre-protocol Plan 078 stop as protocol evidence.

## Plan 069 host-compatible NTCP2 loopback smoke lane (historical)

> **Reclassification (Plan 074, supersession note).** Plan 069
> implements the Plan 067 Level 1 host-loopback smoke runner and its
> static boundary check; at Plan 074 registration the runner was
> scaffolding/fake-process test coverage only. The runner integrity
> correction landed in Plan 075. Plan 069 remains the historical
> scaffolding snapshot in `plans/069-status.md`.

Plan 069 implements the Plan 067 Level 1 host-loopback smoke lane.
The lane is a non-production composition that exercises a single
two-process NTCP2 direction (one i2pr launcher process, one Plan 064
i2pd direct driver process) on the host loopback, without sudo,
namespaces, Multipass, or any public-network access. The runner is
structurally incapable of producing a Level 3 release bundle or
certificate.

The runner lives in
`tests/integration/ntcp2/harness/loopback_smoke.py` and the shell
entry point lives in `scripts/interop/run-ntcp2-loopback-smoke.sh`.
The runner is invoked through:

```text
bash scripts/interop/run-ntcp2-loopback-smoke.sh \
  --direction <i2pr-to-i2pd-ipv4|i2pd-to-i2pr-ipv4> \
  --reference-driver <path> \
  --reference-build-manifest <path> \
  --reference-source-lock <path> \
  --output <smoke-record.json> \
  --source-commit <40-lowercase-hex> \
  [--network-audit-mode auto|strace|configuration-only] \
  [--diagnostics-mode off|sanitized]
```

The runner accepts only the two i2pd directions; the Java and
Emissary directions are explicitly out of scope for the lane. The
diagnostics mode accepts only `off` or `sanitized`; raw payload
capture is structurally unsupported.

Plan 069 delivers:

- `tests/integration/ntcp2/harness/loopback_smoke.py` — the runner
  module. Owns the strict CLI/config parser, the run-root lifecycle,
  the loopback port allocator, the Plan 065 strict scenario
  renderer, the Plan 064 strict driver config builder, the
  listener/dialer process ownership and cleanup, the network-audit
  probe (strace-allowlist or configuration-only), the
  failure-stage classifier, and the Plan 068 smoke record writer.
  The runner must not import or call Plan 056/066 candidate,
  bundle, certificate, rootless-topology, or Multipass authority.
- `scripts/interop/run-ntcp2-loopback-smoke.sh` — the thin shell
  entry point. Locates the repository root, validates the required
  inputs are present, invokes the Python runner, and forwards its
  exit status. The wrapper must never invoke sudo, namespaces,
  containers, VMs, or public-network access.
- `tests/integration/ntcp2/harness/test_loopback_smoke.py` — the
  Plan 069 test matrix (42 cases). Exercises the strict config
  parser, the failure staging, the cleanup contract, the
  network-audit degradation, the listener-before-dialer ordering,
  the exact DeliveryStatus correlation, the typed-blocker rules,
  the runner ownership invariants, and the static shell wrapper
  contract.
- `scripts/check-ntcp2-loopback-smoke-boundary.sh` — the static
  Plan 069 boundary check. Verifies the runner/shell/test artifacts
  are present, the allowlist markers are committed, and the runner
  is free of release/rootless/Multipass authority.
- `plans/069-status.md` — the closure record with exact commands,
  results, and no fabricated live pass.

Plan 069 does not claim mixed-router interoperability by itself; the
implementation surface is **scaffolding and fake-process test coverage
only** under Plan 074 until Plan 075 restores direction-aware process
roles, structured reference events, measured provenance, and
fail-closed guards. Plan 069 also does not modify production NTCP2
code or the i2pd direct driver.

## Plan 075 Plan 069 runner integrity and evidence correction

Plan 075 corrects the Plan 069 runner so it is structurally
incapable of producing a mixed-router pass unless it launches one
real i2pr process and one configured real reference process and
consumes authentic structured events from both.

The corrected runner must:

- launch the reference role through the configured reference driver
  via `tests/integration/ntcp2/reference-drivers/i2pd/run-driver.sh`,
  not a second `i2pr-interop` process;
- bind every accepted event to a measured reference process binary
  digest, implementation name, run ID, direction, Router Hash pair,
  and exact DeliveryStatus message ID;
- derive milestones only from validated structured events
  (`ntcp2_authenticated`, `frame_emitted`,
  `frame_authenticated_and_decrypted`, `i2np_message_decoded`), never
  from a TCP loopback probe alone;
- refuse synthetic provenance fallback hashes that fabricate a
  schema-valid digest from a run string;
- fail closed with one of the typed blockers
  `runner-reference-process-not-executed`,
  `runner-reference-events-missing`,
  `runner-synthetic-provenance-rejected`, or
  `runner-protocol-event-unproven` whenever any of the above
  contracts is violated.

Required focused checks for Plan 075:

```text
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_loopback_smoke.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_loopback_smoke_record.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_reference_event.py'
bash scripts/check-ntcp2-loopback-smoke-boundary.sh
cargo fmt --all --check
cargo check --workspace --all-targets
cargo test --workspace
git diff --check
```

Plan 075 closes the runner-integrity work only; it does not build i2pd, run a real mixed-router direction, add Docker/QEMU/namespaces/CI, change NTCP2 protocol code, or produce a Level 2 or Level 3 record. The next active plan is Plan 076, followed by Plans 077, 078, and 079. The current repository therefore has no real mixed-router attempt and remains experimental and non-advertised.

Plan 085 has since superseded Plan 074 for active execution. Plans 075, 076, 077, and 080 are closed prerequisites; the active sequence is Plan 082 (implemented) → Plan 083 (implemented, execution pending) → Plan 084 (implemented, execution pending) → Plan 085 → Plan 086 → Plan 087 → Plan 088 → Plan 079 (blocked). The Plan 084 historical `lane-invalidated` closure is reclassified as "runner implementation completed; required reverse wire execution never occurred" and the active development decision now lives in `plans/088-status.md`.

Required focused checks for Plan 069 (after the Plan 075 fix
lands):

```text
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_loopback_smoke.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_loopback_smoke_record.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan065.py'
bash scripts/check-ntcp2-loopback-smoke-boundary.sh
```

## Plan 076 real pinned i2pd library and direct driver construction

Plan 076 replaces the Plan 064 terminal-stub helper with a real
source-locked i2pd 2.60.0 test executable that links against the
unmodified pinned i2pd 2.60.0 libraries built from the pinned CMake
project. Plan 076 explicitly eliminates the six documented defects
(`P1`-`P6`) from the Plan 064 implementation surface:

- `P1`: helper `CMakeLists.txt` saw i2pd headers but did not compile
  or link the actual pinned i2pd library targets.
- `P2`: `I2PD_PLAN076_LINKED` was not defined by the build.
- `P3`: `run_listen()` and `run_dial()` were terminal rejection
  stubs.
- `P4`: inspect mode did not prove real i2pd initialization or
  RouterInfo production.
- `P5`: build manifests described linked i2pd behaviour that the
  produced binary did not contain.
- `P6`: a control binary that omits observer call sites was
  insufficient to prove behaviour neutrality when both binaries
  must execute the same genuine transport path.

Plan 076 lands the corrected artefacts:

- `tests/integration/ntcp2/reference-drivers/i2pd/CMakeLists.txt`
  — links the driver against the freshly built pinned i2pd
  libraries via the `I2PD_PATCHED_TREE`, `I2PD_PRISTINE_TREE`, and
  `I2PD_LIB_DIR` cache variables; both driver binaries are
  defined with `-DI2PD_PLAN076_LINKED=1` and the instrumented
  binary additionally carries `-DI2PD_INTEROP_OBSERVER=1`.
- `tests/integration/ntcp2/reference-drivers/i2pd/build-driver.sh`
  — drives the two-stage build: pin the i2pd source tree digest,
  configure and build the pinned i2pd CMake project with
  `WITH_LIBRARY=ON` and `WITH_BINARY=OFF`, apply the observer
  patch to a private copy of the pinned tree, build both driver
  binaries, and write every measured digest into the build
  manifests (`reference_source_tree_sha256`,
  `i2pd_libraries_sha256`, `linked_library_manifest_sha256`,
  `observer_patch_sha256`, `driver_source_sha256`, `observer_header_sha256`,
  `observer_source_sha256`, `source_lock_sha256`,
  `instrumented_binary_sha256`, `uninstrumented_binary_sha256`,
  `linked_i2pd_sources: true`).
- `tests/integration/ntcp2/reference-drivers/i2pd/src/i2pd_ntcp2_interop_driver.cpp`
  — exercises the real i2pd 2.60.0 API in `inspect`, `listen`, and
  `dial` modes. Initialization follows the source-verified pinned
  order (`config::Init` → `fs::DetectDataDir` →
  `config::SetOption` block → `crypto::InitCrypto(false)` →
  `context.Init` → `netdb.Start` → `transports.Start(true, false)`
  → `context.Start`); shutdown reverses ownership.
- `tests/integration/ntcp2/reference-drivers/i2pd/patches/i2pd-2.60.0-interop-observer.patch`
  — the receive seam is placed immediately after
  `nextMsg->FromNTCP2()` inside the `case eNTCP2BlkI2NPMessage`
  block of `NTCP2Session::ProcessNextFrame`; the send seam is
  placed at the top of `HandleI2NPMsgsSent`. Both seams are
  compile-time gated by `I2PD_INTEROP_OBSERVER`; the control build
  uses the pristine tree with the patch reverted.
- `tests/integration/ntcp2/reference-drivers/i2pd/source-lock.json`
  — records the new `linked_marker_macro`,
  `linked_marker_required`, and `pinned_i2pd_cmake_options`
  fields, plus the measured `libi2pd`, `libi2pdclient`, and
  `libi2pdlang` archive digests.
- `tests/integration/ntcp2/reference-drivers/i2pd/build-manifest.schema.json`
  — requires `i2pd_libraries_sha256`,
  `linked_i2pd_sources: true`, and
  `observer_compile_time_gated: true` on every manifest.
- `tests/integration/ntcp2/reference-drivers/source-verification.md`
  — the Plan 076 verified call graph section documents every
  symbol, file path, and line number used by the driver against
  the pinned i2pd 2.60.0 tree.
- `tests/integration/ntcp2/qualification/i2pd-direct-driver.json`
  — the qualification receipt carries the measured
  `reference_tree_sha256`, `libi2pd_sha256`, `libi2pdclient_sha256`,
  `libi2pdlang_sha256`, `cmake_lists_sha256`,
  `build_driver_script_sha256`, and the
  `plan_076_local_build_complete: true` invariant.
- `tests/integration/ntcp2/harness/test_i2pd_direct_driver.py` —
  adds six Plan 076 contracts: source-lock linked-marker,
  source-lock measured library digests, build-manifest schema
  library-digest requirement, CMake `I2PD_PLAN076_LINKED` +
  `I2PD_LIB_DIR` requirement, build-driver library-build
  commands, and driver source real i2pd API surface.

The Plan 076 closure boundary does **not** require a mixed-router
pass; the closure is a real binary with verifiable source linkage
and locally testable inspect / listen / dial behaviour. On the
host in this checkout the inspection path succeeds locally with a
real signed RouterInfo, the listen path binds a real NTCP2
listener, and the dial path reaches the real
`Transports::SendMessage` surface. The
`10/10 fresh-state qualification` remains to be produced in the
Plan 046 rootless sealed-namespace lane or the Plan 048/049
Multipass recovery lane; on this host (the Plan 046
`apparmor_restrict_on` negative baseline) the qualification
receipt records the typed host blocker and an all-zero attempt
count. NTCP2 stays experimental and non-advertised.

Required focused checks for Plan 076:

```text
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_i2pd_direct_driver.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_i2pd_direct_control.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_loopback_smoke.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan065.py'
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-rootless-interop-boundary.sh
bash scripts/check-multipass-interop-boundary.sh
bash scripts/check-ntcp2-loopback-smoke-boundary.sh
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
cargo fmt --all --check
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
```

## Plan 077 constrained-host execution lane

Plan 077 is closed with a typed no-full-runtime-lane result on the current
host. Use `bash scripts/interop/probe-constrained-host-lanes.sh` before any
future constrained-host work. Its order is existing accessible rootful
Docker (`--network none`), QEMU TCG (`-nic none`), reduced inherited
descriptors plus `no_new_privs`/seccomp, manual remote Linux, then no lane.
The probe is read-only and must not install software, invoke privilege
escalation, modify host policy/networking, retry rootless or Multipass, or
start a router.

The common manifest and sanitized qualification schema live in
`tests/integration/ntcp2/harness/execution_lane.py`. The static boundary is
`scripts/check-constrained-host-lane-boundary.sh`; focused tests are
`test_execution_lane.py`. The historical Plan 077 probe was reduced-scope,
but Plan 080 later qualified the owned Multipass guest used for the single
Plan 078 attempt. Do not treat either capability probing or a stale guest as
protocol evidence; consult `plans/080-status.md` for the qualified-lane
record.

## Current active sequence amendment (2026-08-06, plan095 / Plan 095)

The active plan identifier is `plan095` (lowercase file-token) and
`Plan 095` (display form). The active sequence is documented in
`plans/095-ci-host-loopback-live-wire-evidence-lane.md`.

The active authority is [Plan 085](plans/085-milestone-3-host-loopback-development-execution-roadmap.md),
followed by Plan 086 (status correction + `host-loopback-development`
lane enablement, **closed on this host**), Plan 087 (first real
`i2pr -> i2pd` forward probe), Plan 090 (i2pd RouterInfo
corrective pass: four behavior-neutral driver corrections, **corrections
landed on this host**), Plan 091 (forward NTCP2 Noise-handshake corrective
pass: i2pd direct driver preconditions landed — `SetNetID`, explicit logger
start/stop, `tcp_accepted` wait, symmetric `DeliveryStatus` send, **corrections
landed on this host, forward direction still blocked**), Plan 092
(forward-handshake evidence integrity and ownership closure:
privacy-safe handshake stage observation schema, terminal-counter
preservation, current-run event dedup, status authority rewrite,
dedicated regression matrix, **partial / superseded-by-plan093**),
Plan 093 (Plan 087 forward data-phase and reference-observer
closure: i2pd observer reset/generation/lifecycle correction,
bounded sequence ring with exact target predicate waits, i2pr
bounded multi-frame receive oracle, i2pr binary provenance
binding, privacy-safe source reclassification of
`NTCP2: Receive length read error` from SessionRequest (handshake
read) to `HandleReceivedLength` (data-phase length reader), and the
exact-bidirectional-DeliveryStatus forward direction pass,
**implementation-landed-closure-incomplete**), Plan 094 (Plan
093 completion pass and Plan 087 -> Plan 088 handoff: real per-process
invocation IDs, shared canonical event-ingestion primitive, exact
target pass classification, build/binary provenance binding, Plan
094 focused regression matrix, prunes the stale `plan_093b` token,
**implementation-landed-live-closure-environment-blocked**),
Plan 095 (CI host-loopback live-wire evidence lane: GitHub Actions
`ubuntu-24.04` workflow that runs the Plan 086 host-loopback-
development lane on a fresh VM, contract/build/forward-instrumented/
forward-control/validate-gate job sequence with provenance-bound
binaries, sanitized CI contract record, and Plan 095 CI gate
record, **active single next executable plan**),
and Plan 088 (reverse `i2pd -> i2pr` probe and development decision),
and the [Plan 072/079 gate amendment](plans/072-079-gate-plan-088.md)
that moves the Plan 072 and Plan 079 gates from Plan 084 to Plan 088.
Plan 082 is implemented and closed; Plans 083 and 084 are implementation
complete but execution pending; Plan 086 added the
`host-loopback-development` topology kind, the
`HostLoopbackDevelopmentPlacement`, the bounded literal IPv4 loopback
acceptance, the thin wrapper
`scripts/interop/run-minimal-i2pd-host-loopback-probe.py`, and the
listener-only preflight. Plan 095 adds the dedicated
`.github/workflows/ntcp2-interop-host-loopback-development.yml`
workflow and the `tests/integration/ntcp2/harness/test_plan095.py`
test matrix. The Plan 084 historical `lane-invalidated`
closure is reclassified as "runner implementation completed;
required reverse wire execution never occurred." The Plan 080
Multipass lane is qualified and the Plan 076 i2pd driver is real, but
Plan 078 stopped before protocol execution; no real TCP connection,
NTCP2 handshake, authenticated frame, or I2NP DeliveryStatus attempt is
retained. The Plan 078 historical `blocked-protocol-defect` label must
not be used as the current diagnosis.

The Plan 087 implementation surface landed: the canonical Plan 083
runner now drives the placement-owned concurrent i2pd listener and
i2pr dialer (via `HostLoopbackDevelopmentPlacement.popen`) and copies
the i2pd-exported RouterInfo into the scenario exchange path with a
verified digest; the wrapper threads the missing `reference_tree_sha256`
and `source_inspection_record_sha256` provenance digests through to
the i2pd direct driver invocation. The Plan 090 corrective pass
applied four behavior-neutral corrections to the i2pd direct
driver source
(`tests/integration/ntcp2/reference-drivers/i2pd/src/i2pd_ntcp2_interop_driver.cpp`):

- `set_bool_option("ntcp2.published", true)` — store the option as
  `bool` (was stored as `int` by the Plan 064 helper, which
  silently failed to update the i2pd option map).
- `i2p::config::ParseCmdline(1, fake_argv, ignoreUnknown=true)` +
  `Finalize()` — populate the i2pd option store with declared
  defaults before the driver mutates individual options (the
  `boost::program_options` map is empty until `store()` runs).
- `set_uint16_option` helper — store `port` and `ntcp2.port` as
  `uint16_t` (was stored as `int`, which throws
  `boost::bad_any_cast` on extraction).
- `i2p::transport::transports.SetCheckReserved(false)` — disable
  reserved-range filtering so loopback addresses survive
  `RouterInfo::ReadFromBuffer` deserialization
  (`RouterInfo.cpp` lines 256-262).

The driver also fails closed with
`router-info-endpoint-mismatch` if the authoritative in-memory
RouterInfo does not carry the exact configured NTCP2 endpoint.

The Plan 091 corrective pass added four more i2pd direct driver
corrections:

- `i2p::context.SetNetID(cfg.network_id)` between
  `i2p::crypto::InitCrypto(false)` and `i2p::context.Init()` —
  the i2pd standalone daemon performs the same call; without
  it `RouterContext::GetNetID()` returns the default
  `I2PD_NET_ID` (=2) and the NTCP2 listener rejects the
  SessionRequest with `networkID 99 mismatch. Expected 2`.
- `i2p::log::Logger().SendTo(<data_dir>/i2pd.log)` plus
  `Logger().Start()` after `InitCrypto` and `Logger().Stop()`
  in `main()` before each `run_*` return. Without the explicit
  `Start` the global `Log::Log` thread is not running and
  `LogPrint` calls in the i2pd transport are no-ops; the
  `Stop` in `main()` joins the background thread and prevents
  the `terminate called without an active exception` abort on
  shutdown.
- `run_listen` waits boundedly for the i2pd transport to
  record a real TCP accept through the Plan 064 observer
  (`WaitForTcpAccepted`) and emits a `tcp_accepted` event
  before continuing; without it the i2pd listener reported
  `ntcp2_authenticated` only on stale observer slots from a
  previous run.
- `run_listen` composes a `DeliveryStatus` with the exact
  correlation `message_id` and submits it through the real
  i2pd transport (`transports.SendMessage(peer_ident_hash,
  reply)`); without it the i2pr's
  `receive_delivery_status` block reports
  `receiver_delivery_status_missing` and the Plan 065
  directional predicate cannot pass.

The Plan 091 first clean committed-head attempt authenticated
the i2pd listener (`listener_ready`), the i2pr dialer started,
the i2pd NTCP2 transport accepted the TCP connection, the i2pd
log shows `NTCP2: SessionRequest read error: End of file` (the
i2pd transport read zero bytes on the first body read), and
the i2pr status log shows `tcp_connected` immediately followed
by `terminal, result: rejected, reason_code:
receiver_delivery_status_missing`. The wire trace, the i2pd
log, and the i2pr status do not yet agree on which side terminates
the Noise handshake: the i2pr dialer enters
`drive_initiator_handshake` and the runner serializes the
forward direction as `terminal_result = protocol_rejected,
highest_stage_reached = tcp_connected, reason_code =
reference-events-missing` because the i2pd listener never
emits `ntcp2_authenticated`. The retained record is preserved
at `/tmp/opencode/plan091-evidence/forward/forward-record.json`;
see `plans/091-status.md` for the closure record, exact live
command, recorded digests, and the ownership analysis.

The Plan 092 first clean committed-head reproduction recorded
the same forward direction outcome but with the new privacy-safe
observation infrastructure in place: the i2pd log shows
`NTCP2: Receive length read error: End of file` on the first
SessionRequest prefix read while the i2pr status counters carry
`authenticated: 1`, `frames_sent: 1`, `frames_received: 1`,
`i2np_sent: 1`, `i2np_received: 0`. The retained record is
preserved at
`/tmp/opencode/plan092-test-1/forward-record.json` with
`record_sha256 = 696aa1339d3d950f9fec2a2e0b1f5bede2035761a71e167af6ab28b249cc998d`;
see `plans/092-status.md` for the original Branch A ownership
selection. Plan 093 supersedes that ownership analysis with a
privacy-safe source reclassification: the i2pd diagnostic string
originates from the data-phase length reader (`HandleReceivedLength`)
and not from the SessionRequest handshake read. Plan 094 is the
single next executable plan that closes Plan 093 without
reopening its already-landed NTCP2 data-phase design. Plan 088
remains blocked on the Plan 094 completion pass.

Plan 082 provides the test-only `i2pr-interop ntcp2 prepare` and
`validate-scenario` operations, authentic RouterInfo/hash preparation,
canonical pre-launch run identity, precise pre-protocol errors, and truthful
process accounting. Preparation and strict scenario validation open no socket,
run no i2pd peer, and produce no interoperability evidence. Plan 083 implements
the forward ``i2pr -> i2pd`` probe schema, runner orchestration, and focused
test matrix. Plan 084 implements the reverse ``i2pd -> i2pr`` probe schema,
runner orchestration, focused test matrix, and static boundary check; the
reverse probe record schema and runner travel with the repository unchanged
as the Plan 088 implementation surface. Plan 085 introduces the bounded
`host-loopback-development` topology kind that allows literal IPv4 loopback
protocol execution on the constrained host under the development-only lane
contract. Plan 086 closes as `host-loopback-development-ready` (or records
`manual-isolated-fallback-required`) before Plan 087 begins. Plan 087
closes as `passed` only after a TCP-authenticated
`i2pr -> i2pd` first-instrumented pass; the Plan 091 instrumented
attempt reached TCP authentication but the NTCP2 handshake closed
before the i2pd transport recorded a `SessionRequest` body. Plan 087 must
close (as `passed` or as a localized defect that authenticates the
data phase) before Plan 088 begins. Plan 091 closes as `passed` only
after Plan 087 records `status = passed` with both an instrumented
and a control record digest; the Plan 091 corrections are landed on
this host and the forward direction did not pass. Plan 095 is the
single next executable plan; it implements the GitHub Actions
`ubuntu-24.04` host-loopback live-wire evidence lane that
supersedes the local host environment-blocked path that Plan 094
expected to run. Plan 095 adds the contract/build/forward-instrumented/
forward-control/validate-gate job sequence, the dedicated
`.github/workflows/ntcp2-interop-host-loopback-development.yml`
workflow, the `tests/integration/ntcp2/harness/test_plan095.py`
focused test matrix, the CI environment blocker vocabulary, and
the sanitized CI gate record. Plan 094 remains
implementation-landed; its live closure environment is blocked on
this host. The Plan 088 active development decision remains
`insufficient-evidence` until Plan 095 closes with a passing
instrumented and a passing control forward record from the same CI
evidence pair. Plan 079 remains blocked until `plans/088-status.md`
records `decision = two-way-development-probe-passed`. Plan 072
remains inactive until `plans/088-status.md` records `decision =
ambiguous-reference-divergence` with one exact role/stage diagnostic
question. NTCP2 stays experimental and non-advertised. See
`plans/082-status.md`, `plans/083-status.md`, `plans/084-status.md`,
`plans/086-status.md`, `plans/087-status.md`, `plans/088-status.md`,
`plans/091-status.md`, `plans/092-status.md`, `plans/093-status.md`,
`plans/094-plan093-completion-and-plan087-to-plan088-handoff.md`,
`plans/095-ci-host-loopback-live-wire-evidence-lane.md`, and
`plans/096-plan095-ci-workflow-correctness-and-pre-dispatch-closure.md`.

Plan 096 is the active workflow correctness and pre-dispatch
closure pass. The plan delivers a static regression matrix
(`tests/integration/ntcp2/harness/test_plan096.py`), a pre-dispatch
audit script (`scripts/check-plan095-workflow.sh`), and the four
demonstrated workflow corrections: explicit i2pr build path,
disjoint sanitized evidence, embedded Python import audit, and
canonical tracked-source digest.

Plan 097 is the active narrow corrective pass over the Plan 095
GitHub Actions workflow that closes two workflow defects that
remained after Plan 096: the producer/consumer artifact path
identity mismatch and the disposable run-root cleanup that
only deleted descendants with a suppressed absence assertion.
Plan 097 introduces one canonical absolute `BUILD_OUTPUT` path
used by every producer, verifier, manifest, uploader, and live
consumer; adds an exact path guard before `rm -rf --`; and
removes every suppression from the post-cleanup absence
assertion. The Plan 097 regression matrix
(`tests/integration/ntcp2/harness/test_plan097.py`) and the
extended pre-dispatch audit (`scripts/check-plan095-workflow.sh`)
 are green locally. After Plan 097 lands, the current status of
the active sequence is:

```text
plan_093 = implementation-landed-closure-incomplete
plan_094 = implementation-landed-live-closure-environment-blocked
plan_095 = ci-live-wire-lane-corrected-awaiting-one-authoritative-run
plan_096 = passed-pre-dispatch-workflow-correction
plan_097 = passed-artifact-path-and-cleanup-correction
plan_087 = open-pending-plan095-ci-forward-evidence-pair
plan_088 = blocked-pending-plan095-ci-closure
plan_079 = blocked-pending-plan088-two-way-pass
plan_072 = inactive-pending-plan088-ambiguity
ntcp2    = experimental-non-advertised
```

Plan 095 remains the single next executable plan; exactly one
manual Plan 095 GitHub Actions dispatch follows the Plan 097
correction commit.

## Plan 098 Plan 095 runner/provenance boundary corrective pass

Plan 098 is the active narrow corrective pass over the runner
and wrapper provenance surfaces that the first authoritative
Plan 095 manual CI dispatch exposed on 2026-08-10. The
authoritative run advanced through the contract, build, and
live runner launch phases but failed closed before any TCP or
NTCP2 wire activity. The runner reconstructed a non-authoritative
`repo_root / target / debug / i2pr-interop` path instead of
using the canonical absolute artifact path supplied by the
wrapper; the runner therefore returned
`pre-protocol-preparation-failed` before launching `i2pr-interop
ntcp2 prepare`. That result must **not** be interpreted as a
wire-level NTCP2 failure.

Plan 098 corrects the runner/provenance ownership boundary and
the adjacent provenance defects in a single coherent pass:

- **Defect A**: the runner reconstructed the i2pr binary path
  instead of consuming the explicit caller-supplied path.
- **Defect B**: the reverse and preflight runners reproduced
  the same path reconstruction defect.
- **Defect C**: the runner aliased a single generic manifest
  digest into both the i2pr and i2pd build-manifest fields.
- **Defect D**: the wrapper used the instrumented build
  manifest regardless of the requested role.
- **Defect E**: the forward runner hard-coded
  `attempt_kind=instrumented` in the underlying record.
- **Defect F**: `build-driver.sh` used a recursive
  `find -type f` digest that drifts from the workflow's
  canonical `git ls-files` algorithm.
- **Defect G**: the final gate checked nonzeroness but not full
  provenance equivalence against the downloaded artifacts.
- **Defect H**: `plans/088-status.md` misclassified the August 10
  pre-protocol rejection as a wire-level result.

Plan 098 lands:

- `tests/integration/ntcp2/harness/plan083_runner.py`,
  `tests/integration/ntcp2/harness/plan084_runner.py`, and
  `tests/integration/ntcp2/harness/preflight_runner.py` —
  every runner now accepts an explicit `i2pr_binary: Path`
  argument, measures the file bytes against the supplied
  SHA-256, and fails closed with a typed
  `pre-protocol-preparation-failed` rejection when the path is
  missing, not a regular file, or its measured digest does
  not match. The `i2pr_build_manifest_sha256` and
  `i2pd_build_manifest_sha256` fields are independently
  measured; the runner no longer aliases a generic manifest
  digest into both artifact classes. The `attempt_kind`
  argument reaches the underlying record instead of being
  hard-coded.
- `scripts/interop/run-minimal-i2pd-host-loopback-probe.py`
  — the wrapper now threads the exact caller-supplied
  `--i2pr-binary` path to every runner, validates the
  `--attempt-kind` against the i2pd driver binary filename
  and the role-specific build manifest, and measures the
  canonical tracked-tree digest via
  `_canonical_tracked_tree_digest` (which must stay in
  lock-step with the workflow and the build script).
- `tests/integration/ntcp2/reference-drivers/i2pd/build-driver.sh`
  — the source-tree digest is now computed from
  `git ls-files -z` (excluding the `.git` administrative
  tree) so the build manifest digest matches the workflow
  digest byte-for-byte.
- `.github/workflows/ntcp2-interop-host-loopback-development.yml`
  — the live jobs thread `--attempt-kind instrumented` and
  `--attempt-kind control` explicitly, and the validate-gate
  job enforces exact provenance equivalence between the
  sanitized records and the actual downloaded artifacts and
  role-specific manifests.
- `tests/integration/ntcp2/harness/test_plan098.py` — the
  Plan 098 regression matrix (15 cases) covering the
  runner-boundary ownership contract, the wrapper path
  threading, the distinct i2pr/i2pd manifests, the
  role/manifest binding, the canonical tracked-tree
  digest, and the workflow final-gate checks.
- `scripts/check-plan095-workflow.sh` and
  `scripts/check-ntcp2-interoperability.sh` — extended to
  enforce the Plan 098 ownership invariants: the runner must
  accept an explicit `i2pr_binary` path, the wrapper must
  expose `--attempt-kind`, `build-driver.sh` must use the
  canonical tracked-source identity, and the workflow final
  gate must validate role-specific digests.
- Status authority corrections in `plans/087-status.md` and
  `plans/088-status.md`. The new `plan_098` token is
  `passed-runner-provenance-boundary-correction`. The August
  10 result is reclassified as a pre-protocol runner/provenance
  failure with no TCP/NTCP2 wire conclusion.
- Documentation propagation in `README.md`, this file, the
  `i2pr-ntcp2-interop` skill, and
  `docs/architecture/interop-apparatus.md`.

After Plan 098 lands, the current status of the active sequence
is:

```text
plan_093 = implementation-landed-closure-incomplete
plan_094 = implementation-landed-live-closure-environment-blocked
plan_095 = active-runner-provenance-corrected-awaiting-authoritative-rerun
plan_096 = passed-pre-dispatch-workflow-correction
plan_097 = passed-artifact-path-and-cleanup-correction
plan_098 = passed-runner-provenance-boundary-correction
plan_087 = open-pending-plan095-ci-forward-evidence-pair
plan_088 = blocked-pending-plan095-ci-closure
plan_079 = blocked-pending-plan088-two-way-pass
plan_072 = inactive-pending-plan088-ambiguity
ntcp2    = experimental-non-advertised
```

Plan 095 remains the single next executable plan; exactly one
manual Plan 095 GitHub Actions dispatch follows the Plan 098
correction commit. Plan 098 does **not** mark Plan 095 or
Plan 087 passed.

Required focused checks for Plan 098:

```text
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan098.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan097.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan096.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan095.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan094.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan093.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan083_runner.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_i2pd_direct_driver.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_i2pd_direct_control.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan088.py'
bash scripts/check-plan095-workflow.sh
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-ntcp2-vectors.sh
bash scripts/check-rootless-interop-boundary.sh
bash scripts/check-multipass-interop-boundary.sh
bash scripts/check-ntcp2-loopback-smoke-boundary.sh
cargo +1.95.0 fmt --all --check
cargo +1.95.0 check --locked --workspace --all-targets
cargo +1.95.0 test --locked --workspace
cargo +1.95.0 clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo +1.95.0 doc --locked --workspace --no-deps
git diff --check
```

## Plan 083 minimal i2pr-to-i2pd wire probe

Plan 083 owns the first real `i2pr -> i2pd` direction. The plan
introduces `tests/integration/ntcp2/harness/minimal_i2pd_probe.py`
with the locked record schema `i2pr-minimal-i2pd-probe-v1`, the
strictly-increasing stage model, the bounded terminal-result and
reason-code sets, and the per-process counters. The schema refuses
the broad `typed-harness-operation-failed` reason code, mandates the
Plan 082 run-identity contract, and forbids raw payload, private
keys, Noise state, transcripts, and RouterInfo bytes. The focused
matrices are `tests/integration/ntcp2/harness/test_minimal_i2pd_probe.py`
and `tests/integration/ntcp2/harness/test_plan083.py`. The probe
itself is a development diagnostic: a passed probe record does not
authorize Plan 079 (repeated development validation) or Plan 073
(release qualification); those gates remain owned by their own
plans. Plan 083 is implemented as the in-process schema, the
focused test matrix, and the runner orchestration at
`tests/integration/ntcp2/harness/plan083_runner.py`.

The runner owns the 11-step execution architecture and is
structurally incapable of producing a mixed-router pass unless it
launches one real i2pr process and one configured real reference
process and consumes authentic structured events from both. The
real-process entry point is
`plan083_runner.execute_real_probe(...)`, which:

1. validates the lane (``rootless-sealed-single-netns`` or
   ``multipass-owned-guest``);
2. prepares i2pr state via the Plan 082 `prepare` command and copies
   the persisted `router.info` into the exchange directory;
3. runs the Plan 076 i2pd direct driver in inspect mode to capture
   the local i2pd Router Hash from the `router_info_exported`
   event;
4. renders and validates the Plan 065 strict scenario
   (`i2pr-launcher-scenario-v2`) with the per-run
   `delivery_status_message_id` and 64-hex Router Hash pair;
5. launches the i2pd direct driver in listen mode as a separate
   subprocess;
6. launches the i2pr launcher in dial mode as a separate subprocess;
7. consumes the i2pd `events.ndjson` stream and the i2pr JSONL
   status stream concurrently, recording authentic
   `ntcp2_authenticated`, `frame_emitted`,
   `frame_authenticated_and_decrypted`, and `i2np_message_decoded`
   events only when observed;
8. bounded shutdown and cleanup of both subprocesses;
9. writes one sanitized `i2pr-minimal-i2pd-probe-v1` record.

The runner refuses to fall back to SAM, HTTP, support-topology, or
synthetic-fallback helpers for any primary direction; the C++ i2pd
direct driver is the only allowlisted reference driver mode. The
runner never imports Plan 056/066 candidate, bundle, certificate,
rootless-topology, or Multipass authority. No real wire attempt has
been executed in this checkout because the host is the Plan 046
`apparmor_restrict_on` negative baseline and the Plan 080 Multipass
guest cannot complete on this constrained host.

### Plan 083 focused checks

```text
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan083.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_minimal_i2pd_probe.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan083_runner.py'
bash scripts/check-ntcp2-interoperability.sh
```

## Plan 078 first real i2pd two-way execution

Plan 078 used the Plan 080-qualified owned guest, then stopped before TCP at
the i2pr pre-protocol RouterInfo stage. The retained result is not protocol
rejection evidence and does not authorize Plan 079. Do not reuse stale or
unowned guests, and do not infer protocol progress from process lifetime or a
port probe. The closure record is [plans/078-status.md](plans/078-status.md);
the qualified-lane record is [plans/080-status.md](plans/080-status.md).

## Plan 072 activation gate

Plan 072 remains inactive. It may be activated only after Plan 088 reaches a
real wire stage, source/specification review cannot identify ownership, and
`plans/088-status.md` records exactly
`decision = ambiguous-reference-divergence` plus one precise role/stage
question. Plan 082 preparation and any pre-protocol failure do not satisfy
this gate. Emissary driver code and general qualification work are out of
scope until then. See the activation amendment at
[plans/072-activation-amendment-plan-084.md](plans/072-activation-amendment-plan-084.md)
for the full gate, scope, and handoff rule.

## Plan 088 reverse host-loopback probe and development decision

Plan 088 owns the reverse `i2pd -> i2pr` direction and the active
development decision. Plan 088 inherits the Plan 086
`host-loopback-development` lane (literal IPv4 loopback, network ID
99, development-only) and reuses the Plan 084 reverse probe record
schema (`i2pr-minimal-i2pd-reverse-probe-v1`) and runner orchestration
module (`plan084_runner.py`) unchanged.

Plan 088 lands:

- `tests/integration/ntcp2/harness/minimal_i2pd_probe.py` — the
  shared `ALLOWED_TOPOLOGY_KINDS` set accepts
  `host-loopback-development`; the new bounded
  `DEVELOPMENT_ONLY_TOPOLOGY_KINDS` marker records the development-only
  classification.
- `tests/integration/ntcp2/harness/plan083_runner.py` and
  `tests/integration/ntcp2/harness/plan084_runner.py` — both runners
  accept `host-loopback-development` in their lane validators.
- `tests/integration/ntcp2/harness/test_plan088.py` — the Plan 088
  test matrix (35 cases) covering the bounded development decision
  vocabulary, the Plan 079 entry gate, the Plan 072 activation gate,
  the handoff fields, the development-only topology contract, the
  reverse probe schema contract, the cross-direction rejection, and
  the module boundary.
- `scripts/check-ntcp2-interoperability.sh` — enforces the Plan 088
  test matrix presence, the locked decision vocabulary, the
  `host-loopback-development` topology coverage, the plan-of-record
  reference, the `plans/088-status.md` decision token, and the
  prohibition of the legacy `lane-invalidated` and
  `same-stage-two-way-i2pr-defect` tokens.

The Plan 088 development decision vocabulary is exactly five values:
`two-way-development-probe-passed`, `one-way-passed-reverse-defect`,
`ambiguous-reference-divergence`, `manual-isolated-fallback-required`,
`insufficient-evidence`. Only `two-way-development-probe-passed` may
unblock Plan 079; only `ambiguous-reference-divergence` may activate
Plan 072. The historical `lane-invalidated` and
`same-stage-two-way-i2pr-defect` tokens are forbidden. The static
boundary checker enforces this.

On this host the recorded decision is `insufficient-evidence` because
the Plan 090 forward direction recorded a TCP-established protocol
failure and no `i2np_message_decoded` event has been retained. The
Plan 088 implementation surface travels with the repository unchanged
for any future host where the Plan 086 lane becomes executable or
the Plan 089 manual-isolated fallback becomes available. NTCP2
stays experimental and non-advertised. See
[plans/088-status.md](plans/088-status.md) for the closure record.

Plan 090 is the i2pd RouterInfo and Plan 087 evidence corrective
pass. The plan delivered four behavior-neutral corrections in
the i2pd direct driver
(`tests/integration/ntcp2/reference-drivers/i2pd/src/i2pd_ntcp2_interop_driver.cpp`),
a Plan 083 pre-TCP classifier that forbids generic
`protocol_rejected` before `tcp_connected`, the placement-owned
host-loopback `validate-scenario` invocation, and the
`test_plan090.py` test matrix covering the source verification,
driver binary, control parity, pre-TCP classification, placement
validation, and record validation. The source-verification
record (`tests/integration/ntcp2/reference-drivers/source-verification.md`
"Plan 090 verified RouterInfo lifecycle") documents the
pinned-source lifecycle and config/export ownership. The
static boundary check (`scripts/check-ntcp2-interoperability.sh`)
enforces the driver corrections, the lifecycle documentation, and
the test matrix presence.

The Plan 090 corrections landed on this host. The first clean
committed-head forward attempt authenticated the i2pd listener
and reached TCP, then the NTCP2 Noise handshake closed the
socket before the i2pr initiator reached `ntcp2_authenticated`.
The Plan 090 closure remains open: the forward direction did not
pass. Per the Plan 090 "Forward attempt reaches TCP and fails
protocol" branch, the failed record is preserved and Plan 088 is
not allowed to run until the forward direction passes.

Required focused checks for Plan 088:

```text
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan090.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan088.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan086.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan084.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan083.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan083_runner.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_minimal_i2pd_probe.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_minimal_i2pd_reverse_probe.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_i2pd_direct_driver.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_i2pd_direct_control.py'
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
git diff --check
```

## Plan 086 host-loopback development lane

Plan 086 introduces the `host-loopback-development` topology kind
to the canonical Plan 083/084 runners, adds the bounded
`HostLoopbackDevelopmentPlacement`, allows literal IPv4 `127.0.0.1`
only under that topology, and lands the thin wrapper
`scripts/interop/run-minimal-i2pd-host-loopback-probe.py`. The
lane is development-only; it never satisfies a release or
isolation predicate and must not be used for Plan 079/Plan 073
evidence. The closure states are exactly three values:
`host-loopback-development-ready`,
`manual-isolated-fallback-required`, and
`blocked-artifact-or-build-defect`. The legacy
`lane-invalidated` and `same-stage-two-way-i2pr-defect` tokens
are forbidden. The static boundary checker enforces the topology
extension, the placement class, the schema marker, the wrapper
script, and the bounded closure state.

The wrapper accepts only the two i2pd directions, refuses every
release/support profile flag, and writes a single sanitized
record. The preflight path stops before any peer connection
completes; the forward and reverse paths reuse the canonical
Plan 083/084 runners unchanged. Plan 086 closes before Plan 087
begins, and the Plan 086 status record gates the Plan 087/088
execution authority.

Required focused checks for Plan 086:

```text
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan086.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan088.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan083.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan084.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan082.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_i2pr_prepare.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan065.py'
cargo fmt --all --check
cargo check -p i2pr-interop
cargo test -p i2pr-interop
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
git diff --check
```

## Plan 102 Milestone 4 RouterInfo/NetDB authority and the Plan 102 amendment (active parent; amendment closed)

[Plan 102](plans/102-milestone-4-routerinfo-netdb-authority-and-roadmap.md)
is the active Milestone 4 parent authority that supersedes the
historical Milestone 3 "active" blocks for the purpose of
continuing router development. The retained Plan 099/100/101
NTCP2 result (`protocol-defect-localized` at `noise_authenticated`,
normal-daemon NTCP2 disabled and unenableable) is preserved as
the authoritative NTCP2 development record. The Plan 099/100/101
status blocks in this file and in `README.md` describe that
result; the next substantial product work is now governed by
Plan 102 and its child sequence (Plans 103 → 104 → 105 → 106).
The local child sequence has since completed through Plan 113; the formal
amendment closure and future-plan unblock audit are recorded in
`plans/102-amendment-status.md`.

### Plan 102 amendment — exploratory-tunnel dependency

[Plan 102 amendment](plans/102-amendment-exploratory-tunnel-dependency.md)
corrects an over-optimistic wording in the first Plan 102 draft.
The current I2P `DatabaseLookup` operation uses an outbound
exploratory tunnel and requests the response through an inbound
exploratory tunnel; exploratory tunnels are Milestone 5 scope.
Therefore a standards-conformant live RouterInfo lookup cannot
complete inside the Plan 103–106 implementation sequence merely
by re-entering NTCP2 or another direct router transport.

The authoritative Plan 102 sequence is:

```text
Plan 103  RouterInfo validation + bounded local NetDB     [closed]
    -> Plan 104  persistent cache + SU3 reseed trust/ingestion [closed]
     -> Plan 105  transport-neutral lookup/store/publication state machines [closed]
     -> Plan 106  daemon/bootstrap integration                 [closed]
-> Plan 107  Milestone 5 exploratory tunnel substrate     [closed]
      -> Plan 108  ECIES-X25519 short record construction core   [superseded-by-plan109]
      -> Plan 109  short-record + Noise-N conformance correction  [superseded-by-plan111]
      -> Plan 110  multi-record preprocessing + local conformance closure [superseded-by-plan111]
      -> Plan 111  final local short-build conformance correction [passed-final-local-short-build-conformance]
      -> Plan 112  outbound pre-delivery closure [passed-outbound-pre-delivery-closure]
-> Plan 113  inbound reference reconciliation [passed-inbound-reference-reconciliation]
      -> Plan 114  terminal routing + tunnel-chain correction    [passed-terminal-routing-chain-correction]
      -> Plan 115  canonical production I2NP bridge + Q0 native Emissary OBEP reply [passed-emissary-q0-construction-and-obep-reply-only]
      -> Plan 116  local tunnel data plane [passed-final-local-closure]
      -> Plan 117  live exploratory/NetDB integration [closed-for-progression-with-evidence-gap]
      -> Plan 118  planning authority cleanup + Plan 117 disposition [closed]
      -> Plan 119  LeaseSet2 protocol foundation [passed-leaseset2-protocol-foundation]
```

Plan 106 closed the local/bootstrap implementation phase; Plan 107
landed the runtime-neutral exploratory pool, the typed build-record
layout surface, the build-cryptography seam, and the reply-path
provider that flips the Plan 106 NetDB seam from
`BlockedExploratoryTunnelUnavailable` to `Available` once a real
inbound tunnel is registered. Plan 108 landed the local
short-record construction architecture but its wire/cryptographic
algorithm was **not** protocol-conformant against the current
official I2P Tunnel Creation Specification. Plan 109 corrected the
wire format, Noise-N transcript, layer-encryption type, request/
reply key derivation, response codes, and 218-byte envelope layout
but missed four defects that Plan 111 reopens. Plan 110 added
randomized slot allocation, fake records, raw ChaCha20
preprocessing/postprocessing, and the one-byte-count STBM/OTBRM
payload framing but inherited the Plan 109 defects. Plan 111
closed as `passed-final-local-short-build-conformance` and
corrected the remaining defects (Noise null prologue, single-HKDF
`es` split, slot byte at offset 4, 8-byte OBEP garlic tag, explicit
per-hop tunnel IDs, role-aware hop processor, frozen independent
fixed vectors). Plan 113 then closed inbound as
`reference-compatible-spec-text-discrepancy`: pinned Java I2P and
i2pd agree on exactly one originator fake, while the final-spec
prose has no concrete separate plaintext creator-key encoding.
Plan 114 closed as `passed-terminal-routing-chain-correction`:
explicit outbound `outbound_reply_router` and inbound
`originator_hash` terminal-routing metadata, intermediate
`hops[i].next_tunnel == hops[i+1].receive_tunnel` chain continuity
enforced at both the high-level `ShortBuildPath::validate()`
boundary and the lower-level `prepare_short_build_message()`
entry point, and strict outbound/inbound E2E trajectories that
deterministically reach `Established` without the prior
`InvalidReply OR Established` permissive acceptance. Plan 115
closed the canonical production I2NP bridge
(`ShortBuildI2npBridge`) that converts a
`ShortBuildAction::Deliver` into a complete I2NP type-25 message
without double-prefixing the STBM record count, and closed the
Plan 115 Q0 independent-consumer seam on this host as
`passed-emissary-q0-construction-and-obep-reply-only` against
pinned Emissary revision
`9b43484a21d5a1291c4881cdae62a36c527f8c0f` (emissary-core 0.4.0).
The i2pr-emitted STBM is consumed by Emissary's native
short-build handler (the same code path Emissary uses in
production), Emissary replies with a `TunnelGateway` wrapping a
Garlic inner message, and a feedback channel is returned. Q1
(authenticated transport delivery) and Q2 (reply round-trip to
`Established`) remain pending and depend on a qualified external
delivery lane. Milestone 4A is now
`local-foundation-complete-short-build-outbound-conformant-fixed-vectors`
with `inbound_short_build = locally-reference-compatible` and
`production_i2np_bridge = locally-conformant-no-double-prefix`
and `independent_short_build = passed-emissary-q0-native-consumer`.
After the Plan 116 terminal cleanup pass, the local tunnel data
plane is `passed-final-local-closure`. Plan 117 closes per Plan 118
as `closed-for-progression-with-evidence-gap`: the in-tree pinned
Emissary test reaches native OBEP admission and reply AEAD
opening, then rejects the reference-side request-prefixed reply
during strict i2pr Mapping decoding. Parser-only compatibility is
not native interoperability evidence.
A direct `DatabaseLookup` over NTCP2 is not accepted as a
substitute for the standard exploratory-tunnel path. Plan 119
closed as `passed-leaseset2-protocol-foundation` per
[`plans/119-status.md`](plans/119-status.md); the ordinary
online-signed published Standard LeaseSet2 carrier is wired into
`i2pr-proto` (40-byte `Lease2`, `LeaseSet2Header`,
`LeaseSet2EncryptionKey`, canonical `Mapping` options, signature
domain `0x03 || signed_bytes`) and into `i2pr-netdb`
(`ValidatedLeaseSet2`, `LeaseSet2Store`, `DestinationHash`,
`LookupKind::LeaseSet2`). `DatabaseStoreData::LeaseSet2` replaces
the type-3 `Deferred` payload for the ordinary subset; types 5/7
remain explicitly deferred. The next executable plan is **Plan
120** (destination lifecycle and dedicated tunnel pools) under the
Milestone 6 router-construction roadmap in
[`plans/118-123-milestone6-router-construction-roadmap.md`](plans/118-123-milestone6-router-construction-roadmap.md).
Authenticated external transport and final mixed-router certificate
remain separate deferred evidence items tracked in the same
roadmap's external acceptance debt ledger.

## Plan 106 daemon NetDB/bootstrap integration and Milestone 5 handoff (closed)

Plan 106 composes the Plan 103/104/105 runtime-neutral surfaces into
the real `i2pr` daemon without activating any I2P transport:

- new `[netdb]` and `[reseed]` configuration sections with hard
  ceilings (`max_records ≤ 65536`, `max_encoded_bytes ≤ 64 MiB`,
  `max_su3_bytes ≤ 16 MiB`, etc.);
- a bounded `BootstrapState` vocabulary
  (`empty`, `cache-sufficient`, `reseed-required`, `reseeding`,
  `ready-for-network-integration`, `degraded-insufficient-peers`,
  `failed`) with a `BootstrapPolicy` derived from the validated
  config;
- a synchronous `bootstrap_daemon` pipeline (cache revalidation,
  local RouterInfo construction, bounded offline reseed) before the
  supervisor starts; HTTPS reseed remains deferred;
- a `netdb-bootstrap` service in the supervisor graph alongside the
  `lifecycle` service; both are wired to the supervisor's
  cancellation primitive;
- a `netdb_seam::NetDbSeam` runtime-facing seam that exposes the
  Plan 105 lookup state machine vocabulary while reporting
  `ExploratoryPathStatus::BlockedExploratoryTunnelUnavailable`
  until Milestone 5 supplies exploratory tunnels;
- 24 integration tests under
  `crates/i2pr-daemon/tests/netdb_integration.rs` covering all 14
  Plan 106 integration items plus the typed-blocker paths.

The Plan 106 closure record is `plans/106-status.md`. Milestone 4A
status after Plan 106:

```text
routerinfo_validation             = implemented (Plan 103)
local_netdb                       = implemented (Plan 103)
persistent_routerinfo_cache       = implemented (Plan 104)
su3_reseed_verification           = implemented (Plan 104)
reseed_ingestion                  = implemented (Plan 104)
netdb_query_state_machine         = implemented (Plan 105)
routerinfo_publication_state      = implemented (Plan 105)
netdb_daemon_integration          = implemented (Plan 106)
live_routerinfo_lookup            = blocked-on-milestone5-exploratory-tunnels
live_publication_verification     = blocked-on-milestone5-and-qualified-transport
milestone4_full_exit              = pending-cross-milestone-checkpoint
normal_daemon_ntcp2               = disabled-and-unenableable
ntcp2                             = experimental-non-advertised
external_netdb_over_ntcp2         = blocked
```

## Plan 099 Milestone 3 interop exit, harness reduction, and router buildout (historical; retained NTCP2 result)

Plan 099 is the corrective and exit plan from the multi-job
CI/provenance expansion that consumed Plans 095–098. It corrects
the central Plan 099 implementation finding — the instrumented
i2pd transport libraries were never actually compiled from the
patched source tree — and it freezes interoperability
architecture growth. The build script now produces two separate
i2pd archive sets (`I2PD_INSTRUMENTED_LIB_DIR` and
`I2PD_PRISTINE_LIB_DIR`), and the pristine control driver uses
native `Transports::SendMessage`, `Transports::IsConnected`, and
`TransportSession::IsEstablished` instead of observer APIs the
control build cannot emit.

Plan 100 closed its exit-readiness defects (D1, D2, D3, D4) and
the Plan 099 single-job CI workflow was dispatched exactly once
from the Plan 100 correction commit. The bounded replacement
runs (allowed by Plan 100 Result branch C) consumed two narrow
direct corrections before the bound forward-instrumented attempt
reached authentic post-TCP protocol evidence. The final
development result is `protocol-defect-localized`:

```text
plan_099 = closed-protocol-defect-localized
plan_100 = closed-exit-cleanup-with-recorded-procedural-deviation
plan_101 = active-daemon-ntcp2-activation-safety-correction
plan_095 = historical-superseded-by-plan099-single-job-lane
plan_087 = historical-development-sequence
plan_088 = historical-development-sequence
plan_079 = deferred-to-pre-normal-ntcp2-activation-and-public-network-checkpoint
plan_072 = inactive-pending-plan088-ambiguity
ntcp2    = experimental-non-advertised
normal_daemon_ntcp2 = disabled-after-plan101
router_construction = active
development_interop = protocol-defect-localized
exact_wire_stage = noise_authenticated
external_netdb_over_ntcp2 = blocked
```

The compact sanitized summary is preserved at
`target/interop/evidence/milestone-3/31521642090/plan099-summary.json`.
Plan 101 closed the daemon-safety correction that removes premature
NTCP2 production activation and restores the correct activation
boundary. The Milestone 4 child sequence (Plan 103 → Plan 104 →
Plan 105 → Plan 106) closed locally; the next substantial product
roadmap is governed by
[Plan 102](plans/102-milestone-4-routerinfo-netdb-authority-and-roadmap.md)
and its child sequence (Plans 103 → 104 → 105 → 106), not another
NTCP2 evidence framework plan.

The active development interop lane is bounded to:

- one fresh GitHub Actions `ubuntu-24.04` workflow
  (`.github/workflows/ntcp2-interop-host-loopback-development.yml`)
  with one `development-interop` job;
- one pinned i2pd 2.60.0 source tree at
  `f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e`;
- one canonical absolute `$BUILD_OUTPUT` artifact path used by
  every producer, verifier, manifest, and live consumer;
- one canonical absolute `$PLAN099_SANITIZED` evidence path that
  survives the destructive cleanup;
- four primary attempts: forward-instrumented, forward-control,
  reverse-instrumented, reverse-control, gated sequentially so
  the next attempt only runs after the previous one passes;
- one compact JSON summary that binds source commit, reference
  revision, topology kind, development-only flag, i2pr binary
  digest, both i2pd binary digests, all four per-attempt results,
  cleanup disposition, and the sanitized `summary_sha256`.

The active functional surface is small:

- `scripts/interop/run-minimal-i2pd-host-loopback-probe.py` —
  the only allowed entry point to live subprocess execution;
- `tests/integration/ntcp2/harness/plan083_runner.py` and
  `plan084_runner.py` — the canonical forward/reverse runners;
- `tests/integration/ntcp2/harness/preflight_runner.py` —
  the listener-only preflight;
- `tests/integration/ntcp2/harness/i2pd_direct_driver.py`,
  `minimal_i2pd_probe.py`, `minimal_i2pd_reverse_probe.py`,
  `interop_topology.py`, `reference_event.py`,
  `reference_trigger_v4.py`, `execution_lane.py`,
  `plan099_exit_gate.py` — the functional interop modules;
- `tests/integration/ntcp2/harness/test_minimal_i2pd_probe.py`,
  `test_i2pd_direct_driver.py`, `test_i2pd_direct_control.py`,
  `test_execution_lane.py` — the bounded functional tests;
- `tests/integration/ntcp2/reference-drivers/i2pd/build-driver.sh`
  and `CMakeLists.txt` — the i2pd driver build script and the
  split-library link contract;
- `scripts/check-ntcp2-interoperability.sh` — the trimmed
  static boundary check.

The Plan 099/Plan 100 development exit gate vocabulary is exactly
three values:

```text
passed
protocol-defect-localized
environment-or-harness-blocked
```

- `passed` — all four per-attempt records carry
  `terminal_result = passed` and `cleanup_result = clean`.
- `protocol-defect-localized` — at least one executed primary
  direction reached `tcp_connected` (or any later wire stage) and
  then failed before the required correlated DeliveryStatus pass.
  A skipped downstream attempt cannot erase that classification.
- `environment-or-harness-blocked` — the earliest nonpassing path
  is pre-TCP preparation, build, reference startup, or
  workflow/API failure.

The gate is implemented in
`tests/integration/ntcp2/harness/plan099_exit_gate.py` and is
covered by the focused `Plan099ExitGateTests` class in
`test_minimal_i2pd_probe.py`.

Plan 099/Plan 100 forbid adding new `test_planNNN.py` files, new
plan-number-specific Python runners, or new plan-token static
checks. Historical plan documents remain in `plans/` as audit
records but are not executable API contracts.

The active sequence forbids repairing or retrying the Plan 046
rootless lane or the Plan 048/049/050 Multipass recovery lane.
Multipass, Docker, public-network traffic, reseed, SAM, I2CP,
SSU2, and Java I2P are not part of the development interop
surface. A localized NTCP2 defect keeps NTCP2 disabled and
non-advertised but does not block production daemon composition,
RouterInfo publication architecture, NetDB storage/indexing, SU3
reseed parsing, or deterministic local state-machine tests.

Required focused checks for the active sequence:

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-ntcp2-vectors.sh
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_i2pd_direct_driver.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_minimal_i2pd_probe.py'
git diff --check
```

The focused local seam is sufficient for routine development.
The full historical plan-specific Python matrix, rootless checker,
Multipass checker, and release-certificate validator are not
required for Plan 100 closure; they remain available via git
history for forensic archaeology.
