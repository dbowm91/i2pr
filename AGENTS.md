# Repository Guidelines

`i2pr` is an experimental Rust I2P router. It is not production-ready and must
not be used for anonymity or security-sensitive workloads. Read `README.md`,
`GUARDRAILS.md`, `CONTRIBUTING.md`, the applicable `plans/` document, and
relevant `docs/adr/` records before changing code.

## Workspace Layout

Nine-crate workspace under `crates/`:

- `i2pr-proto` — bounded wire codecs (crate-root façade, borrowed cursors, strict decoding, typed errors)
- `i2pr-crypto` — Ed25519/X25519/AES/ChaCha20-Poly1305/HMAC/SipHash wrappers
- `i2pr-storage` — versioned persistence; identity and NTCP2 static-key records
- `i2pr-core` — runtime-neutral service contracts
- `i2pr-transport` — runtime-neutral link/delivery contracts (no Tokio, no I/O)
- `i2pr-transport-ntcp2` — runtime-neutral NTCP2 handshake + data frames
- `i2pr-runtime` — **only** production owner of Tokio tasks, sockets, timers, channels, wakeable cancellation
- `i2pr-daemon` — composition root + CLI
- `i2pr-testkit` — deterministic simulation; production crates must not depend on it

Fixtures: `tests/fixtures/i2np/` (manifest at `tests/fixtures/i2np/manifest.tsv`),
`tests/fixtures/ntcp2/crypto/` (manifest at `…/manifest.tsv`). Opt-in nightly
fuzz workspace at `fuzz/`.

## Hard Boundaries (enforced by scripts)

These are checked on CI and will reject the change:

- Dependency direction (`scripts/check-dependency-direction.sh`):
  `i2pr-proto <- i2pr-crypto <- i2pr-storage`; `i2pr-core <- i2pr-transport
  <- i2pr-runtime <- i2pr-daemon`; `i2pr-transport-ntcp2` consumes
  `i2pr-crypto`/`i2pr-proto`/`i2pr-transport`, and `i2pr-runtime` may compose
  `i2pr-transport-ntcp2` for Plan 042. **No production crate may
  depend on `i2pr-testkit`.**
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

Toolchain is pinned to Rust 1.95.0 (`rust-toolchain.toml`); MSRV is 1.85
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

Required focused checks are:

```text
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
