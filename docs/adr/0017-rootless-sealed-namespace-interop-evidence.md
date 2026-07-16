# ADR 0017: Rootless sealed-namespace interoperability evidence lane

- Status: accepted for Plan 046
- Date: 2026-07-16
- Decision owners: repository maintainers

## Context

Plan 038/040/041/043/044/045 established an Ubuntu-amd64 NTCP2
interoperability apparatus. The execution leg of that apparatus relies on
host-global named network namespaces (`ip netns add i2pr-*` / `ref-*`), veth
pairs created in the initial network namespace, and `nft` firewall rules
installed through `sudo -n`. A routine evidence run therefore requires
passwordless sudo, host capabilities, and host-visible namespace/veth state.

That posture blocks routine evidence gathering in three concrete ways:

1. Most developer hosts and shared CI workers do not grant passwordless sudo,
   nor do they permit mutating host network state.
2. The privileged topology exposes the parent host to accidental public
   contact if a topology step regresses (for example, a missing route
   isolation rule).
3. The Plan 042/045 launcher is a complete local proof but cannot reach
   mixed-router evidence without a topology that an ordinary user can run.

Plan 046 keeps every Plan 044/045 protocol correction intact and replaces
only the topology leg: the primary mixed-router evidence lane must run inside
a process-scoped, rootless sandbox that an ordinary invoking user can create
without `sudo`, host capabilities, setuid helpers, host-visible namespaces,
host-visible veths, or host nftables mutation.

## Decision

The primary NTCP2 mixed-router evidence topology is the **rootless sealed
single-network-namespace** topology. It is identified as:

```text
rootless-sealed-single-netns
```

with privilege model:

```text
unprivileged-userns
```

The legacy dual-network-namespace/veth topology is renamed to:

```text
privileged-dual-netns-veth
```

It is preserved for explicit optional qualification only and is never an
automatic fallback.

### Topology shape

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

Both routers execute inside the same sealed network namespace but use distinct
exact bind addresses and independent state directories. The topology is
sufficient for the primary protocol compatibility proof because it exercises:

- real TCP sockets;
- exact local and peer address binding;
- RouterInfo address and key validation;
- NTCP2 obfuscation and Noise handshakes;
- authenticated link promotion;
- encrypted frame write and read paths;
- directional I2NP send/receive behavior;
- process lifecycle, deadlines, cancellation, and cleanup.

### What the topology does not prove

The single-network-namespace topology does not prove separate
network-stack behavior, asymmetric firewall semantics, packet loss, route
mutation, or interface-failure behavior. Those remain optional qualification
work for the existing privileged dual-namespace backend or a future
process-held rootless dual-namespace supervisor. The retained evidence claim
is intentionally narrow:

> The pinned i2pr and reference-router processes completed the declared
> NTCP2 direction inside a process-scoped, rootless user/network namespace
> whose canonical isolation checks passed and whose creation and teardown
> did not alter the parent host's canonical network state.

The claim is never extended to two isolated network stacks, host-route
isolation, or interface-failure semantics.

### Privilege boundary

The rootless lane is defined by what it does not do:

1. No `sudo`, `setcap`, setuid helper, file capability, ambient host
   capability, `--privileged` container, `--network host` container, or
   privileged sidecar is permitted in any rootless code path.
2. No `ip netns add`, no entry under `/run/netns`, no host link mutation,
   no host route mutation, and no host nftables mutation is permitted in any
   rootless code path.
3. `CAP_NET_ADMIN` is never granted to Python, `ip`, the reference routers,
   `i2pr-interop`, or the test harness in the initial host user namespace.
4. Capabilities held as UID 0 inside the newly created user namespace are
   permitted only when the UID/GID map is a single invoking-user mapping
   and the owned network namespace is newly created by that user namespace.
5. The sandbox sets and verifies `no_new_privs` before starting either
   router.
6. The rootless lane does not install system packages. It verifies
   dependencies and emits a typed blocker when the environment is
   incomplete.
7. Preparation and execution remain separate. Pinned reference source
   acquisition and cache construction may use the network; scenario
   execution is offline.
8. Public I2P reseed, discovery, RouterInfo publication, transit, tunnels,
   proxy services, SAM exposure outside the sandbox, I2CP exposure outside
   the sandbox, console exposure, and SSU2 remain prohibited.
9. Cleanup failure overrides protocol success.
10. Rootless capability failure is a blocker, not a skipped success.
11. The harness does not fall back to the privileged backend unless the
    operator explicitly selects that backend in a separate command or
    workflow.

### Single-ID UID/GID mapping requirement

The user namespace's UID map must be exactly one inside UID mapped to the
invoking host UID. The GID map must be exactly one inside GID mapped to the
invoking host GID. `setgroups` must be denied. Broader maps or missing
denials are typed blockers; they are not silently accepted.

### Relationship to the privileged topology

The privileged dual-network-namespace/veth topology remains available for
explicit later qualification work (for example, separate-stack behavior,
asymmetric firewall semantics, packet-loss qualification). It is not the
default evidence lane, it is not invoked by the rootless workflow, and it
is never a silent fallback. Any future rootless dual-namespace supervisor is
a separate plan.

### Evidence-schema implications

The active mixed-router evidence schema is extended in a versioned manner
to record:

- `topology_kind`
- `privilege_model`
- `sandbox_attestation_sha256`
- `parent_network_state_unchanged`

A passed mixed-router record must require the topology kind to be
`rootless-sealed-single-netns`, the privilege model to be
`unprivileged-userns`, the parent state to be unchanged, and the sandbox
attestation digest to be a non-zero SHA-256. The aggregate manifest must
verify that all four handshake-smoke scenario records reference the same
gate attestation, that the attestation exists and validates, that the
attestation commit matches the scenario-record commit, that the attestation
topology and privilege model match the scenario records, and that cleanup
passed in both the scenario records and the attestation.

### Host compatibility and typed blockers

The rootless dependency contract verifies the tools required by the lane
(Python, Rust toolchain, Java build tools, C/C++ build tools, `unshare`
feature support, `ip` support inside the user-owned network namespace,
`/proc` availability, sufficient writable disk, UTF-8 locale, and
repository/cache ownership by the invoking user) but does not install
packages.

The rootless capability probe emits a strict typed status. Allowed outcomes
include `rootless_sandbox_available` and any of `blocked_unprivileged_user_namespace`,
`blocked_uid_map`, `blocked_gid_map`, `blocked_setgroups_contract`,
`blocked_network_namespace`, `blocked_namespace_local_net_admin`,
`blocked_mount_namespace`, `blocked_private_proc`, `blocked_no_new_privs`,
`blocked_loopback_configuration`, `blocked_synthetic_address_configuration`,
`blocked_external_route_present`, `blocked_external_connect_possible`, and
`blocked_rootless_cleanup`. The probe must not collapse distinct capability
failures into a generic host failure. The workflow and local runner must
stop before reference construction when the probe does not return
`rootless_sandbox_available`.

A future option for a process-held rootless dual-namespace supervisor is
allowed but is not part of this decision.

## Consequences

- The primary mixed-router evidence lane is runnable as an ordinary user on a
  host that allows unprivileged user namespaces, without passwordless sudo
  and without host network mutation.
- The privileged topology remains explicit and opt-in. Privileged execution
  is not automatically exposed to forked or untrusted pull requests.
- Sandbox attestation records and parent-network state equivalence make
  isolation failures visible and reviewed, not hidden.
- NTCP2 remains experimental and non-advertised. Milestone 3 remains open
  until a separate evidence review against `plans/000-mvp-roadmap.md`,
  `plans/030-milestone-3-overview.md`, and `specs/CONFORMANCE.md` is
  completed.
- The retained evidence claim is narrower than the privileged topology's
  claim: the rootless lane proves protocol compatibility, not separate-stack
  network behavior.

## Rejected alternatives

- Public-network or shared-host execution: it violates the Plan 038 harness
  boundary and cannot prove that peer discovery or route leakage is absent.
- Docker/Podman/systemd as a first dependency: it obscures Linux namespace
  and process ownership and is outside the narrow Ubuntu host contract.
- `unshare --net` without a user namespace: it requires root in the initial
  user namespace and reproduces the privilege requirement Plan 046 removes.
- A privileged fallback gated only on a missing dependency: it lets a
  host without unprivileged user namespaces silently fall through to a
  topology that mutates the parent host, which is exactly the regression
  Plan 046 prevents.
- A rootless topology that pretends to be two isolated network stacks: it
  would overstate the evidence claim and misrepresent the structural
  isolation guarantee.
