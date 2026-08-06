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

## Current active sequence amendment (2026-08-05, Plan 092 / plan092)

The active authority is [Plan 085](plans/085-milestone-3-host-loopback-development-execution-roadmap.md),
followed by Plan 086 (status correction + `host-loopback-development`
lane enablement, **closed on this host**), Plan 087 (first real
`i2pr -> i2pd` forward probe, implementation surface delivered
and instrumented attempt recorded), Plan 090 (i2pd RouterInfo
corrective pass: four behavior-neutral driver corrections, **corrections
landed on this host**), Plan 091 (forward NTCP2 Noise-handshake corrective
pass: i2pd direct driver preconditions landed — `SetNetID`, explicit logger
start/stop, `tcp_accepted` wait, symmetric `DeliveryStatus` send, **corrections
landed on this host, forward direction still blocked**), Plan 092
(forward-handshake evidence integrity and ownership closure:
privacy-safe handshake stage observation schema, terminal-counter
preservation, current-run event dedup, status authority rewrite,
dedicated regression matrix, **active single next executable plan**),
and Plan 088 (reverse `i2pd -> i2pr` probe and development decision),
and the [Plan 072/079 gate amendment](plans/072-079-gate-plan-088.md)
that moves the Plan 072 and Plan 079 gates from Plan 084 to Plan 088.
Plan 082 is implemented and closed; Plans 083 and 084 are implementation
complete but execution pending; Plan 086 added the
`host-loopback-development` topology kind, the
`HostLoopbackDevelopmentPlacement`, the bounded literal IPv4 loopback
acceptance, the thin wrapper
`scripts/interop/run-minimal-i2pd-host-loopback-probe.py`, and the
listener-only preflight. The Plan 084 historical `lane-invalidated`
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
see `plans/092-status.md` for the ownership selection
(Branch A — i2pr runtime / state-machine defect, with Branch D
reserved as the secondary owner) and the planned narrow
correction surface. Plan 088 remains blocked on a follow-up
execution pass under Plan 092.

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
this host and the forward direction did not pass. The Plan 088 active
development decision is `insufficient-evidence` because the Plan 091
forward direction recorded a TCP-established protocol failure with
the i2pd NTCP2 transport reading `End of file` on the first handshake
body read while the i2pr reports a data-phase
`receiver_delivery_status_missing` terminal. Plan 079 remains
blocked until `plans/088-status.md` records `decision =
two-way-development-probe-passed`. Plan 072 remains inactive until
`plans/088-status.md` records `decision =
ambiguous-reference-divergence` with one exact role/stage diagnostic
question. NTCP2 stays experimental and non-advertised. See
`plans/082-status.md`, `plans/083-status.md`, `plans/084-status.md`,
`plans/086-status.md`, `plans/087-status.md`, `plans/088-status.md`, and
`plans/091-status.md`.

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
