# Controlled NTCP2 testnet boundary

Plan 035 permits socket tests only on loopback or an explicitly authorized
isolated testnet. This document is a harness contract, not a runnable public
bootstrap configuration.

The harness must pin Java I2P and i2pd versions, create synthetic identities
and independently stored NTCP2 static key/IV records, assign private literal
IPv4/IPv6 endpoints, select inbound/outbound roles, and capture only bounded
typed events. Each run records versions, configuration identifiers, deterministic
seed/scenario names, timeout policy, and teardown counters. It must not retain
private keys, RouterInfo payloads, I2NP bytes, raw endpoint diagnostics, or
arbitrary remote error text.

The harness must fail closed when a target is not loopback or inside the
explicit isolated namespace. It must not use reseeding, public bootstrap,
automatic address discovery, NAT mapping, RouterInfo publication, or NetDB
mutation. Every process and task is terminated and drained before the run is
reported complete.

The reproducible Plan 036/037 lane is documented in
[`tests/integration/ntcp2/README.md`](../tests/integration/ntcp2/README.md),
with exact reference pins in its manifest and a fail-closed repository
preflight in `scripts/check-ntcp2-interoperability.sh`. The lane is manual and
does not run from normal CI. Plan 037 adds local ownership and parser
corrections, but mixed-router handshake and data evidence is still not present
in this checkout because the complete wire-level adapter and an authorized
testnet run are not available; this remains a closure blocker.

Plan 044 adds a mixed-router scenario expansion layer under
`tests/integration/ntcp2/mixed-scenarios/`. It defines four directional
i2pr/reference IPv4 scenarios, composes `I2prAdapter` with each reference
adapter through a strict launcher renderer, and uses a data-phase oracle
that does not rely on an echo assumption. The expansion layer converts the
component implementations into one executable path, but no completed
mixed-router i2pr record is present in this checkout.

## Plan 038 Ubuntu harness boundary

Plan 038 defines the first harness foundation for Ubuntu amd64. It has two
security domains. Preparation may use `apt` and network access only to install
declared tools and fetch the locked Java I2P 2.12.0 and i2pd 2.60.0 revisions;
it records source, tool, build-command, and artifact hashes. Execution must
run from those prepared inputs without downloads, DNS, reseed, bootstrap,
RouterInfo publication, NetDB mutation, or public endpoints.

The commands are:

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

Before a router starts, the harness creates one namespace per participant and
connects them only with a veth pair. Both veth endpoints leave the host
namespace. Each participant gets loopback, its scenario interface, and only
the expected directly connected IPv4/IPv6 routes. Missing default-route and
public-egress checks are fatal; namespace-scoped nftables rules are only
defense in depth. Cleanup must terminate and drain processes, delete the
namespaces and veth interfaces, and remove secret-bearing run state. Cleanup
failure is a scenario failure.

Environment smoke means only reference startup/readiness and clean teardown.
Reference crosscheck means Java I2P and i2pd exercise one another through the
isolated topology; it is not i2pr evidence. i2pr mixed-router evidence starts
only with bounded authenticated runs between i2pr and each reference in both
directions. Records may retain typed outcomes, bounded metadata, and hashes of
sanitized artifacts/configuration. Raw addresses, identities, RouterInfo,
I2NP, keys, transcripts, logs, and arbitrary remote error text must be deleted.

## Plan 043 build-system boundary

The Ubuntu build-system lane makes the trust-domain split and promotion order
explicit. Preparation may use network access only for the declared Ubuntu
packages, the locked Java I2P/i2pd sources, the verified IzPack artifact, and
their declared build dependencies. Execution starts only after preparation and
cache validation complete; it is offline and restricted to the disposable
namespace-local veth topology described above.

The terminal gate order is:

```text
contract -> reference-build -> reference-offline-reuse -> environment-smoke
-> reference-crosscheck-ipv4 -> i2pr-handshake-smoke-ipv4 -> full-matrix
-> evidence-validation -> cleanup-verification
```

The reference-control profile must pass before an i2pr profile is eligible. It
uses separate `java-*` and `i2pd-*` namespaces, the explicit private network
ID 99, staged strict RouterInfo validation/import, controlled direction policy,
and dual authenticated observations. It is a harness control and never an
i2pr support claim. The i2pr gate requires four independent authenticated
i2pr↔reference IPv4 directions and bounded DeliveryStatus exchange. TCP
connectivity, listener readiness, RouterInfo production, or local launcher
success cannot substitute for that evidence.

Cache reuse is restore-only and keyed by the canonical reference ID, complete
source revision, lock digest, Ubuntu/architecture contract, build-command
version, and relevant tool/ABI metadata. The complete runtime tree is
re-hashed before execution. No cache miss may fetch or use an arbitrary cache;
identities, keys, RouterInfo, NetDB state, rendered configs, run roots, raw
logs, namespaces, and evidence records are excluded from caches.

Evidence is finalized only after process and namespace cleanup. The aggregate
manifest records expected and actual scenario records, hashes, gate
dispositions, and cleanup verification. Only sanitized JSON and approved
hashes are retained. Cleanup runs unconditionally, and an independent clean-
host check must reject residual prefixed namespaces/veths, router or launcher
processes, secret-bearing run roots, forbidden files, and attributable global
nftables/routes/forwarding changes. Cleanup failure converts a protocol pass to
failure.

Promotion is manual first, scheduled only after repeated clean-checkout and
cache-reuse success, then a current successful run at Milestone 3 closure. A
trusted pull-request lane requires a separate decision and must not expose
privileged execution to forked or untrusted code. The current checkout has no
mixed-router evidence and remains experimental/non-advertised.

## Plan 046 rootless sealed-namespace evidence lane

Plan 046 replaces the host-global namespace requirement for the primary NTCP2
interoperability evidence path with a **rootless, process-scoped sandbox**.
The primary mixed-router evidence topology is `rootless-sealed-single-netns`
with privilege model `unprivileged-userns`. The legacy
`privileged-dual-netns-veth` topology is renamed and reserved for explicit
later qualification work; it is never the default and never a silent
fallback.

The rootless topology is sufficient for the primary protocol compatibility
proof because it exercises real TCP sockets, exact local/peer address
binding, RouterInfo validation, NTCP2 obfuscation and Noise handshakes,
authenticated link promotion, encrypted frame write/read paths, directional
I2NP send/receive behavior, and process lifecycle/deadline/cancellation/
cleanup. It does not claim separate-stack network behavior, asymmetric
firewall semantics, packet loss, route mutation, or interface-failure
semantics. The retained claim is intentionally narrow.

The sandbox contains only `lo` plus the synthetic `192.0.2.{1,2}/32`
addresses (and optional `2001:db8:36::{1,2}/128`). The structural isolation
basis is the freshly created user, network, mount, and PID namespaces with a
single-ID UID/GID mapping, `setgroups deny`, and `no_new_privs`. No
host-visible named namespace, veth, or firewall mutation occurs. The
topology is runnable by an ordinary user as long as the host allows
unprivileged user namespaces.

The new command surface is:

```text
bash scripts/interop/probe-rootless-sandbox.sh            # strict typed probe
bash scripts/interop/rootless-enter.sh --probe           # sandbox-only verify
bash scripts/interop/rootless-enter.sh --scenario <id>   # bounded direction
```

The lane forbids automatic fallback to the privileged topology. A missing
rootless capability is a typed blocker, not a skipped success. The mixed-
router evidence schema now carries `topology_kind`, `privilege_model`,
`sandbox_attestation_sha256`, and `parent_network_state_unchanged`. A passed
record that violates any of those is rejected. The static rootless boundary
checker (`scripts/check-rootless-interop-boundary.sh`) fails the change
whenever rootless-owned files contain sudo, host-network-state mutation,
capability grants, privileged containers, or any fallback. NTCP2 remains
experimental and non-advertised; Milestone 3 is still open.

## Plan 048/049 Multipass recovery environment

The host remains the Plan 046 negative baseline; Plans 048 and 049 do not
change its AppArmor or user-namespace policy. On a host with Multipass, the
disposable Ubuntu 24.04 amd64 guest described by
`scripts/interop/multipass/environment.toml` supplies the
`host.apparmor-restrict-off` recovery category. Cloud-init applies the
permissive sysctls inside the guest only and creates the non-sudo `i2ptest`
execution user.

The reviewed environment ID is stable and distinct from the generated run ID,
concrete instance name, and instance generation. The default lane reserves
sanitized host lifecycle state atomically before launch and allocates a fresh,
bounded name; the legacy `i2pr-interop-rootless` name is not authoritative.
Ownership requires a matching host/guest token and contract digest, not a name
match. `--inspect` is read-only; `--adopt-owned`, `--resume-owned`,
`--recreate-owned`, and `--destroy-owned` are explicit operations. Unowned or
ambiguous instances are never silently adopted or mutated, and global
`multipass purge` is not allowed in normal recovery.

Use `bash scripts/interop/multipass/run-evidence-lane.sh --all` for the
collision-safe create, immutable source/cache transfer, snapshot, early and
final guest probe, offline transition, four-direction matrix, validation, and
export sequence. The host baseline probe is recorded separately and does not
substitute for the guest gate. The cache root is `target/interop/cache`, not
`target/interop/build/cache`; host mounts are not authoritative.
`export-evidence.sh` preserves only sanitized records under
`target/interop/evidence/multipass/<run-id>/`, and explicit destruction of an
owned instance leaves that directory intact. Records include environment,
run/generation, ownership, and probe attribution; mixed generations cannot
form a passing manifest. Multipass, guest-policy, rootless-probe, offline,
cleanup, and evidence failures are typed blockers, never support claims.
