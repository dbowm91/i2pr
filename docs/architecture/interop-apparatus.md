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

## Plan 048/049 Multipass recovery environment

The host-level Plan 046 AppArmor restriction remains unchanged as the negative
baseline. Plan 048 adds a disposable Multipass Ubuntu 24.04 amd64 guest for
the `host.apparmor-restrict-off` recovery category. Plan 049 corrects its
lifecycle ownership model. The reviewed environment contract is identified by
a stable environment ID, while each execution has a separate run ID and each
realization has a generation-bound concrete instance name. The legacy
`i2pr-interop-rootless` name is not authoritative.

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
