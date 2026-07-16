# ADR 0018: Multipass lifecycle-owned permissive rootless interoperability environment

- Status: Accepted
- Date: 2026-07-16
- Plans: 046, 047, 048, 049

## Context

The current host is the deliberate Plan 046 negative baseline: it permits
unprivileged user namespaces at the sysctl level but has
`kernel.apparmor_restrict_unprivileged_userns=1`, producing the typed blocker
`blocked_unprivileged_user_namespace`. Changing that host-wide policy would
alter unrelated work and violate Plan 046's ordinary-user boundary.

## Decision

Use a disposable Ubuntu 24.04 amd64 Multipass VM as the Plan 048 recovery
environment. The reviewed environment contract has a stable environment ID
(`i2pr-plan048-rootless-v1` for the current manifest), 4 CPUs, 8 GiB RAM, 40
GiB disk, and the checked-in `cloud-init.yaml`. A run ID and a concrete
instance name are separate, ephemeral identifiers. The default instance name
is collision-resistant and generation-bound; the legacy
`i2pr-interop-rootless` name is not an authoritative resource.

Cloud-init changes only the guest's `kernel.unprivileged_userns_clone=1` and
`kernel.apparmor_restrict_unprivileged_userns=0`; it does not disable AppArmor
globally or change the invoking host.

Provisioning runs through the guest administrative account. The evidence lane
runs as `i2ptest`, whose password is locked, home is private, group list has no
sudo/adm/docker/lxd/device access, and all effective/permitted/inheritable/
and ambient capabilities are zero. The execution user never invokes
sudo.

The host never mounts the repository into the VM. An exact clean commit is
transferred as a normalized, content-addressed archive and verified with a
source manifest. The pinned reference cache is transferred from the canonical
host path `target/interop/cache` with its existing metadata and complete-tree
hashes; `target/interop/build` remains sidecar metadata, not a competing cache
root. Preparation may use the network for image/package/toolchain/cache work.
After the guest-only nftables output deny policy is installed, scenario
execution is offline and the rootless probe runs before any router process.

Multipass is selected because its supported lifecycle commands provide named
launch, readiness, transfer, snapshot, restore, stop, and delete operations
without depending on daemon internals. Selective purge is used only when the
installed client can attribute it to the proven-owned deleted instance; global
`multipass purge` is never part of normal lifecycle recovery. Snapshots are
limited to `provisioned` and `source-and-cache-ready`; a guest containing
secret-bearing scenario state is never snapshotted. Rebuild and restore rerun
ownership, contract, policy, process, and probe checks before execution.

### Plan 049 lifecycle ownership amendment

The host creates a versioned lifecycle record atomically before launch under
`target/interop/multipass/state/<run-id>/lifecycle.json`. It records the stable
environment ID, run ID, instance name, generation, validated state, source and
environment digests, ownership-token hash, and typed outcome. A per-run/
per-instance lock serializes state transitions and prevents concurrent adopt,
resume, recreate, or destroy operations.

The guest receives the full random ownership token only through the
root-owned, non-`i2ptest`-writable contract at
`/var/lib/i2pr-interop/ownership-token`; the host retains only its SHA-256.
Ownership requires matching host and guest contracts, token hash, environment
and cloud-init digests, generation, source/cache phase, guest policy, and
execution-user properties. A matching name alone never proves ownership.

The supported recovery operations are explicit: read-only `--inspect`,
`--adopt-owned`, `--resume-owned`, `--recreate-owned`, and `--destroy-owned`.
Normal execution never silently adopts, recreates, destroys, stops, restores,
or purges an existing instance. Unowned, ambiguous, incompatible, or
deleted-but-unpurged instances remain untouched and produce typed blockers.
Resume follows the validated lifecycle state machine; recreation increments the
generation and cannot reuse unexported passing evidence.

The host baseline probe and guest rootless probe are recorded independently.
The host `blocked_unprivileged_user_namespace` outcome is a negative baseline
only; guest scenario execution requires `rootless_sandbox_available` after
provisioning and again immediately before router start. Every environment and
directional record carries run/generation and contract ownership digests. A
pre-router failure writes sanitized environment-blocker evidence, never a
protocol record, and mixed generations or run IDs cannot form a passing
manifest.

Only the sanitized, run- and generation-attributed evidence bundle is transferred back into
`target/interop/evidence/multipass/<run-id>/`. The exporter rejects links,
devices, FIFOs, sockets, hardlink surprises, unexpected names, oversized
files, manifest mismatches, and non-passing directional records before an
atomic rename. Raw RouterInfo, identities, keys, I2NP, endpoints, logs,
private paths, and run roots never cross the export boundary.

## Supported hosts and blockers

The target host needs a working Multipass installation, Ubuntu 24.04 amd64
guest support, and sufficient virtualization resources. Missing Multipass,
unavailable daemon/image, guest policy mismatch, non-zero execution
capabilities, failed rootless probe, inability to enforce offline execution,
cache/source mismatch, or cleanup/evidence validation failure is a typed
blocker. The legacy privileged dual-netns topology is never an automatic
fallback.

## Consequences

Plan 047's `host.apparmor-restrict-on` row remains the negative baseline while
the VM exercises the `host.apparmor-restrict-off` recovery category. Plans 046,
047, and 048 pass predicates remain unchanged. A successful VM run provides
reproducible environment evidence, not automatic NTCP2 support or Milestone 3
closure; the support ledger remains experimental and non-advertised until the
existing conformance requirements are independently satisfied. Plan 049
corrects lifecycle ownership and collision recovery; it does not weaken any
protocol or isolation predicate.
