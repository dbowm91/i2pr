# Plan 040/041/043/044 interoperability apparatus

The Ubuntu reference-router harness is preparation infrastructure, not a
runtime plane and not an interoperability claim. Preparation runs on the
supported Ubuntu 24.04 amd64 host and may fetch only the lock-listed source,
IzPack artifact, and declared packages. Execution is offline and runs each
reference in disposable namespaces connected by one veth pair. There is no
default route, DNS, forwarding path, or public egress.

## Canonical build contract

The machine identifiers are `java_i2p` and `i2pd`. Java I2P 2.12.0 is pinned
to `2800040deee9bb376567b671ef2e9c34cf3e30b6`; i2pd 2.60.0 is pinned to
`f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e`. Cache keys hash the canonical
reference, full source object ID, lock digest, `ubuntu-24.04-amd64` host
contract, and reviewed build-command version. `current-cache.json` is the
only cache lookup index; recursive metadata guessing is forbidden.

Each cache contains strict schema-2 metadata. The parser rejects duplicate or
unknown keys, abbreviated revisions, invalid SHA-256 values, mismatched
references/locks, and launcher or artifact paths escaping the cache root. The
installed runtime tree is re-hashed before every execution. `--offline`
cannot fetch a missing source or dependency and fails before a builder can
perform network I/O.

## Topology and firewall

Namespace names retain the run description, but veth names use an eight-hex
token derived from the run ID and synthetic network ID. Generated names are
at most 15 bytes. The topology verifier requires exactly `lo` and `peer0`,
the expected `.1`/`.2` addresses, directly connected `/30` and optional `/64`
routes, no defaults, no public route probes, disabled namespace forwarding,
no host endpoint, no router process, and the expected nftables digest.

Each namespace has its own exact policy. Loopback and established traffic are
allowed; new TCP output is limited to the peer address and peer listening
port; new input is limited to the peer source address and local destination
port. IPv6 uses the same protocol/port constraints. A disposable canary
proves the allowed peer port, rejects a second peer port, and rejects a public
route before a router starts.

Plan 041 does not reuse the i2pr/reference topology owner for its control run.
`harness/reference_topology.py` creates `java-<short-run-id>` and
`i2pd-<short-run-id>` namespaces, assigns `192.0.2.1/30` and `192.0.2.2/30`,
and installs a one-way new-TCP policy selected by the scenario. The reverse
direction is a separate run; source-port observations never decide who
initiated a session.

The private network-ID contract is explicit and checked after rendering:
Java I2P uses `router.networkID=99` and i2pd uses `netid = 99`. The names are
source-traced in the adjacent configuration READMEs to the locked Java
`Router.java` and i2pd `libi2pd/Config.cpp` revisions. A missing or public value
rejects the run before either router starts.

## Runtime layout and evidence

The Java adapter stages the read-only cache under `reference-runtime`, keeps
configuration under `config`, and writes router data under `reference-data`.
The i2pd adapter uses its pinned binary/data-file cache and the same disposable
data/config roots. Both adapters derive the `routerInfo-<identity-hash>.dat`
NetDB filename from the bounded RouterInfo identity instead of trusting an
arbitrary source filename.

Child handles are retained for normal stop/join and atomically recorded PID
files support emergency recovery. `cleanup.sh` additionally enumerates
namespace PIDs, terminates then force-kills within a bound, removes namespaces
and host veths, deletes run roots, and returns nonzero for any residual state.

Secret-bearing state lives only under `target/interop/runs/<run-id>/`.
Sanitized records are atomically finalized under `target/interop/evidence/`
after processes and namespaces are gone. A passed record contains the actual
clean/dirty i2pr commit disposition, full reference revision, artifact/tree,
configuration and topology hashes, counters, and cleanup result. Cleanup
failure changes a protocol pass to `failed_cleanup`; it never leaves a secret
run root behind.

Plan 041 schema-2 records additionally carry both reference revisions and
artifact/tree/configuration hashes, the direction policy, typed RouterInfo
validation results, dual authenticated-link observations, connection/process
counters, and the evidence digest. The reference control is not a support
claim; i2pr mixed-router evidence still requires the authorized Plan 042
launcher-to-reference execution.

## Plan 042 launcher boundary

The Plan 042 launcher is now a bounded runtime composition seam, not a
placeholder readiness process. It validates the strict confined scenario,
prepares disposable permission-hardened identity, NTCP2 static-key/IV, and
RouterInfo state, then invokes the runtime listener/dial, handshake executor,
authenticated-link promotion, and DeliveryStatus exchange. Its JSONL status
records keep listener readiness separate from terminal authentication/data
results and use fixed reason codes only.

This local launcher path is still not reference evidence. The reference runner
must complete the Ubuntu namespace, cache, RouterInfo import, and observation
gates before any mixed-router result can be retained. The normal daemon remains
disabled and all NTCP2 support rows remain experimental/non-advertised.

## Plan 043 build-system gate contract

Plan 043 adds a fail-closed build-system promotion boundary around this
apparatus. The required ordered gates are:

```text
contract
-> reference-build
-> reference-offline-reuse
-> environment-smoke
-> reference-crosscheck-ipv4
-> i2pr-handshake-smoke-ipv4
-> full-matrix
-> evidence-validation
-> cleanup-verification
```

The contract gate runs without starting routers and covers the locked Rust
build, tests, documentation, dependency/runtime boundary checks, NTCP2
manifest/evidence checks, and Python harness unit tests. The reference-build
gate is the only network-enabled build phase. It uses the exact lock-listed
packages and sources, records Ubuntu/tool metadata, runs available reference
tests, and emits a canonical summary with source, artifact, and complete
installed-tree hashes.

The supported host is exactly Ubuntu 24.04 amd64/x86_64 with Bash 4+, a UTF-8
locale, non-interactive `sudo` when not root, Linux user/network namespaces,
nftables, at least 4 GiB free under `target/`, and the commands checked by the
host preflight. The declared setup package set is:

```text
ca-certificates curl git wget xz-utils unzip zip coreutils findutils procps
util-linux iproute2 nftables openssl python3 python3-venv
openjdk-17-jdk-headless ant gettext
build-essential cmake pkg-config libboost-all-dev libssl-dev zlib1g-dev
```

The pinned references are Java I2P 2.12.0 at
`2800040deee9bb376567b671ef2e9c34cf3e30b6` and i2pd 2.60.0 at
`f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e`. The IzPack 5.2.4 download is
accepted only with the SHA-256 in `references.lock.toml`. Rust uses the
repository-pinned 1.95.0 toolchain and locked Cargo builds. Host metadata
records the Ubuntu release, kernel, architecture, Java, Ant, compiler, CMake,
Python, and iproute2/nftables versions; the aggregate manifest records
workflow run and attempt as non-secret metadata.

The offline-reuse gate restores only a verified cache, runs
`build-references.sh --offline`, and re-hashes the complete runtime tree. It
must not clone, fetch, download, install packages, resolve DNS, or silently
fall back to another cache. Cache identity includes the canonical reference,
full source revision, lock digest, `ubuntu-24.04-amd64` host contract,
build-command version, and relevant tool/ABI versions. Identities, keys,
RouterInfo, NetDB state, rendered runtime configuration, run roots, raw logs,
namespace state, and evidence records are never cache inputs.

After offline reuse, the environment-smoke and reference-control gates run
before any i2pr gate. Environment smoke proves reference startup, disposable
state production, and bounded cleanup only. `reference-crosscheck-ipv4` runs
the separate `reference-java-i2pd-ipv4` and `reference-i2pd-java-ipv4`
scenarios with private network ID 99, staged strict RouterInfo validation and
import, controlled directions, dual authenticated observations, and clean
shutdown. It is a harness control, not i2pr evidence. The i2pr gate requires
four independent directions (i2pr↔Java I2P and i2pr↔i2pd), authenticated
handshake and bounded DeliveryStatus exchange; one passing direction cannot
mask another failure. The full matrix adds the bounded adversarial and
resource cases, not unbounded fuzzing.

Evidence validation consumes an aggregate run manifest. It rejects missing or
unexpected passed records, placeholders, digest mismatches, incomplete
direction coverage, forbidden content, and non-clean cleanup. Only sanitized
JSON records, the sanitized reference-build summary, and the aggregate
manifest belong in an upload allowlist. `cleanup.sh` must run with an
always-run policy after privileged phases and at the end. Plan 043 requires a
separate `verify-clean-host.sh` check for residual prefixed namespaces/veths,
reference or launcher processes, secret-bearing run roots, forbidden retained
files, and attributable nftables/routes/forwarding changes. The workflow now
exposes the ordered manual lane and its verifier helper, but no completed
successful aggregate run is present; this is a required contract, not a
passing result.

The clean-host verifier records a sanitized baseline before privileged
execution:

```text
sudo -E bash scripts/interop/verify-clean-host.sh --record-baseline
```

After cleanup, it compares the host state and retained tree against that
baseline:

```text
sudo -E bash scripts/interop/verify-clean-host.sh --verify --baseline target/interop/build/clean-host-baseline.json
```

The baseline and verification marker remain under ignored `target/interop`
state and are not evidence uploads.

Promotion is manual first, then low-frequency scheduled control after repeated
clean-checkout and cache-reuse runs, then a current successful run at
Milestone 3 closure. Any trusted pull-request lane requires a separate later
decision and must not expose privileged execution to forked or untrusted code.

## Plan 044 mixed-router composition

Plan 044 converts the component implementations from Plans 040–043 into one
executable, reproducible, fail-closed path. It adds four directional
i2pr/reference mixed-scenario definitions under
`tests/integration/ntcp2/mixed-scenarios/`:

- `i2pr-to-java-ipv4` (i2pr initiates, Java I2P responds)
- `java-to-i2pr-ipv4` (Java I2P initiates, i2pr responds)
- `i2pr-to-i2pd-ipv4` (i2pr initiates, i2pd responds)
- `i2pd-to-i2pr-ipv4` (i2pd initiates, i2pr responds)

Each direction has a unique execution ID, one declared initiator and responder,
one terminal typed result, and one evidence record. No direction may mask
another.

The mixed runner composes `I2prAdapter` with each reference adapter through a
strict launcher scenario renderer. The renderer populates the exact launcher
schema and rejects absolute paths, parent traversal, endpoints outside
synthetic namespace ranges, mismatched address families, missing peer data for
initiators, peer data for responders, and unknown fields.

The data-phase oracle does not rely on an echo assumption. It uses a
protocol-valid trigger supported by both pinned references. Evidence records
carry real counters for authenticated-link count, frames sent/received, I2NP
message aggregates, admission/replay counters, process lifecycle counters,
and cleanup disposition.

Gate archival uses gate-specific staging to prevent cross-gate record
relabeling. The aggregate manifest must include exactly the expected records
for the selected profile; missing, extra, mislabeled, or zero-valued records
fail the gate.

The current checkout contains the mixed-scenario definitions, the mixed-runner
composition, the strict launcher renderer, and the non-echo data-phase oracle.
No completed mixed-router i2pr record is present; these are explicit blockers,
not skipped successes. NTCP2 remains experimental and non-advertised.

## Plan 046 rootless sealed-namespace evidence lane

Plan 046 replaces the host-global namespace requirement for the primary NTCP2
interoperability evidence path with a **rootless, process-scoped user/
network/mount/PID sandbox** that an ordinary user can run without sudo,
passwordless elevation, host capabilities, setuid helpers, host-visible
namespaces, host veth creation, or host firewall mutation. The primary
evidence topology is `rootless-sealed-single-netns` with privilege model
`unprivileged-userns`. The legacy `privileged-dual-netns-veth` topology is
renamed and kept for explicit later qualification only.

The sandbox contains only `lo`. Both routers bind distinct synthetic RFC 5737
addresses (`192.0.2.1/32` and `192.0.2.2/32`) and an optional synthetic IPv6
pair (`2001:db8:36::1/128` and `2001:db8:36::2/128`). The structural
isolation basis is the freshly created network namespace plus the
single-ID UID/GID maps, `no_new_privs`, `setgroups deny`, and the
absence of default or external routes. Namespace-local nftables are not
required.

The lane defends against accidental public-network contact, an adapter
binding wildcard, an adapter attempting DNS or external connect, a stale
host-global namespace, a sandbox process surviving the supervisor, a
broader-than-one UID/GID map, a passing record generated outside the
sandbox, a successful rootless probe that lacked a usable namespace, and
any evidence that retains raw namespaces, UIDs, paths, endpoints, logs,
RouterInfo, or I2NP contents.

The topology backend contract (`tests/integration/ntcp2/harness/interop_topology.py`)
defines `InteropTopology` and the `ProcessPlacement` value object. Adapters
and runners select the topology through `select_topology("rootless-sealed-single-netns", ...)`
and never inspect effective UID or construct `sudo` / `ip netns` prefixes.

The outer entrypoint (`scripts/interop/rootless-enter.sh`) creates the
sandbox and execs the inner supervisor. It accepts only a strictly
allowlisted set of operations, has no shell `eval`, and never falls back
to the privileged backend. The inner supervisor
(`tests/integration/ntcp2/harness/rootless_supervisor.py`) verifies the
sandbox via:

- single-ID UID/GID maps;
- `setgroups` denial;
- `no_new_privs`;
- distinct user, network, mount, and PID namespaces;
- `lo` readiness;
- exact synthetic bind and connect behavior;
- the absence of any default or external route;
- a bounded external connect probe.

On success it writes a sanitized `IsolationAttestation` record whose
sha256 is bound to every mixed-router evidence record, and whose
parent-network state pre/post digests must be byte-equal for the run
to be considered passing.

A static rootless boundary checker (`scripts/check-rootless-interop-boundary.sh`)
fails the change whenever rootless-owned files contain prohibited
patterns or omit required contracts. The mixed-router evidence schema
 adds `topology_kind`, `privilege_model`, `sandbox_attestation_sha256`,
and `parent_network_state_unchanged`. A passed record that violates any
of these is rejected. The status file `plans/046-status.md` tracks the
stages of implementation completion and external evidence completion;
the closure record is `plans/046-closure.md`. Plan 046 closed with the
canonical typed blocker `blocked_unprivileged_user_namespace` recorded
on this host, and `plans/047-cross-host-rootless-lane-expansion.md`
takes on cross-host recovery.

## Plan 048/049/050 Multipass recovery environment

The host-level Plan 046 AppArmor restriction remains unchanged as the negative
baseline. Plan 048 adds a disposable Multipass Ubuntu 24.04 amd64 guest for
the `host.apparmor-restrict-off` recovery category. Plan 049 corrects its
lifecycle ownership model. Plan 050 minimizes the cloud-init unit (no
`rustup` or host toolchain inside the guest), adds a sanitized cloud-init
failure taxonomy, a `--guest-probe-only` flow, and a selective-purge
remediation that requires a verified ownership contract. The reviewed
environment contract is identified by a stable environment ID, while each
execution has a separate run ID and each realization has a generation-bound
concrete instance name. The legacy `i2pr-interop-rootless` name is not
authoritative.

The host reserves a versioned lifecycle record atomically before launch under
`target/interop/multipass/state/<run-id>/lifecycle.json`. A per-run/
per-instance lock serializes transitions through explicit states such as
`reserved`, `launching`, `provisioned`, `source_and_cache_ready`, `probe_passed`,
`offline_ready`, `running`, `exported`, `blocked`, and `destroyed`. Structured
Multipass state is normalized; unknown and deleted-but-unpurged states fail
closed. A generated name collision causes bounded reallocation, never mutation
of the colliding resource.

Each managed guest carries a root-owned environment contract and ownership
token. Ownership is proven by matching host and guest records, token hash,
environment/cloud-init/source/cache digests, generation, policy, execution
user, mounts, snapshots, and process state. A name match alone is insufficient.
`--inspect` is read-only. `--adopt-owned`, `--resume-owned`,
`--recreate-owned`, and `--destroy-owned` are explicit and require proof;
normal execution never silently adopts, recreates, stops, deletes, or purges an
existing instance. Global `multipass purge` is not a lifecycle operation.

Cloud-init and source/cache preparation may use the network;
`prepare-offline.sh` installs a guest-only nftables egress-deny policy before
`run-matrix.sh`. The host baseline probe is recorded independently and does not
gate guest launch. After ownership/policy verification and immediately before
router start, `probe.sh` must obtain `rootless_sandbox_available` and a
non-zero validated `IsolationAttestation`. The matrix runs the four Plan 045
directions in fixed order and requires the existing topology, privilege,
attestation, cleanup, and parent-network predicates.

The canonical cache is `target/interop/cache`, matching `build-references.sh`,
`offline-reuse.sh`, and the Python cache resolver. The older Plan 047 example
`target/interop/build/cache` is not an executable path. Host mounts are not
authoritative inputs. Snapshots are allowlisted and bound to the instance
generation and environment/source/cache contract. `export-evidence.sh`
transfers only the sanitized bundle, independently hashes it, validates the
guest manifest, and atomically places it under
`target/interop/evidence/multipass/<run-id>/`. Every directional record refers
to the same environment evidence hash; mixed runs or generations are rejected.
Pre-router failures produce sanitized environment-blocker records and never
become protocol evidence. Destroying an owned VM preserves the host evidence
directory. A typed blocker or reference-only result never advances the support
ledger or closes Milestone 3.

## Plan 054 Java startup and reference-observation qualification

Plan 054 closes the two Plan 052 evidence gates that depend on a live
Java reference and a per-side observation marker. It adds three local
artifacts and three new constraints:

- The Java startup matrix driver
  (`tests/integration/ntcp2/harness/java_matrix.py`) composes
  `java_startup_probe.py` once per cell of the 16-cell matrix
  (namespace × data-state × launcher × sequence) with three
  independent attempts each. The new `seeded-clone` data state
  copies a frozen template into a fresh per-attempt directory and
  refuses to launch the template directly (`template-launch-forbidden`).
- The frozen Java template lifecycle is anchored by
  `scripts/interop/java-prepare-template.py`. The preparation phase
  is the only path that may download, install, or seed Java state.
  The execution phase is restricted to `seeded-clone` clones; the
  template digest is verified unchanged before and after every
  qualification start.
- The machine-readable reference observation catalog
  (`tests/integration/ntcp2/reference-observation-catalog.toml`)
  binds every marker to its exact source path, symbol, marker text,
  sanitization rule, and minimum count. The Markdown
  (`reference-observation-catalog.md`) is now drift-checked,
  explanatory documentation; the static
  `check-ntcp2-interoperability.sh` checker rejects any
  `PENDING-SOURCE-INSPECTION` entry and any hardcoded rejection in
  the Plan 052 predicate.

The Java and i2pd adapters expose
`collect_observation(role, run_id, correlation, log_cursor, catalog)`
and return finalized `i2pr-ntcp2-direction-observation-v2` records.
`mixed_runner._evaluate_plan052_predicate` now applies the
`receiver_passes_data_phase` predicate against those records. The
Plan 053 pipeline accepts the live records through
`write_direction_artifacts(..., i2pr_observation=...,
reference_observation=...)`; the synthetic builder remains the typed
fallback for blocked and rejected directions.

External qualification (the complete 48-start matrix, the ten
consecutive rootless starts, and the seven control experiments) still
requires the pinned Java 2.12.0 and i2pd 2.60.0 references on an
authorized Ubuntu 24.04 amd64 host or Multipass guest. The current
host is the Plan 046 negative baseline and cannot exercise the matrix;
the Plan 048/049 Multipass recovery lane is the canonical external
path. Plan 054 does not close Milestone 3.

## Plan 058 record and candidate integrity closure pass

Plan 058 is a documentation, provenance, and execution-contract closure
pass. It retires the Plan 056 candidate, supersedes the Plan 057
follow-up plan, decides ADR 0021 (Rejected), and splits the previous
Plan 057 responsibilities into Plan 059 (reference-side implementation
and live qualification) and Plan 060 (fresh candidate + two-run
certificate). Plan 058 does not implement the i2pd direct helper, the
Java support topology, or external mixed-router execution.

- The candidate record integrity validator
  (`tests/integration/ntcp2/harness/candidate_record.py`, schema
  `i2pr-interop-candidate-v1`) refuses records with multiple
  authoritative SHAs, retired candidates consumed by execution
  tooling, candidates frozen before the implementation floor, and
  `committed` evidence claims that name ignored diagnostics.
- The Plan 058 test matrix (`test_plan058.py`) covers the positive
  and 14 negative fixtures, the on-disk
  candidate/ADR/Plan 057 supersession markers, the locked field
  set, and the two-lane contract.
- The static boundary checker
  (`scripts/check-ntcp2-interoperability.sh`) enforces the
  candidate record integrity invariants, the supersession markers,
  and the ADR decision marker.
- The Plan 058 closure defines two alternative execution lanes for
  any future Milestone 3 evidence run: Lane A (direct-host, requires
  `rootless_sandbox_available` on the execution host) and Lane B
  (guest, the outer host may continue to report
  `blocked_unprivileged_user_namespace` but the Multipass recovery
  guest must report `rootless_sandbox_available`). Exactly one lane
  is selected per candidate; a certificate may not combine Run A from
  one lane with Run B from another.
- The Plan 056 candidate is marked
  `retired; never used for an authoritative external run`. The
  historical SHA `fbf2cdb9ec12d35c7b7422c412e09d6db2d2d0cf` is
  preserved verbatim as an audit record. The Plan 056 closure
  record describes the locally generated diagnostic bundles under
  the ignored `target/interop/evidence/plan056/` directory and names
  the bounded local-diagnostic receipt at
  `tests/integration/ntcp2/evidence-receipts/plan056-local-diagnostic.json`
  with `artifact_storage = local-untracked`. Plan 057 is superseded
  before execution.
- ADR 0021 (`docs/adr/0021-minimal-java-support-topology.md`) is
  Rejected. The repository does not implement the Java support
  topology; the `java-to-i2pr-ipv4` direction remains a typed
  blocker for the pinned Java I2P 2.12.0 revision; Plan 059 must
  close with the typed blocker
  `blocked_java_support_topology_rejected`; Plan 060 must not start
  under the current four-direction contract.

The Plan 058 plan-of-record is
`plans/058-plan056-record-and-candidate-integrity-closure-pass.md`;
the closure record is `plans/058-status.md`.

## Plan 059 reference-side implementation and live qualification closure pass

Plan 059 implements the i2pd direct helper, the per-reference
observation qualification receipts, and the canonical pipeline
live-mode wiring that Plans 055-057 deferred. Plan 058 rejected
ADR 0021, so Plan 059 closes with the typed blocker
`blocked_java_support_topology_rejected`; the Java support topology
is forbidden under the current four-direction contract.

- The i2pd direct helper source, build contract, and source-lock
  record are committed under
  `tests/integration/ntcp2/reference-drivers/i2pd_direct_connect/`.
  The C++ helper (`i2pd_direct_connect.cpp`) links against the
  pinned i2pd 2.60.0 libraries and exercises the documented
  `i2pd::transports::Transports::SendMessage` call graph recorded
  in `tests/integration/ntcp2/reference-trigger-contracts.md`. The
  Python bounded driver (`i2pd_direct_connect.py`) provides the
  local qualification seam when the C++ helper cannot be built.
  `source-lock.json` records the pinned revision, the helper build
  inputs, and the locked constraints required by Plan 055 B2.
- The per-reference observation qualification receipts
  (`i2pd-2.60.0.json` and `java_i2p-2.12.0.json`) record the
  catalog metadata, the runtime-control blocker, and the typed
  absence per semantic level. The summary at `summary.json`
  tracks the overall qualification status. Both receipts mark every
  semantic level as `qualified = false` until the Plan 046
  rootless sealed-namespace lane or the Plan 048/049 Multipass
  recovery lane exercises the runtime controls.
- The Plan 052 pipeline now exposes a `live_mode` flag; in live
  mode a passed reference-initiated direction requires a real
  trigger record and live sender/receiver observation-v2 records.
  Helper, source, catalog, and qualification-receipt digests bind
  into the direction record so drift fails the bundle cross-check.
  Cleanup failure overrides pass. The synthetic fallback remains
  available for blocked/diagnostic fixture runs only.
- The Plan 059 test matrix
  (`tests/integration/ntcp2/harness/test_plan059.py`) covers 36
  cases across the five required surfaces: i2pd helper, Java
  support-topology gate, receiver observations, Java startup gate,
  and pipeline live mode.
- The Plan 059 closure contract is the typed blocker
  `blocked_java_support_topology_rejected` because ADR 0021 is
  Rejected. The Java receiver observations and the Java support
  topology remain blocked; the runtime qualification requires the
  Plan 046 rootless sealed-namespace lane or the Plan 048/049
  Multipass recovery lane.

The Plan 059 plan-of-record is
`plans/059-reference-side-implementation-and-live-qualification-closure-pass.md`;
the closure record is `plans/059-status.md`.

## Plan 060 fresh-candidate and two-run Milestone 3 certificate closure pass

Plan 060 is the execution-only pass that cuts one fresh candidate
from the fully implemented and qualified repository, selects
exactly one execution lane (direct-host or guest), runs the four
primary IPv4 mixed-router directions twice on independent mutable
state, and produces a verified Milestone 3 certificate over the
two sanitized bundles.

On this host the plan closes with the typed blocker
`blocked_execution_lane_unavailable`. The Plan 046 rootless
sealed-namespace probe returns `blocked_unprivileged_user_namespace`
(the host's kernel activates
`kernel.apparmor_restrict_unprivileged_userns=1`, which confines
every unprivileged user namespace to a restrictive AppArmor policy
and prevents `unshare -U -r --map-root-user` from writing
`/proc/self/uid_map`). The Plan 048/049 Multipass recovery lane is
the canonical external path but cannot complete on this
constrained host (per Plan 051; the bridge is wired end-to-end but
the host's 15 GiB physical RAM plus four reserved qemu guests
repeatedly destabilize the guest SSH endpoint mid-dispatch, and the
host's multipassd became unresponsive mid-investigation).
ADR 0021 was Rejected by Plan 058; the Java support topology is
forbidden under the current four-direction contract; the
`java-to-i2pr-ipv4` direction remains a typed blocker for the
pinned Java I2P 2.12.0 revision.

The Plan 060 candidate is `declared-not-executable` on this host
(`plans/060-candidate.md`). The Plan 060 implementation surface is
mandatory regardless of close outcome:

- `tests/integration/ntcp2/harness/plan060.py` — Plan 060 helper
  module. Exports `plan060_typed_blocker() ->
  "blocked_execution_lane_unavailable"`, `plan060_close_status()
  -> "declared-not-executable"`, `execution_lane_lock(...)` for
  the Plan 058 two-lane contract, `candidate_record_digests()`
  for the bounded digest table, `freeze_readiness_report()` for
  the freeze-readiness checklist,
  `assert_plan060_freeze_invariants()` for the typed blocker
  enforcement, and `plan060_two_bundle_independence(...)` for the
  cross-run independence rules.
- `tests/integration/ntcp2/harness/test_plan060.py` — Plan 060
  test matrix (35 cases across the Plan 060 surface).
- `scripts/check-ntcp2-interoperability.sh` extended to enforce
  the Plan 060 artifacts, the Plan 060 test matrix coverage, and
  the candidate/closure marker invariants.
- `plans/060-candidate.md` and `plans/060-closure.md` — the
  candidate and closure records.

The Plan 060 plan-of-record is
`plans/060-fresh-candidate-and-two-run-milestone3-certificate-closure-pass.md`;
the candidate record is `plans/060-candidate.md`; the closure
record is `plans/060-closure.md`. The aggregate Milestone 3
closure (`plans/030-milestone-3-closure.md`) is amended by Plan 060
to record the close outcome. NTCP2 stays experimental and
non-advertised; Milestone 3 stays open until a future pinned Java
revision exposes a transport-only direct seam or the closure
contract is revised through a new ADR, and until either the Plan
046 rootless sealed-namespace lane or the Plan 048/049 Multipass
guest lane becomes runnable.

## Plan 062 NTCP2 evidence-contract and architecture correction

Plan 062 is the evidence-contract and architecture correction
pass that supersedes the Plan 060 execution authority. Plan 062
corrects the repository's mixed-router architecture and evidence
contract before new reference drivers are implemented.

Plan 062 lands:

- `docs/adr/0022-direct-reference-router-ntcp2-interop-drivers.md`
  (Accepted) — two-process direct transport drivers for Java I2P
  and i2pd. ADR 0022 replaces the rejected Java-support-topology
  premise (ADR 0021, Rejected by Plan 058) without rewriting
  ADR 0021. The primary topology is one reference router plus
  one i2pr process inside a rootless sealed network namespace or
  an equivalently isolated guest; there is no support router,
  floodfill, reseed, SAM, I2CP, HTTP/I2PControl, or tunnel pool
  in the primary path.
- `tests/integration/ntcp2/reference-drivers/source-verification.md`
  — the source-locked API inspection record for the pinned Java
  I2P 2.12.0 revision
  (`2800040deee9bb376567b671ef2e9c34cf3e30b6`) and the pinned
  i2pd 2.60.0 revision
  (`f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e`).
- `tests/integration/ntcp2/harness/reference_trigger_v4.py` — the
  Plan 062 v4 trigger schema (`i2pr-reference-trigger-v4`). The
  schema uses 64-lowercase-hex Router Hash for both local and
  peer sides, mandates the per-run DeliveryStatus `message_id`
  in `1..=0xffffffff`, binds the helper, source, build manifest,
  observer patch, source inspection record, and run identity
  digests, and rejects v3 trigger records for new bundles. The
  historical v3 module remains the bounded historical-reader
  path.
- `tests/integration/ntcp2/harness/reference_event.py` — the
  Plan 062 reference-event v1 schema
  (`i2pr-reference-event-v1`). The schema records per-driver
  structured events (`process_started`, `listener_ready`,
  `router_info_exported`, `peer_router_info_validated`,
  `tcp_connected`, `ntcp2_authenticated`, `frame_emitted`,
  `frame_authenticated_and_decrypted`, `i2np_message_decoded`,
  `terminal_clean`, `terminal_rejected`) with strict
  per-process sequence ordering, exact DeliveryStatus message ID
  correlation for data-phase events, and continuous Router Hash
  binding.
- `tests/integration/ntcp2/harness/observation_v3.py` — the Plan
  062 v3 observation schema
  (`i2pr-ntcp2-direction-observation-v3`). The schema adds the
  mandatory correlation fields `delivery_status_message_id`,
  `peer_router_hash_sha256`, `local_router_hash_sha256`, and
  `source_event_sha256`. The receiver pass predicate requires
  nonzero decrypt and decode counts and rejects
  generic-phrase-only sources. The historical v2 module remains
  the bounded historical-reader path.

Plan 062 retires the Plan 060 candidate from all future
candidate validators and the static boundary checker. The
Plan 060 candidate record (`plans/060-candidate.md`) is preserved
verbatim for audit; the Plan 060 closure record
(`plans/060-closure.md`) carries the explicit "Superseded by
Plan 062" marker. The future candidate implementation floor is
Plan 065 closure or later. v3 trigger records and v2 observation
records remain readable for historical inspection but cannot
contribute to a new passing bundle; only Plan 062 v4 trigger
records, v3 observation records, and reference-event v1 records
may contribute.

Plan 062 extends
`scripts/check-ntcp2-interoperability.sh` to enforce the v4
trigger schema, the v3 observation schema, the reference-event v1
schema, the source-verification record, ADR 0022 (Accepted),
the Plan 060 retirement markers, and the absence of active 40-hex
SHA-1 Router Hash width in the active schemas. Plan 062 does not
implement the Java or i2pd drivers; those belong to Plan 063 and
Plan 064.

NTCP2 stays experimental and non-advertised; Milestone 3 stays
open until Plan 065 closes with one complete four-direction live
diagnostic bundle and Plan 066 produces a verified Milestone 3
certificate.

## Plan 063 Java I2P stripped-router direct NTCP2 driver

Plan 063 implements the source-locked Java I2P 2.12.0 stripped-router
direct NTCP2 driver. The driver is the test-only counterpart to the
Plan 064 i2pd driver; together they form the source-locked pair
required by the Plan 061 roadmap. The driver is **test-only** and
never becomes a production dependency of `i2pr-daemon`.

Plan 063 lands:

- `tests/integration/ntcp2/reference-drivers/java/src/JavaNtcp2InteropDriver.java`
  — the source-locked Java driver. It embeds the upstream
  `net.i2p.router.Router` and `RouterContext`, activates the
  pinned dummy facades (`DummyNetworkDatabaseFacade`,
  `DummyClientManagerFacade`, `DummyPeerManagerFacade`,
  `DummyTunnelManagerFacade`), uses the real
  `net.i2p.router.transport.ntcp.NTCPTransport` (no patch, no SSU2,
  no `VMCommSystem`), and submits a real correlated
  `DeliveryStatusMessage` through `OutNetMessagePool`. The driver
  exposes `inspect`, `listen`, and `dial` modes through a strict
  config contract. The receive handler is a `HandlerJobBuilder`
  registered for `DeliveryStatusMessage.MESSAGE_TYPE` (constant
  value 10); the data-phase events (`frame_emitted`,
  `frame_authenticated_and_decrypted`, `i2np_message_decoded`) are
  emitted from the receive handler invocation, never from a
  generic log phrase.
- `tests/integration/ntcp2/reference-drivers/java/source-lock.json`
  — the source-lock record
  (`i2pr-java-helper-source-lock-v1`) binding the pinned Java
  revision `2800040deee9bb376567b671ef2e9c34cf3e30b6`, the helper
  source path, the build contract, the locked constraints, and the
  required verification controls.
- `tests/integration/ntcp2/reference-drivers/java/classpath-manifest.json`
  — the runtime classpath binding every pinned jar in
  `target/interop/cache/java_i2p/<tree>/lib/` to its purpose. No
  Maven Central dependency may be introduced.
- `tests/integration/ntcp2/reference-drivers/java/build-manifest.schema.json`
  — the build-manifest schema
  (`i2pr-java-helper-build-manifest-v1`) that requires measured
  digests for the i2p.jar, router.jar, all runtime jars, the
  driver source, the driver binary, the classpath manifest, and the
  JDK versions. No zero or placeholder digest is allowed in an
  attempted run.
- `tests/integration/ntcp2/reference-drivers/java/build-driver.sh`
  and `run-driver.sh` — the offline build and runtime seams. The
  build script requires the exact pinned source/build cache, uses a
  deterministic sorted source list, uses an explicit classpath,
  emits no download, and writes only into an owned output
  directory.
- `tests/integration/ntcp2/harness/java_direct_driver.py` — the
  Python harness adapter that binds every helper invocation into a
  Plan 062 v4 trigger record (`i2pr-reference-trigger-v4`) and
  validates the Plan 063 strict driver config contract. The
  adapter never reaches inside the Java helper state and never
  synthesises a passing record.
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
  sealed-namespace lane or the Plan 048/049 Multipass recovery
  lane.

Plan 063 extends the Plan 062 source-verification record
(`tests/integration/ntcp2/reference-drivers/source-verification.md`)
with the canonical two-process topology contract. The Plan 062 v4
trigger schema, the Plan 062 reference-event v1 schema, and the
Plan 062 v3 observation schema remain the authoritative schemas for
the direction records.

Plan 063 extends `scripts/check-ntcp2-interoperability.sh` to
enforce the Java direct driver source, the source-lock record, the
classpath manifest, the build-manifest schema, the build/run
scripts, the Python adapter, the test matrix, and the qualification
receipt. The active v4 trigger, v3 observation, and reference-event
schemas continue to enforce the 64-hex SHA-256 Router Hash
contract; the historical v3 trigger and v2 observation paths remain
the bounded historical-reader path.

Plan 063 does not advance any support row. NTCP2 stays experimental
and non-advertised; Milestone 3 stays open until Plan 065 closes
with one complete four-direction live diagnostic bundle and Plan
066 produces a verified Milestone 3 certificate. Plan 063 does not
wire the Java driver into the canonical primary `mixed_runner.py`;
that wiring belongs to Plan 065.

## Plan 064 i2pd direct NTCP2 driver and observer correction

Plan 064 replaces the partial Plan 059 i2pd direct connect helper
with a correctly initialized, dual-mode, source-locked i2pd 2.60.0
NTCP2 interoperability driver. The driver is the test-only
counterpart to the Plan 063 Java driver; together they form the
source-locked pair required by the Plan 061 roadmap. The driver is
**test-only** and never becomes a production dependency of
`i2pr-daemon`.

Plan 064 lands:

- `tests/integration/ntcp2/reference-drivers/i2pd/src/i2pd_ntcp2_interop_driver.cpp`
  — the source-locked C++ driver. It performs the source-verified
  pinned i2pd initialization sequence (`config::Init` →
  `context::ParseConfig` → `fs::SetAppDir` → `crypto::Init` →
  `context::Init` → transport singleton → `netdb.Start` →
  `transports.Start(true, false)` → `context.Start`), uses the real
  pinned NTCP2 transport (no patch, no SSU2, no SAM, no I2CP, no
  HTTP/I2PControl), and submits a real correlated
  `CreateDeliveryStatusMsg(delivery_status_message_id)` through
  `Transports::SendMessage` exactly once. The driver exposes
  `inspect`, `listen`, and `dial` modes through a strict config
  contract. The receive observer is placed immediately after
  `HandleData()` completes AEAD verification, block bounds
  validation, and `FromNTCP2` conversion; the send observer is
  placed in the successful branch of `HandleI2NPMsgsSent()` (or
  pinned equivalent).
- `tests/integration/ntcp2/reference-drivers/i2pd/src/interop_observer.h`
  and `interop_observer.cpp` — the compile-time-gated passive
  observer API and sink. The observer is `noexcept`, never blocks
  the transport thread on unbounded I/O, writes only to an owned
  bounded sink, and drops the observation with a typed local
  counter if the sink is unavailable.
- `tests/integration/ntcp2/reference-drivers/i2pd/patches/i2pd-2.60.0-interop-observer.patch`
  — the minimal observer patch that activates the post-AEAD
  receive seam and the successful frame-write send seam.
- `tests/integration/ntcp2/reference-drivers/i2pd/source-lock.json`
  — the source-lock record
  (`i2pr-i2pd-direct-driver-source-lock-v1`) binding the pinned
  i2pd revision `f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e`, the
  helper source path, the build contract, the observer patch
  digest, and the locked constraints, and the required
  verification controls. Plan 064 explicitly eliminates the eight
  defects of the Plan 059 helper: 64-hex SHA-256 Router Hash,
  NTCP2-address static-key binding, source-verified pinned
  initialization, real `CreateDeliveryStatusMsg` dispatch, bounded
  `SendMessage` asynchronous semantics, sealed-topology
  reserved-range disable, exact post-AEAD receive correlation,
  and measured provenance for every helper input.
- `tests/integration/ntcp2/reference-drivers/i2pd/build-manifest.schema.json`
  — the build-manifest schema
  (`i2pr-i2pd-direct-driver-build-manifest-v1`) that requires
  measured digests for the i2pd source tree, the observer patch,
  the helper source and binaries, the linked library manifest,
  the CMake version, and the compiler version. No zero or
  placeholder digest is allowed in an attempted run.
- `tests/integration/ntcp2/reference-drivers/i2pd/CMakeLists.txt`,
  `build-driver.sh`, and `run-driver.sh` — the offline build and
  runtime seams. The build script verifies the pristine pinned
  source tree digest, applies exactly one reviewed observer
  patch with `--fuzz=0` equivalent behaviour, builds the
  instrumented driver, restores the pristine tree, builds the
  uninstrumented control driver, and emits two build manifests.
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

Plan 064 extends the Plan 062 source-verification record
(`tests/integration/ntcp2/reference-drivers/source-verification.md`)
with the canonical two-process topology contract for the i2pd
driver. The Plan 062 v4 trigger schema, the Plan 062 reference-
event v1 schema, and the Plan 062 v3 observation schema remain the
authoritative schemas for the direction records. The legacy Plan
059 helper at `tests/integration/ntcp2/reference-drivers/i2pd_direct_connect/`
is replaced by a fail-closed compatibility stub with the explicit
Plan 064 supersedure marker; the original source-lock record is
preserved verbatim as the bounded historical-reader path.

Plan 064 extends `scripts/check-ntcp2-interoperability.sh` to
enforce the i2pd driver source, the observer header, the observer
source, the observer patch, the source-lock record, the
build-manifest schema, the CMakeLists, the build/run scripts, the
Python adapter, the test matrices, the control topology contract,
the qualification receipt, and the Plan 059 supersedure marker.
The active v4 trigger, v3 observation, and reference-event schemas
continue to enforce the 64-hex SHA-256 Router Hash contract; the
historical v3 trigger and v2 observation paths remain the bounded
historical-reader path.

Plan 064 does not advance any support row. NTCP2 stays experimental
and non-advertised; Milestone 3 stays open until Plan 065 closes
with one complete four-direction live diagnostic bundle and Plan
066 produces a verified Milestone 3 certificate. Plan 064 does not
wire the i2pd driver into the canonical primary `mixed_runner.py`;
that wiring belongs to Plan 065.

## Plan 065 NTCP2 canonical integration and live qualification

Plan 065 wires the corrected Java and i2pd direct drivers into the
canonical four-direction mixed-router lane, enforces the exact
DeliveryStatus correlation on the i2pr side, and produces one
complete four-direction live diagnostic bundle from a clean
implementation commit. Plan 065 establishes the implementation
floor from which Plan 066 may cut a candidate.

### Workstream A: i2pr scenario contract

The strict launcher scenario schema is bumped to
`i2pr-launcher-scenario-v2` (schema name string) / 2 (schema version
integer). The strict parser requires the per-run DeliveryStatus
`message_id` in `1..=0xffffffff`, the 64-lowercase-hex expected sender
and receiver Router Hashes, the `reference_driver_mode` field
allowlisted to `java-direct-driver` or `i2pd-direct-driver`, and the
`run_identity_sha256` 64-lowercase-hex digest. The strict parser
refuses the historical schema 1 path, refuses zero message IDs,
refuses uppercase or short Router Hashes, refuses all-zero
provenance, and refuses a reference driver mode that does not match
the direction encoded by `scenario_id`.

The Rust strict parser lives in `tools/i2pr-interop/src/scenario.rs`;
the Python strict parser lives in
`tests/integration/ntcp2/harness/launcher_protocol.py`. The strict
renderer lives in `tests/integration/ntcp2/harness/launcher_renderer.py`.

### Workstream B: i2pr launcher send correction

The `send_i2np_block` helper no longer hard-codes the
`0x0420_0001` DeliveryStatus authority. The helper accepts the
scenario-owned message ID, rejects a zero ID with
`SenderDeliveryStatusMessageIdZero`, constructs the DeliveryStatus
envelope using the exact ID, decodes the constructed message and
verifies the round-trip envelope and payload message IDs before
frame emission, and records the per-run DeliveryStatus
`message_id` and the expected peer Router Hash in the typed
counters. The hard-coded `0x0420_0001` is removed from the active
primary code path.

### Workstream C: i2pr launcher receive correction

The `receive_delivery_status` helper requires the exact envelope
message ID and the DeliveryStatus payload message ID before any
other condition. A type-only DeliveryStatus match is rejected with
`ReceiverDeliveryStatusIdMismatch`. A missing DeliveryStatus is
rejected with `ReceiverDeliveryStatusMissing`. A duplicate is
rejected with `ReceiverDeliveryStatusDuplicate`. The helper records
the per-run DeliveryStatus `message_id` and the expected peer
Router Hash in the typed counters.

### Workstream D: reference adapter integration

The canonical mixed-runner wires the new scenario primary fields
through `render_and_validate` for both the i2pr initiator and
responder paths. The `_plan065_primary_fields` helper derives the
DeliveryStatus `message_id` from the run identity and the
correlation nonce; the `_reference_driver_mode_for` helper returns
the source-locked driver mode for a reference kind. The renderer
rejects SAM, HTTP, I2PControl, support-topology, and
synthetic-fallback helpers for any primary direction.

### Workstream E: canonical two-process topology

The canonical two-process topology enforces exactly one i2pr
process and exactly one reference driver process per primary
direction. The Plan 046 rootless sealed-namespace lane owns the
canonical external lane; the Plan 048/049 Multipass recovery lane
owns the canonical recovery lane. The Plan 058 deprecated the
privileged dual-netns-veth lane as an opt-in qualification lane.

### Workstream F: pass predicate

The Plan 065 pass predicate requires both-side
`ntcp2_authenticated`, sender `frame_emitted`, receiver
`frame_authenticated_and_decrypted` AND `i2np_message_decoded`, a
matching `delivery_status_message_id` between scenario, trigger,
sender, and receiver, and a matching `peer_router_hash_sha256` /
`local_router_hash_sha256` between trigger, sender, receiver, and
direction record. The reference observation v3 schema carries the
exact correlation fields and the canonical mixed-runner refuses
to mark a direction as `passed` when the synthetic fallback is
used.

### Workstream G: evidence model and durability

The Plan 052 evidence bundle and the Plan 053 pipeline integration
remain the canonical evidence model. The Plan 065 evidence
retention requires every primary direction to write exactly one
`run-identity`, `environment-manifest`, `direction`, `trigger`,
`observation-v3`, `cleanup`, and `diagnostics/sanitized-summary`
record. The Plan 060 candidate is retired and the
`declared-not-executable` status marker is preserved; the Plan 060
candidate record is preserved verbatim as the bounded
historical-reader path. The Plan 066 implementation floor is the
Plan 065 closure commit or later.

### Workstream H: live qualification sequence

The Plan 065 live qualification sequence is documented in the plan
of record. The Plan 046 rootless sealed-namespace lane is the
canonical external lane; the Plan 048/049 Multipass recovery lane
is the canonical recovery lane. Plan 066 cannot start under the
current four-direction contract until either a future pinned Java
revision is adopted or the closure contract is revised through a
new ADR (because ADR 0021 is Rejected by Plan 058). The Plan 046
rootless sealed-namespace lane returns
`blocked_unprivileged_user_namespace` on this host; the Plan
048/049 Multipass recovery lane is the canonical external path
but cannot complete on this constrained host (per Plan 051).
Plan 066 therefore closes on this host with the typed
environment blocker `blocked_execution_lane_unavailable`.

Plan 065 does not advance any support row. NTCP2 stays experimental
and non-advertised; Milestone 3 stays open until Plan 066
produces a verified Milestone 3 certificate.

## Plan 066 fresh-candidate and authoritative NTCP2 two-run closure pass

Plan 066 is the execution-only pass that cuts one fresh candidate
descended from the Plan 065 implementation floor, selects exactly
one execution lane (direct-host or guest), runs the four primary
IPv4 mixed-router directions twice on independent mutable state,
and produces a verified Milestone 3 certificate over the two
sanitized bundles.

The plan inherits the Plan 058/060 two-lane contract: Lane A
(direct-host, requires `rootless_sandbox_available` on the
execution host) and Lane B (guest, the outer host may continue to
report `blocked_unprivileged_user_namespace` but the Multipass
recovery guest must report `rootless_sandbox_available`). The two
lanes are alternatives; cross-lane combinations are forbidden.

The Plan 066 plan-of-record cannot start under the current
four-direction contract until either a future pinned Java revision
is adopted or the closure contract is revised through a new ADR
(because ADR 0021 is Rejected by Plan 058). The host in the Plan
046 `apparmor_restrict_on` negative baseline cannot exercise the
Plan 046 sealed-namespace lane; the Plan 048/049 Multipass
recovery lane is the canonical external path but cannot complete
on this constrained host (per Plan 051). Plan 066 therefore closes
on this host with the typed environment blocker
`blocked_execution_lane_unavailable`; the candidate is
`declared-not-executable` on this host.

The Plan 066 implementation surface is mandatory:

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

### Plan 066 supersession of Plan 060

Plan 060 was retired by Plan 062. Plan 066 supersedes the Plan 060
two-run certificate authority. The Plan 060 helper module, test
matrix, freeze-readiness checks, candidate record, and closure
record remain mandatory as an audit trail and a Plan 066
prerequisite. Future candidates must descend from the Plan 065
implementation floor or later, must use the Plan 062 v4 trigger
schema, the Plan 062 reference-event v1 schema, the Plan 062 v3
observation schema, and the 64-hex SHA-256 Router Hash contract.

The Plan 066 implementation surface is mandatory regardless of
close outcome. Any change that removes or weakens the Plan 066
helper module, the Plan 066 test matrix, the static boundary
checker extension, or the freeze-readiness invariants must be
re-justified in a new plan-of-record and must not silently weaken
the Milestone 3 evidence gate. NTCP2 stays experimental and
non-advertised; Milestone 3 stays open until a future candidate
produces two independent verified bundles.

## Plan 067 staged interoperability corrective roadmap

Plan 067 is the **active** Milestone 3 corrective roadmap. Plan 067
supersedes Plan 066 as the active execution authority. Plan 066
remains an immutable historical record of the unavailable
release-qualification lane on the constrained host.

Plan 067 separates NTCP2 interoperability evidence into four bounded
tiers:

- **Level 0 — local conformance.** Deterministic local protocol and
  runtime ownership.
- **Level 1 — external loopback smoke.** Two real processes on the
  host loopback. i2pd is the primary initial validator. Emissary is
  conditional. No rootless namespace, no Multipass, no candidate
  freeze, no two-bundle certificate, no reviewer record.
- **Level 2 — repeated development interoperability.** Both
  directions against the primary independent validator (pinned i2pd
  2.60.0), three fresh-state repetitions per direction, exact
  message and identity correlation, bounded negative controls.
- **Level 3 — release qualification.** Java I2P 2.12.0 and i2pd
  2.60.0, isolated no-public-egress lane, reproducible
  source/reference provenance, exact authenticated data-phase
  message correlation, independent fresh state, sanitized durable
  evidence. The Plan 066 certificate verifier may be reused at Level
  3.

Java and i2pd remain required for release qualification. NTCP2 stays
experimental and non-advertised.

## Plan 068 staged evidence and authority correction

Plan 068 implements the staged-evidence and authority correction
that Plan 067 proposes. Plan 068 lands:

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
  (`i2pr-ntcp2-loopback-smoke-v1`).
- `tests/integration/ntcp2/harness/development_validation.py` — the
  Level 2 development-validation summary schema
  (`i2pr-ntcp2-development-validation-v1`).
- The Plan 068 test matrices (`test_evidence_tier.py`,
  `test_loopback_smoke_record.py`,
  `test_development_validation.py`).
- `scripts/check-ntcp2-interoperability.sh` extended to enforce the
  new schema modules, the new test matrices, the ADR 0023
  acceptance marker, and the release-bundle smoke/development
  rejection.

Plan 068 also removes the stale `blocked_java_support_topology_rejected`
interpretation from the active Java path: ADR 0021 remains Rejected
and the Java support topology remains forbidden, but the ADR 0022
direct Java driver is the active Java architecture. Java may still be
unavailable because of host/runtime/build defects, but not because
ADR 0021 forbids the already accepted replacement architecture.

The focused closure baseline for Plans 069-073 is the touched-code
test suite plus `cargo fmt --all --check`, `cargo check --workspace
--all-targets`, `cargo test --workspace`,
`scripts/check-dependency-direction.sh`, and
`scripts/check-runtime-boundaries.sh`. Full historical harness
matrices, rootless checks, and Multipass checks remain available
for explicit integration checkpoints but are not required for
Level 1 or Level 2 closures.

## Plan 074 real-driver and constrained-host corrective roadmap (historical)

Plan 074 is historical execution authority. Plan 085 supersedes its active
sequence with **Plan 082 (implemented) → Plan 083 (implemented, execution
pending) → Plan 084 (implemented, execution pending) → Plan 085 → Plan 086
→ Plan 087 → Plan 088 → Plan 079 (blocked)**. Plans 075, 076, 077, and 080
are closed prerequisites or historical lane records. The Plan 084 historical
`lane-invalidated` closure is reclassified as "runner implementation
completed; required reverse wire execution never occurred" and the active
development decision now lives in `plans/088-status.md`.

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
support = experimental
advertised = false
normal_daemon_activation = disabled
```

The constrained-host lane decision and Plan 077 capability probe remain
historical records. Do not treat capability probing or the pre-protocol
Plan 078 stop as protocol evidence.

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
certificate. A passed Level 1 record satisfies the Plan 068 smoke
schema (`i2pr-ntcp2-loopback-smoke-v1`) and the evidence tier
`external-loopback-smoke`; it never satisfies a release-qualification
predicate.

Plan 069 lands:

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
  entry point. The wrapper must never invoke sudo, namespaces,
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

The Plan 068 Level 1 smoke record schema
(`i2pr-ntcp2-loopback-smoke-v1`) remains the canonical record
contract. Plan 069 consumes it without modifying the schema. A
Level 1 passed record requires every positive boolean to be `True`,
`cleanup_clean = True`, and `network_audit != not-run`. Raw payload,
private key, Noise state, and full RouterInfo bytes are forbidden.

Plan 069 does not claim mixed-router interoperability by itself; the
implementation surface is **scaffolding and fake-process test coverage
only** under Plan 074 until Plan 075 restores direction-aware process
roles, structured reference events, measured provenance, and
fail-closed guards. Plan 069 also does not modify production NTCP2
code, the i2pd direct driver, or the Plan 065 strict launcher
scenario contract.

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

The Plan 046 `apparmor_restrict_on` host remains the Plan 046
negative baseline; the Plan 048/049 Multipass recovery lane is
the canonical external path. Plan 075 closes on this host with
the typed environment blocker `runner-reference-events-missing`
or `runner-synthetic-provenance-rejected` because the pinned i2pd
build cache is not present and the Plan 064 driver is not
linked against the pinned libraries.

Plan 077 has selected and documented the constrained-host capability state;
Plan 078 remains blocked because no full-runtime lane is qualified. No real
mixed-router attempt has occurred.

Plan 085 has since superseded Plan 074 for active execution. Plans 075,
076, 077, and 080 are closed prerequisites; the active sequence is
**Plan 082 (implemented) → Plan 083 (implemented, execution pending) →
Plan 084 (implemented, execution pending) → Plan 085 → Plan 086 → Plan 087
→ Plan 088 → Plan 079 (blocked)**. The Plan 084 historical
`lane-invalidated` closure is reclassified as "runner implementation
completed; required reverse wire execution never occurred" and the active
development decision now lives in `plans/088-status.md`.

## Plan 077 constrained-host execution lane

Plan 077 adds a separate capability-selection boundary for hosts that cannot
use the Plan 046 rootless or Plan 048/049 Multipass paths. The probe at
`scripts/interop/probe-constrained-host-lanes.sh` is inspection-only. It
records Docker CLI/daemon access, QEMU system/TCG availability,
`no_new_privs` support, and the presence of a manual remote workflow.

Selection is fail-closed and ordered: existing Docker with `--network none`,
QEMU TCG with `-nic none`, inherited connected descriptors plus seccomp as a
reduced-scope diagnostic, manual remote Linux, and finally no full-runtime
lane. The common manifest and qualification record are validated by
`tests/integration/ntcp2/harness/execution_lane.py`. A tool, workflow, or
reduced-scope capability is not a qualification; a full-runtime record must
prove loopback-only communication, no public interface or route, exact
artifact digests, the two-process control, result export, and cleanup.

The historical Plan 077 probe found only the reduced descriptor capability.
Plan 080 later qualified the owned Multipass guest used for the single Plan
078 attempt; no Docker/QEMU packaging was added speculatively. See [ADR
0024](../adr/0024-constrained-host-ntcp2-execution-lanes.md), [the Plan 077
record](../../plans/077-status.md), and [the Plan 080 closure](../../plans/080-status.md).

## Active correction: Plan 082 pre-protocol state preparation

The current execution authority is Plan 081 and its Plan 082 child, not the
historical Plan 078 close label. The qualified Plan 080 guest and real Plan
076 i2pd driver are valid prerequisites, while the Plan 078/080 attempt
stopped before TCP. The i2pr launcher therefore has separate test-only
`ntcp2 prepare` and `ntcp2 validate-scenario` operations that reuse the
existing identity, NTCP2 static-key, RouterInfo-signing, and
endpoint-verification code without opening a socket or dialing a peer. The
mixed runner prepares i2pr and the pinned reference, validates both
RouterInfos and Router Hashes, asserts the frozen
`i2pr-minimal-run-identity-v1` digest, and invokes the Rust
`validate-scenario` command before any live process. Plan 082 is
implemented and closed per `plans/082-status.md`.

The mixed runner prepares i2pr and the pinned reference before rendering a
strict Plan 065 scenario, validates both RouterInfos and Router Hashes, and
freezes `i2pr-minimal-run-identity-v1` before any live process. A primary
scenario never uses a generated suffix or empty correlation fields. Preparation,
scenario validation, and the frozen run identity are pre-protocol diagnostics
only; they cannot claim TCP, authentication, frame, or I2NP progress. Plans
083 and 084 own the first minimal i2pd wire probes; the active execution
authority is Plan 085 (host-loopback development roadmap) → Plan 086 (lane
enablement) → Plan 087 (forward probe) → Plan 088 (reverse probe and
development decision). Plan 079 is blocked until the Plan 088 decision.

## Plan 083 minimal i2pr-to-i2pd wire probe

Plan 083 introduces the canonical development diagnostic for the first real
`i2pr -> i2pd` direction. The probe lives at
`tests/integration/ntcp2/harness/minimal_i2pd_probe.py` and uses the locked
record schema `i2pr-minimal-i2pd-probe-v1`. The schema enforces the
strictly-increasing stage model (`not_started`, `state_prepared`,
`peer_router_info_imported`, `listener_ready`, `tcp_connected`,
`noise_authenticated`, `session_confirmed_accepted`,
`authenticated_frame_written`, `authenticated_frame_decrypted`,
`i2np_delivery_status_decoded`), the bounded terminal-result set
(`passed`, `protocol_rejected`, `protocol_timeout`, `pre_protocol_rejected`,
`cleanup_failed`, `lane_invalid`), and the bounded reason-code set. The
generic `typed-harness-operation-failed` reason is explicitly rejected.
Preparation and live process counters are separated; a rendering or
preparation failure cannot fabricate a live process start.

The probe is a development diagnostic, not a release certificate. A passed
probe record requires the final stage, the four canonical observed events
(`ntcp2_authenticated`, `frame_emitted`, `frame_authenticated_and_decrypted`,
`i2np_message_decoded`), clean cleanup, and `reason_code = not_started`. The
probe never imports Plan 056/066 candidate, bundle, certificate,
rootless-topology, or Multipass authority. The bounded schema and the
focused test matrices (`test_minimal_i2pd_probe.py` and `test_plan083.py`)
are committed; the probe does not invoke the broad Plan 045/052 release-style
finalization path.

The runner lives at `tests/integration/ntcp2/harness/plan083_runner.py` and
owns the 11-step execution architecture. It is structurally incapable of
producing a mixed-router pass unless it launches one real i2pr process and
one configured real reference process and consumes authentic structured
events from both. The C++ i2pd direct driver is the only allowlisted
reference driver mode; the runner refuses to fall back to SAM, HTTP,
support-topology, or synthetic-fallback helpers for any primary direction.
The runner never imports Plan 056/066 candidate, bundle, certificate,
rootless-topology, or Multipass authority. The implementation surface is
in place on this host; no real wire attempt has been executed because the
host is the Plan 046 `apparmor_restrict_on` negative baseline and the Plan
080 Multipass guest cannot complete on this constrained host.

## Plan 078 first real i2pd two-way execution

Plan 078 used the Plan 080-qualified guest and stopped before TCP at the i2pr
pre-protocol RouterInfo stage. No protocol pass or failure was inferred. The
exact stop result is in [`plans/078-status.md`](../../plans/078-status.md),
with the qualified-lane record in [`plans/080-status.md`](../../plans/080-status.md).

## Plan 072 activation gate

Plan 072 is a conditional differential lane, not the next executable plan. It
may start only after Plan 088 reaches a real wire stage, i2pr and i2pd disagree
at a precise stage that source/specification review cannot own, and
[`plans/088-status.md`](../../plans/088-status.md) records
`decision = ambiguous-reference-divergence` plus one exact diagnostic
question. Preparation, rendering, cleanup, and generic pre-protocol failures
never satisfy this gate. The Plan 072/079 gate amendment
[`plans/072-079-gate-amendment-plan-088.md`](../../plans/072-079-gate-amendment-plan-088.md)
records the active gate authority.

## Plan 088 reverse host-loopback probe and development decision

Plan 088 owns the reverse `i2pd -> i2pr` direction and the active
development decision. Plan 088 inherits the Plan 086
`host-loopback-development` lane (literal IPv4 loopback, network ID
99, development-only) and reuses the Plan 084 reverse probe schema
(`i2pr-minimal-i2pd-reverse-probe-v1`) and runner orchestration module
(`plan084_runner.py`) unchanged.

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
`same-stage-two-way-i2pr-defect` tokens are forbidden by the static
boundary checker.

On this host the recorded decision is `insufficient-evidence` because
the Plan 086 lane has not closed, the Plan 087 forward direction has
not executed, and no real wire run has been retained. The Plan 088
implementation surface travels with the repository unchanged for any
future host where the Plan 086 lane becomes executable or the Plan
089 manual-isolated fallback becomes available. See
[`plans/088-status.md`](../../plans/088-status.md) for the closure
record. NTCP2 remains experimental and non-advertised.
