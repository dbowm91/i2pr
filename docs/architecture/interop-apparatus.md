# Plan 040/041/043 interoperability apparatus

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
