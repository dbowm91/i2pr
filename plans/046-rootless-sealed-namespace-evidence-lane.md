# Plan 046: Rootless sealed-namespace interoperability evidence lane

## Objective

Replace the privileged host-global namespace requirement for primary NTCP2 interoperability evidence with a rootless, process-scoped sandbox that uses an unprivileged user namespace and a sealed network namespace.

The primary Plan 045 evidence lane must be runnable by an ordinary user without `sudo`, passwordless elevation, host capabilities, setuid helpers, privileged containers, host-visible named network namespaces, host-visible veth devices, or host nftables mutation.

The completed phase must provide a trustworthy evidence path in which:

- the exact i2pr and reference-router binaries are built as the invoking user;
- reference preparation may use the network only for pinned source acquisition and verified cache construction;
- scenario execution is offline and occurs inside a new unprivileged user namespace and a new process-scoped network namespace;
- the sandbox network namespace has no inherited host interface, no default route, no public-network path, and no host port exposure;
- i2pr and the selected reference router bind distinct synthetic addresses inside the sandbox;
- the four Plan 045 directional scenarios execute through real kernel TCP sockets and the real NTCP2 handshake/data path;
- the sandbox disappears when the supervising process exits;
- parent-host network state is byte-for-byte equivalent under a canonical pre/post digest;
- evidence identifies the topology and privilege model honestly;
- no passing result can be produced when the rootless isolation contract is unavailable or unverified;
- no rootless execution path silently falls back to `sudo` or the existing privileged topology.

This plan changes how evidence is gathered. It does not weaken Plan 045's identity-continuity, dual-authentication, directional data-phase, evidence-sanitation, cleanup, or aggregate-pass requirements.

Plan 046 does not advertise NTCP2 support and does not close Milestone 3 by itself. Milestone 3 remains open until successful retained evidence is separately reviewed against `plans/000-mvp-roadmap.md`, `plans/030-milestone-3-overview.md`, and `specs/CONFORMANCE.md`.

## Starting repository state

This plan starts from `main` commit:

```text
374ad9534db1eb113b30e2ab34ef3f997b942ccd
```

The immediately preceding implementation commit is:

```text
0f444020b18b8a22c9ed1e4e774d94da00fb15e1
interop: close Plan 045 NTCP2 mixed-router corrective defects (D1-D10)
```

Plan 045 has corrected the mixed-runner's identity continuity, RouterInfo path, launcher schema, directional data modes, trigger execution, oracle pass predicate, evidence fields, scenario routing, and unknown-reference behavior. No privileged Ubuntu mixed-router evidence has been produced. The current evidence path is still held behind a topology implementation that creates host-global named namespaces, veth devices, and nftables state through `sudo`.

Relevant files include:

- `plans/045-ntcp2-mixed-router-proof-closure-corrective-pass.md`
- `plans/044-closure.md`
- `tests/integration/ntcp2/harness/topology.py`
- `tests/integration/ntcp2/harness/mixed_runner.py`
- `tests/integration/ntcp2/harness/reference_runner.py`
- `tests/integration/ntcp2/harness/runner.py`
- `tests/integration/ntcp2/harness/i2pr.py`
- `tests/integration/ntcp2/harness/java_i2p.py`
- `tests/integration/ntcp2/harness/i2pd.py`
- `tests/integration/ntcp2/harness/reference_trigger.py`
- `tests/integration/ntcp2/harness/data_oracle.py`
- `scripts/interop/run-scenario.sh`
- `scripts/interop/run-matrix.sh`
- `scripts/interop/run-gate.sh`
- `scripts/interop/ubuntu/check-host.sh`
- `scripts/interop/ubuntu/setup-host.sh`
- `scripts/interop/verify-clean-host.sh`
- `scripts/interop/cleanup.sh`
- `.github/workflows/ntcp2-interop-ubuntu.yml`

## Controlling documents

The implementing agent must read and preserve the boundaries in:

- `plans/000-mvp-roadmap.md`
- `plans/030-milestone-3-overview.md`
- `plans/039-plan-038-corrective-interoperability-roadmap.md`
- `plans/040-interop-apparatus-corrective-pass.md`
- `plans/041-reference-router-private-crosscheck.md`
- `plans/042-runtime-owned-ntcp2-wire-driver.md`
- `plans/043-ubuntu-build-system-interop-gates.md`
- `plans/044-ntcp2-interop-final-integration-corrective-pass.md`
- `plans/044-closure.md`
- `plans/045-ntcp2-mixed-router-proof-closure-corrective-pass.md`
- `docs/adr/0015-ubuntu-reference-router-harness.md`
- `docs/adr/0016-ubuntu-build-system-interop-gates.md`
- `docs/architecture/interop-apparatus.md`
- `docs/private-testnet.md`
- `docs/security-model.md`
- `docs/protocol-support.md`
- `specs/CONFORMANCE.md`
- `tests/integration/ntcp2/README.md`
- `tests/integration/ntcp2/evidence/README.md`
- `.opencode/skills/i2pr-ntcp2-interop/SKILL.md`

Where executable behavior and documentation disagree, fail closed and correct both in the same commit.

## Architectural decision

The primary evidence topology will be:

```text
ordinary host user
  |
  +-- unprivileged user namespace
        |
        +-- mount namespace
        +-- PID namespace
        +-- one sealed network namespace
              |
              +-- loopback interface only
              +-- 192.0.2.1/32 bound by i2pr
              +-- 192.0.2.2/32 bound by reference router
              +-- optional 2001:db8:36::1/128 bound by i2pr
              +-- optional 2001:db8:36::2/128 bound by reference router
              +-- no default route
              +-- no host interface
              +-- no forwarded port
              +-- no public-network path
```

Both routers execute inside the same sealed network namespace but use distinct exact bind addresses and independent state directories. This topology is sufficient for the primary compatibility proof because it exercises:

- real TCP sockets;
- exact local and peer address binding;
- RouterInfo address and key validation;
- NTCP2 obfuscation and Noise handshakes;
- authenticated link promotion;
- encrypted frame write and read paths;
- directional I2NP send/receive behavior;
- process lifecycle, deadlines, cancellation, and cleanup.

It does not claim separate network-stack behavior, asymmetric firewall behavior, packet loss, route mutation, or interface-failure semantics. Those remain optional qualification work for the existing privileged dual-namespace backend or a later rootless dual-namespace implementation.

The topology identifier must be:

```text
rootless-sealed-single-netns
```

The existing topology must be renamed and identified as:

```text
privileged-dual-netns-veth
```

The rootless topology is the default for Plan 045 handshake evidence. The privileged topology is opt-in only and must never be an automatic fallback.

## Non-negotiable privilege boundaries

1. No rootless script, Python module, Rust binary, workflow step, or test may invoke `sudo`.
2. No rootless path may invoke `ip netns add`, create entries under `/run/netns`, mutate host links, mutate host routes, or mutate host nftables.
3. No rootless path may use `setcap`, file capabilities, setuid binaries created by this repository, ambient host capabilities, `docker --privileged`, `podman --privileged`, `--network host`, or a privileged sidecar.
4. Do not grant `CAP_NET_ADMIN` to Python, `ip`, the reference routers, `i2pr-interop`, or the test harness in the initial host user namespace.
5. Capabilities held as UID 0 inside the newly created user namespace are permitted only when the UID/GID map is a single invoking-user mapping and the owned network namespace is newly created by that user namespace.
6. The sandbox must set and verify `no_new_privs` before starting either router.
7. The rootless lane must not install system packages. It verifies dependencies and emits a typed blocker when the environment is incomplete.
8. Preparation and execution remain separate. Pinned reference source acquisition and cache construction may use the network; scenario execution must be offline.
9. Public I2P reseed, discovery, RouterInfo publication, transit, tunnels, proxy services, SAM exposure outside the sandbox, I2CP exposure outside the sandbox, console exposure, and SSU2 remain prohibited.
10. Cleanup failure overrides protocol success.
11. Rootless capability failure is a blocker, not a skipped success.
12. The harness must not fall back to the privileged backend unless the operator explicitly selects that backend in a separate command or workflow.

## Threat model and evidence claim

The rootless lane is intended to prove protocol interoperability while preventing accidental public-network contact and host-network mutation.

The lane must defend against:

- a reference router ignoring an intended no-reseed setting;
- a process binding `0.0.0.0` or `::` instead of its declared synthetic address;
- a process attempting DNS or public address resolution;
- a process attempting to create an outbound public socket;
- a stale host-global namespace or interface from an earlier run;
- a sandbox process surviving the supervisor;
- a passing record generated outside the sandbox;
- a rootless probe that succeeds partially but lacks a usable network namespace;
- a user namespace whose UID/GID mapping is broader than the invoking user;
- a rootless implementation that accidentally mutates parent-host network state;
- evidence that retains raw namespace identifiers, UIDs, paths, endpoints, logs, RouterInfo, or I2NP contents.

The retained claim must be limited to:

> The pinned i2pr and reference-router processes completed the declared NTCP2 direction inside a process-scoped, rootless user/network namespace whose canonical isolation checks passed and whose creation and teardown did not alter the parent host's canonical network state.

Do not describe the single-network-namespace topology as two isolated network stacks.

## Deliverable 1: Add an ADR for rootless evidence execution

Create:

```text
docs/adr/0017-rootless-sealed-namespace-interop-evidence.md
```

The ADR must record:

- the reason the privileged topology blocked routine evidence gathering;
- the rootless user-namespace and network-namespace design;
- why one sealed network namespace is sufficient for the primary NTCP2 compatibility proof;
- what the topology does not prove;
- the exact privilege boundary;
- the single-ID UID/GID mapping requirement;
- the no-silent-fallback rule;
- the relationship to the existing privileged dual-namespace topology;
- evidence-schema implications;
- host compatibility and typed blocker behavior;
- future option for a process-held rootless dual-namespace supervisor.

Update ADR indexes and architecture documents in the same change.

## Deliverable 2: Introduce a topology backend contract

Refactor `tests/integration/ntcp2/harness/topology.py` so callers no longer depend directly on named namespace strings or `ip netns exec`.

Define a narrow backend protocol, for example:

```python
class InteropTopology(Protocol):
    topology_kind: str
    description: TopologyDescription

    def create(self) -> None: ...
    def command_prefix(self, actor: str) -> list[str]: ...
    def verify_before_start(self) -> IsolationAttestation: ...
    def verify_during_run(self) -> IsolationAttestation: ...
    def destroy(self) -> str: ...
```

Required actor values:

```text
i2pr
reference
control
```

Implement:

```text
RootlessSealedTopology
PrivilegedDualNamespaceTopology
```

Move the current named-netns/veth/nft behavior into `PrivilegedDualNamespaceTopology` without weakening its existing checks.

`RootlessSealedTopology.command_prefix()` returns an empty prefix because the whole inner runner already executes inside the sealed namespace.

Do not let adapters inspect effective UID and infer whether to use `sudo`. Process placement must be explicit and supplied by the selected topology backend.

## Deliverable 3: Add an explicit process executor boundary

Create a small execution abstraction used by:

- `I2prAdapter`;
- `JavaI2pAdapter`;
- `I2pdAdapter`;
- reference triggers;
- data-phase control probes;
- readiness probes;
- canary processes.

Suggested interface:

```python
@dataclass(frozen=True)
class ProcessPlacement:
    topology_kind: str
    actor: str
    command_prefix: tuple[str, ...]

    def command(self, argv: Sequence[str]) -> list[str]: ...
```

Requirements:

- rootless execution uses direct child processes;
- privileged execution uses the existing named-netns prefix;
- no adapter constructs `sudo` or `ip netns exec` itself;
- unknown topology kinds fail closed;
- arbitrary command prefixes cannot enter from scenario files, workflow inputs, or environment variables;
- control probes use the same placement as the reference router;
- all process logs remain under the disposable run root and are never retained as evidence.

Add static checks rejecting direct `sudo` and `ip netns exec` construction outside the privileged backend.

## Deliverable 4: Implement the rootless outer entrypoint

Create a strict outer entrypoint, for example:

```text
scripts/interop/rootless-enter.sh
```

It must accept only the fixed operations required by the harness, such as:

```text
--probe
--scenario <allowlisted-scenario-id>
--profile <allowlisted-profile>
```

It must not accept an arbitrary command string.

The entrypoint must create the sandbox with the equivalent of:

```text
new user namespace
single invoking-user UID map
single invoking-user GID map
setgroups denied
new network namespace
new mount namespace
new PID namespace
forked child supervisor
kill child on supervisor exit
private mount propagation
mounted private /proc for the PID namespace
```

A util-linux `unshare` implementation is acceptable for the first version when:

- all arguments are fixed or allowlisted;
- no `eval` is used;
- no shell-fragment input is accepted;
- the exact required `unshare` features are probed;
- the inner supervisor verifies rather than trusts the requested namespace state.

The entrypoint must export a fixed marker such as:

```text
I2PR_INTEROP_ROOTLESS_INNER=1
```

The inner runner must reject direct invocation with this marker forged unless its namespace and mapping verification passes.

## Deliverable 5: Implement the rootless inner supervisor

Create a focused inner supervisor, preferably:

```text
tests/integration/ntcp2/harness/rootless_supervisor.py
```

A later Rust migration is allowed, but the first implementation must remain small, auditable, and covered by process-level tests.

The supervisor must:

1. Verify it is inside a user namespace distinct from the parent context.
2. Verify the UID map is exactly one inside UID mapped to the invoking host UID.
3. Verify the GID map is exactly one inside GID mapped to the invoking host GID.
4. Verify `/proc/self/setgroups` is `deny` where applicable.
5. Set `no_new_privs` and verify `/proc/self/status` reports it.
6. Verify it is inside a network namespace distinct from the parent namespace.
7. Bring up only `lo`.
8. Add only the reviewed synthetic addresses.
9. Confirm no non-loopback interface exists.
10. Confirm no default route exists in any routing table.
11. Confirm a route lookup for an external documentation-range address fails.
12. Confirm exact synthetic bind and connect behavior with a bounded canary.
13. Record a sanitized isolation attestation.
14. Execute the requested inner scenario/profile runner.
15. Reap all descendants.
16. Verify no router or canary process remains.
17. Return the typed inner status to the outer entrypoint.

The supervisor must close unrelated inherited file descriptors before launching routers.

Use a parent-death signal or equivalent supervisor lifetime mechanism in addition to PID-namespace teardown.

## Deliverable 6: Configure the sealed network namespace

The rootless topology must initially use only loopback.

Required IPv4 addresses:

```text
192.0.2.1/32
192.0.2.2/32
```

Optional IPv6 addresses when the rootless IPv6 capability probe passes:

```text
2001:db8:36::1/128
2001:db8:36::2/128
```

Required behavior:

- i2pr binds exactly `192.0.2.1` or `2001:db8:36::1`;
- the reference binds exactly `192.0.2.2` or `2001:db8:36::2`;
- control surfaces bind the reference synthetic address, not wildcard addresses;
- no service binds a host-visible interface because the sandbox has none;
- no default route is installed;
- no route to the host namespace is installed;
- no DNS proxy or slirp/pasta user-mode network is started;
- no port forwarding is configured;
- no host socket is passed into the sandbox.

Do not add nftables as a mandatory dependency for the rootless lane. The primary isolation basis is structural: a newly created network namespace with only loopback, reviewed local addresses, and no external route.

Namespace-local nftables may be investigated later as defense in depth, but it must not obscure or replace the structural checks.

## Deliverable 7: Add exact socket-inventory checks

Before starting routers and again after readiness, inspect the sandbox socket table through a structured parser.

Required assertions:

- only reviewed protocol and control ports are listening;
- every listening socket is bound to an expected synthetic address or an explicitly reviewed loopback control address;
- wildcard binds are rejected;
- unexpected UDP listeners are rejected;
- SSU2 listeners are rejected;
- proxy, console, SAM, I2CP, or JSON-RPC listeners are rejected unless they are required for the active scenario and bound to the reviewed synthetic reference address;
- the reference control listener disappears during cleanup;
- no listener survives the scenario.

Retain only typed listener classifications and a digest of the canonical expected/observed socket inventory. Do not retain raw `ss` output or endpoint strings.

## Deliverable 8: Split host contracts into dependency and privilege modes

Refactor the host checker into explicit modes:

```text
--dependency-contract
--rootless-preflight
--privileged-pre-install
--privileged-post-install
```

The rootless contract must not require:

- non-interactive sudo;
- host nftables mutation;
- host named-network-namespace creation;
- veth creation in the initial network namespace;
- package installation.

The rootless dependency contract must verify the exact tools required by the lane, including at minimum:

- Python version;
- Rust 1.95.0 toolchain and required components;
- Java runtime/build tools required by the pinned Java I2P build;
- C/C++ build tools required by the pinned i2pd build;
- `unshare` feature support;
- `ip` support needed inside the user-owned network namespace;
- `/proc` availability;
- sufficient writable disk space;
- UTF-8 locale;
- repository and cache ownership by the invoking user.

Missing dependencies produce:

```text
blocked_rootless_dependency_contract
```

Do not install packages automatically.

## Deliverable 9: Add typed rootless capability probing

Create a standalone probe:

```text
scripts/interop/probe-rootless-sandbox.sh
```

The probe must perform a real bounded create/configure/connect/teardown cycle without starting a router.

Allowed outcomes include:

```text
rootless_sandbox_available
blocked_unprivileged_user_namespace
blocked_uid_map
blocked_gid_map
blocked_setgroups_contract
blocked_network_namespace
blocked_namespace_local_net_admin
blocked_mount_namespace
blocked_private_proc
blocked_no_new_privs
blocked_loopback_configuration
blocked_synthetic_address_configuration
blocked_external_route_present
blocked_external_connect_possible
blocked_rootless_cleanup
```

The probe must emit one strict JSON status line and optionally one sanitized attestation file.

Do not collapse distinct capability failures into a generic host failure.

The workflow and local runner must stop before reference construction when this probe does not return `rootless_sandbox_available`.

## Deliverable 10: Preserve parent-host network state

Add a canonical, unprivileged parent-network snapshot utility.

It must collect and normalize only non-secret structural state needed to prove non-mutation, for example:

- link names, types, and flags;
- address families and prefix metadata with address values redacted or hashed;
- route table structure with destinations and gateways redacted or hashed;
- rule priorities and table identifiers;
- named network namespace names, if listing is permitted;
- listening-socket classifications without endpoint values.

The utility must produce a SHA-256 digest before the sandbox starts and after it exits.

A passing rootless run requires:

```text
parent_network_state_pre_sha256 == parent_network_state_post_sha256
```

If the canonical state differs, classify the run as:

```text
failed_cleanup
parent_network_state_changed
```

Do not attempt privileged cleanup from the rootless path. A parent-state difference is a hard failure requiring operator inspection.

## Deliverable 11: Add a rootless sandbox attestation record

Create a separate sanitized record for each rootless gate invocation.

Suggested type:

```text
rootless-sandbox-attestation
```

Suggested fields:

```text
schema
record_type
date_utc
i2pr_commit
topology_kind
privilege_model
user_namespace_distinct
network_namespace_distinct
mount_namespace_distinct
pid_namespace_distinct
uid_map_class
gid_map_class
setgroups_policy
no_new_privs
external_interface_count
default_route_count
synthetic_ipv4_ready
synthetic_ipv6_disposition
external_route_probe
external_connect_probe
socket_inventory_sha256
sandbox_policy_sha256
parent_network_state_pre_sha256
parent_network_state_post_sha256
parent_network_state_unchanged
child_reap_result
sandbox_cleanup_result
attestation_sha256
known_deviation
reproduction
```

Allowed fixed values must be enum-validated. Passed attestations must not contain zero-filled digests.

Do not retain:

- host UID/GID values;
- raw UID/GID maps;
- namespace inode numbers;
- host interface names unless they are fixed repository-owned synthetic names;
- raw addresses from the host;
- raw routes;
- raw socket tables;
- host paths;
- logs;
- environment-variable dumps.

## Deliverable 12: Bind scenario evidence to the sandbox attestation

Extend the active mixed-router evidence schema in a versioned manner.

Add at minimum:

```text
topology_kind
privilege_model
sandbox_attestation_sha256
parent_network_state_unchanged
```

A passed mixed-router record must require:

```text
topology_kind = rootless-sealed-single-netns
privilege_model = unprivileged-userns
parent_network_state_unchanged = true
sandbox_attestation_sha256 = nonzero SHA-256
```

The aggregate manifest must verify that:

- all four handshake-smoke scenario records reference the same gate attestation;
- the referenced attestation exists and validates;
- the attestation commit matches the scenario-record commit;
- the attestation topology and privilege model match the scenario records;
- cleanup passed in both the scenario records and the attestation;
- no privileged-topology record is mixed into a rootless aggregate;
- no rootless record is relabeled as privileged evidence.

## Deliverable 13: Add a rootless gate catalog

Update the canonical gate catalog established by Plan 045 so topology requirements are explicit.

Each gate entry must declare:

```text
runner_type
allowed_topologies
required_privilege_models
requires_sandbox_attestation
scenario_ids
allowed_evidence_schemas
predecessor_gates
```

For the primary handshake gate:

```text
gate = handshake-smoke-rootless
topology = rootless-sealed-single-netns
privilege_model = unprivileged-userns
```

Required scenarios remain exactly:

```text
i2pr-to-java-ipv4
java-to-i2pr-ipv4
i2pr-to-i2pd-ipv4
i2pd-to-i2pr-ipv4
```

The existing `handshake-smoke` name may be migrated to the rootless topology when all consumers are updated atomically. Otherwise use the explicit new gate name until cutover.

## Deliverable 14: Update adapters and triggers for direct in-sandbox execution

Modify all adapters and control surfaces so the rootless inner runner starts processes directly.

Required corrections:

- remove effective-UID-based `sudo` selection from adapters;
- remove direct namespace-string ownership from adapters;
- pass `ProcessPlacement` into each adapter;
- bind Java I2P control surfaces only when required by the selected directional scenario;
- bind i2pd control surfaces only when required by the selected directional scenario;
- verify control surfaces are unreachable outside the sandbox by construction and socket inventory;
- preserve exact RouterInfo identity continuity from Plan 045;
- preserve source-verified directional trigger behavior;
- preserve the Plan 045 data-phase pass predicate;
- preserve strict status parsing and typed errors;
- preserve run-root confinement and evidence sanitation.

A rootless scenario must not contain a namespace name because process placement is inherited from the sealed outer sandbox.

## Deliverable 15: Create a no-escalation rootless workflow

Create:

```text
.github/workflows/ntcp2-interop-rootless.yml
```

The workflow must be manual initially and must contain no `sudo` invocation.

Required stages:

1. Checkout source.
2. Install the repository Rust toolchain as the job user.
3. Run static and deterministic repository checks.
4. Run the rootless dependency contract.
5. Run the rootless capability probe.
6. Build pinned Java I2P and i2pd caches as the job user.
7. Verify exact cache manifests.
8. Verify offline cache reuse.
9. Run rootless environment smoke.
10. Run rootless reference crosscheck if it is compatible with the single-netns topology.
11. Run the four-direction rootless handshake-smoke gate.
12. Validate all scenario records and the sandbox attestation.
13. Aggregate the rootless evidence.
14. Verify parent-host network state equivalence.
15. Upload only sanitized evidence and build summaries.

Workflow constraints:

- `permissions: contents: read`;
- fixed Ubuntu 24.04 runner label;
- no arbitrary command, revision, URL, endpoint, network-ID, or topology input;
- no privileged service container;
- no Docker socket use;
- no host networking;
- no package installation step;
- no automatic fallback workflow dispatch;
- bounded timeout;
- single active lane per ref;
- artifact retention remains bounded.

If the managed runner lacks a required dependency or disables unprivileged user namespaces, the workflow must fail with a typed blocker. It must not add privilege to make the run green.

Keep the existing privileged workflow as an explicitly named optional topology-qualification lane. Update its name and documentation so it is not confused with the primary evidence path.

## Deliverable 16: Add a static rootless boundary checker

Create:

```text
scripts/check-rootless-interop-boundary.sh
```

It must fail when rootless-owned files contain prohibited behavior, including:

```text
sudo
ip netns
setcap
getcap-based authorization
--privileged
--network host
/var/run/docker.sock
/run/netns
CAP_NET_ADMIN in the initial user namespace
arbitrary shell command execution
shell eval
automatic privileged fallback
```

It must also verify:

- every new shell script is executable;
- rootless workflows call the rootless dependency and capability probes;
- rootless workflows contain no setup-host/package-install step;
- rootless scenario runners select the rootless backend explicitly;
- privileged-backend code is isolated to reviewed files;
- topology identifiers match the gate catalog;
- evidence validators require the sandbox attestation.

Add this checker to normal CI and to both interoperability workflows.

## Deliverable 17: Add rootless process-level tests

Unit mocks are insufficient. Add process-level tests that invoke the real outer entrypoint when user namespaces are available.

Required probe tests:

- user namespace creation succeeds;
- single-ID UID mapping is verified;
- single-ID GID mapping is verified;
- `no_new_privs` is set;
- network namespace is distinct;
- loopback becomes ready;
- both synthetic IPv4 addresses bind and connect;
- external route lookup fails;
- external connect fails without transmitting through a host interface;
- no non-loopback interface exists;
- parent network digest is unchanged;
- all child processes are reaped.

Required failure-injection tests:

- forged inner marker outside a namespace;
- broader-than-one-ID UID map;
- broader-than-one-ID GID map;
- missing setgroups denial;
- missing `no_new_privs`;
- injected default route;
- injected unexpected interface;
- wildcard protocol listener;
- unexpected UDP listener;
- unexpected control port;
- child process ignores graceful stop;
- supervisor receives SIGTERM;
- evidence write fails;
- parent-state post-digest differs;
- unknown topology kind;
- rootless probe blocked;
- attempted automatic privileged fallback.

Tests requiring user-namespace support must report a typed test skip only in ordinary developer unit-test contexts. The evidence workflow must treat unavailable rootless support as a failed/blocked lane, not a successful skip.

## Deliverable 18: Add simulated rootless mixed-run tests

Reuse the Plan 045 fake reference and launcher processes to exercise the real rootless supervisor and direct process-placement path.

Required cases:

- all four primary directional scenarios;
- identity-continuity preservation inside one sandbox;
- Java trigger path;
- i2pd trigger path;
- i2pr initiator data-only mode;
- i2pr responder data-only mode;
- reference authentication missing;
- sender observation missing;
- receiver observation missing;
- malformed terminal status;
- scenario-ID mismatch;
- process stop timeout;
- sandbox cleanup failure;
- attestation digest mismatch;
- parent-state mutation classification;
- gate aggregation with four records and one attestation.

Every case must assert the exact typed result, cleanup result, and whether evidence is retained.

## Deliverable 19: Define the rootless execution ladder

The first authorized rootless evidence attempt must use this order:

```text
1. static contract checks
2. rootless dependency contract
3. rootless sandbox probe
4. pinned reference build
5. cache-manifest verification
6. offline cache reuse
7. rootless environment smoke
8. rootless reference crosscheck, if supported
9. i2pr-to-java-ipv4
10. java-to-i2pr-ipv4
11. i2pr-to-i2pd-ipv4
12. i2pd-to-i2pr-ipv4
13. evidence validation
14. rootless aggregate creation
15. parent-host state verification
16. sanitized artifact upload
```

Stop immediately when a predecessor fails.

Do not introduce i2pr as the variable under test if the reference-control gate fails.

Do not run IPv6, negative, duplicate-link, resource-pressure, or adversarial scenarios until all four IPv4 directions pass in the rootless topology.

## Deliverable 20: Require repeatable evidence

A single green rootless run is not enough for closure review.

Before Plan 046 may be marked externally complete, retain successful sanitized evidence from:

1. a clean checkout and fresh pinned reference build;
2. a second execution using the verified offline reference caches;
3. a third fresh sandbox process using the same checkout and caches.

All three executions must show:

- the same repository commit;
- the same pinned reference revisions;
- the same reference artifact and installed-tree digests;
- the same canonical configuration digests;
- the same topology kind and privilege model;
- valid, independently generated sandbox attestations;
- all four direction records passing;
- parent-host state unchanged;
- clean child reaping and cleanup.

Runtime-generated identities and RouterInfo digests may differ between clean runs when deterministic identity generation is intentionally disabled. Evidence review must compare the correct invariants rather than requiring unsafe deterministic production identities.

## Deliverable 21: Documentation reconciliation

Update:

- `README.md`;
- `AGENTS.md`;
- `CONTRIBUTING.md`;
- `GUARDRAILS.md`;
- `docs/architecture.md`;
- `docs/architecture/interop-apparatus.md`;
- `docs/private-testnet.md`;
- `docs/security-model.md`;
- `docs/protocol-support.md`;
- `specs/CONFORMANCE.md`;
- `tests/integration/ntcp2/README.md`;
- `tests/integration/ntcp2/evidence/README.md`;
- `.opencode/skills/i2pr-ntcp2-interop/SKILL.md`;
- `.opencode/skills/i2pr-ntcp2-interop/references/operations.md`.

Documentation must state:

- rootless sealed single-netns is the primary evidence topology;
- the topology proves protocol compatibility, not separate-stack network behavior;
- the privileged dual-netns/veth topology is optional qualification;
- no automatic escalation occurs;
- user-namespace unavailability produces a typed blocker;
- evidence requires a sandbox attestation and parent-state equivalence;
- NTCP2 remains experimental and non-advertised until separate Milestone 3 review.

## Deliverable 22: Create a Plan 046 execution record

Create:

```text
plans/046-status.md
```

The status file must distinguish:

- deterministic implementation complete;
- rootless capability probe complete;
- reference-only rootless control complete;
- four-direction rootless handshake evidence complete;
- repeatability complete;
- Milestone 3 evidence review complete.

Do not use `implementation-complete` as a synonym for authenticated evidence.

If the current host blocks rootless user namespaces, record the exact typed blocker and leave external closure open.

## Local validation ladder

Before any rootless evidence attempt, all of the following must pass:

```bash
cargo +1.95.0 fmt --all --check
cargo +1.95.0 check --locked --workspace --all-targets
cargo +1.95.0 test --locked --workspace
cargo +1.95.0 clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo +1.95.0 doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-ntcp2-vectors.sh
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-rootless-interop-boundary.sh
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py'
python3 scripts/interop/validate-scenarios.py
python3 scripts/interop/validate-build-contract.py
python3 scripts/interop/validate-evidence.py
bash -n scripts/check-ntcp2-interoperability.sh scripts/check-rootless-interop-boundary.sh scripts/interop/*.sh scripts/interop/lib/*.sh scripts/interop/ubuntu/*.sh
git diff --check
```

Run the real rootless probe after deterministic checks:

```bash
bash scripts/interop/probe-rootless-sandbox.sh
```

No command in the rootless validation path may require `sudo`.

## Suggested implementation sequence

### Commit A: ADR and topology contract

- add ADR 0017;
- introduce `InteropTopology`;
- rename the existing backend to `PrivilegedDualNamespaceTopology`;
- add topology identifiers;
- preserve existing privileged tests.

### Commit B: Process placement abstraction

- add `ProcessPlacement`;
- migrate i2pr, Java I2P, i2pd, trigger, and oracle process launches;
- remove adapter-owned sudo and named-netns logic;
- add static boundary checks.

### Commit C: Rootless entrypoint and capability probe

- add the strict outer entrypoint;
- add inner namespace verification;
- add the bounded create/configure/connect/teardown probe;
- add typed blocker taxonomy.

### Commit D: Rootless sealed topology

- configure loopback and synthetic addresses;
- implement structural isolation checks;
- implement socket inventory;
- implement child reaping and teardown;
- add parent-network pre/post digests.

### Commit E: Evidence integration

- add rootless sandbox attestation schema;
- version mixed-router evidence;
- bind scenario records to attestation digest;
- update validators and aggregate logic;
- add sanitation and mutation tests.

### Commit F: Gate and workflow integration

- update canonical gate catalog;
- add rootless matrix/gate execution;
- add no-escalation workflow;
- rename existing workflow as optional privileged qualification;
- add workflow static tests.

### Commit G: Process-level and simulated tests

- add real rootless probe tests;
- add failure injection;
- add simulated four-direction rootless runs;
- test signal and cleanup behavior.

### Commit H: Documentation and status

- reconcile all documentation;
- add `plans/046-status.md`;
- record local validation without claiming external evidence.

### External execution commits

After authorized rootless execution, commit only sanitized status/documentation changes when appropriate. Do not commit raw logs, RouterInfo, identities, keys, packet captures, namespace files, or I2NP payloads.

## Acceptance criteria

Plan 046 implementation is locally complete only when:

1. The primary mixed-router runner supports `rootless-sealed-single-netns`.
2. The rootless path contains no sudo or host-global network mutation.
3. The current privileged backend remains explicit and opt-in.
4. Adapters use explicit process placement rather than effective-UID inference.
5. The rootless capability probe emits strict typed results.
6. UID/GID map, namespace distinction, `no_new_privs`, loopback, synthetic addresses, no-interface, no-default-route, and external-connect checks are verified inside the sandbox.
7. Parent-host canonical network state is checked before and after every gate.
8. The sandbox attestation schema validates and rejects placeholders.
9. Mixed-router evidence references a valid attestation digest.
10. Aggregate validation rejects missing, mismatched, privileged, or failed attestations.
11. The rootless gate catalog contains exactly the four Plan 045 IPv4 directions.
12. Static checks reject escalation and privileged fallback.
13. Process-level tests cover success, blocked capability, signal, process-leak, and host-state-mutation cases.
14. All local validation commands pass.
15. Documentation accurately limits the evidence claim.

Plan 046 external evidence gathering is complete only when:

1. The rootless dependency contract passes on the supported Ubuntu 24.04 amd64 build environment.
2. The rootless sandbox capability probe passes.
3. Pinned reference caches are built and verified as the invoking user.
4. Offline cache reuse passes.
5. Reference-control execution passes where applicable.
6. All four directional mixed-router scenarios pass.
7. Every passed scenario has dual authentication and the required directional data observation.
8. Every scenario record references a valid rootless sandbox attestation.
9. Parent-host network state is unchanged.
10. Cleanup and child reaping pass.
11. Aggregate validation passes.
12. The three required repeatability executions pass.
13. Sanitized artifacts contain no prohibited material.

Even after these criteria pass, Milestone 3 remains open until a separate evidence review is completed.

## Stop conditions

Stop and retain a typed blocker rather than escalating when:

- unprivileged user namespaces are disabled;
- the required single-ID UID/GID maps cannot be established;
- `no_new_privs` cannot be verified;
- a new network namespace cannot be created;
- loopback or synthetic addresses cannot be configured inside the owned namespace;
- a non-loopback interface appears;
- a default or external route appears;
- an external connect succeeds;
- a wildcard or unexpected listener appears;
- a child survives supervisor teardown;
- parent-host network state changes;
- a dependency is missing;
- the reference cache cannot be built as the invoking user;
- the evidence attestation cannot be finalized;
- any code attempts automatic privileged fallback.

Do not respond to these conditions by adding passwordless sudo, broad capabilities, setuid wrappers, privileged containers, or host networking.

## Out of scope

The following are explicitly deferred:

- rootless dual-network-namespace/veth implementation;
- packet loss, latency, reordering, or bandwidth shaping;
- asymmetric firewall qualification;
- route deletion during an active link;
- interface-down behavior;
- public-network testing;
- tunnel construction;
- transit routing;
- SSU2 interoperability;
- daemon activation;
- SAM/I2CP product support;
- full adversarial/resource matrix closure;
- non-Linux rootless topology;
- automatic container fallback;
- Milestone 3 closure itself.

## Handoff summary

The implementing agent should treat this as a topology and evidence migration, not a protocol rewrite.

Preserve all Plan 045 protocol and pass-predicate corrections. Replace the primary evidence runner's dependency on host-global named namespaces with a strictly verified rootless sandbox. Keep the privileged dual-namespace path available only for explicit later qualification. Never hide an unavailable rootless capability behind escalation or a successful skip.
