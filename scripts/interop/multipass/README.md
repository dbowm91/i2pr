# Plan 049 Multipass recovery environment lifecycle

This directory is the disposable recovery lane for the Plan 046 rootless
sealed-namespace evidence harness. It is not a production router setup and it
does not change the invoking host's AppArmor or user-namespace policy.

The reviewed environment contract is identified by a stable environment ID
(`i2pr-plan048-rootless-v1` in the current manifest). It is not a Multipass
instance name. The manifest remains the source of truth for Ubuntu 24.04
amd64, 4 CPUs, 8 GiB RAM, 40 GiB disk, `/home/i2ptest` as the ordinary
execution boundary, and `target/interop/cache` as the canonical reference-cache
root. Source and cache are transferred as immutable archives; host mounts are
not authoritative inputs.

Each execution has a separate safe run ID (8–48 lowercase ASCII characters,
digits, and hyphens) and a concrete instance name derived from that run ID and
its generation. The default path allocates a fresh name; it does not reuse the
legacy `i2pr-interop-rootless` name. The host records the reservation
atomically before launch at
`target/interop/multipass/state/<run-id>/lifecycle.json` and serializes updates
with a run/instance lifecycle lock.

Preparation is the only network-enabled phase. Cloud-init installs the locked
build dependencies, applies the two permissive sysctls inside the guest, and
creates `i2ptest` without sudo, device, Docker, LXD, or ambient capabilities.
After `prepare-offline.sh`, a guest nftables output policy denies non-loopback
egress.

The host baseline probe and guest rootless probe are separate results. A host
`blocked_unprivileged_user_namespace` is retained as a negative baseline but
does not gate guest launch. The guest probe must return
`rootless_sandbox_available` both after ownership verification and immediately
before routers start. A failed guest probe is a typed blocker. The four
directions run in fixed order:

```text
bash scripts/interop/multipass/run-evidence-lane.sh --all
bash scripts/interop/multipass/run-evidence-lane.sh --all \
  --run-id plan049-example --destroy-after-export
```

The one-command path is interruption-safe and does not implicitly adopt,
recreate, stop, delete, purge, or destroy an existing instance. Use explicit
operations for an owned lifecycle:

```text
bash scripts/interop/multipass/run-evidence-lane.sh --inspect --run-id <run-id>
bash scripts/interop/multipass/run-evidence-lane.sh --all --run-id <run-id>
bash scripts/interop/multipass/run-evidence-lane.sh --all --resume-owned --run-id <run-id>
bash scripts/interop/multipass/run-evidence-lane.sh --all --adopt-owned --run-id <run-id>
bash scripts/interop/multipass/run-evidence-lane.sh --all --recreate-owned --run-id <run-id>
bash scripts/interop/multipass/run-evidence-lane.sh --destroy-owned --run-id <run-id>
bash scripts/interop/multipass/run-evidence-lane.sh --all --keep-on-blocker --run-id <run-id>
```

`--adopt-owned`, `--resume-owned`, `--recreate-owned`, and `--destroy-owned`
require a complete ownership proof: matching host lifecycle state, stable
environment ID, run ID, generation, guest contract, ownership-token hash,
contract digests, and the expected guest policy. A name match alone is never
ownership. A stale or unrelated instance is inspected and left untouched.
Recreation is allowed only for a proven-owned instance, after passing evidence
has been exported, and never uses global `multipass purge`; an unavailable
selective purge is a typed blocker requiring operator action.

The read-only inspection output is sanitized. It reports lifecycle state,
normalized Multipass state, ownership/contract status, generation, source/cache
readiness, host and guest probe outcomes, export status, and the recommended
next operation. It never prints ownership tokens, private host paths, raw
Multipass authentication details, RouterInfo, endpoints, or raw logs.

The lower-level lifecycle is also available when a staged run is needed:

```text
bash scripts/interop/multipass/create.sh --run-id <run-id>
bash scripts/interop/multipass/run-evidence-lane.sh --prepare
bash scripts/interop/multipass/snapshot.sh --name source-and-cache-ready
bash scripts/interop/multipass/run-evidence-lane.sh --probe
bash scripts/interop/multipass/run-evidence-lane.sh --run
bash scripts/interop/multipass/run-evidence-lane.sh --export --run-id <run-id>
```

Creation reserves and persists lifecycle state before calling Multipass, derives
a collision-resistant bounded instance name, and polls structured
`multipass info --format json` state plus the cloud-init completion marker with
the manifest timeout. Unknown or deleted-but-unpurged states are blockers, not
implicit cleanup requests. The installed Multipass client does not expose a
`wait-ready` command, so readiness is not delegated to an unavailable
subcommand.

Lifecycle operations are narrow and reject unknown instance, snapshot,
scenario, path, and command arguments. `verify-clean-host.sh` records and
compares only sanitized Multipass/host-policy state. Snapshots are generation-
and contract-bound and limited to `provisioned` and `source-and-cache-ready`.
The host-side evidence directory is never inside the VM and is preserved by
owned destruction.

Every lifecycle and directional record carries the stable environment ID, run
ID, instance generation, instance-name digest, lifecycle schema version,
ownership/environment/cloud-init digests, separate host-baseline and guest
probe outcomes, and the environment evidence hash. A pre-router blocker writes
sanitized `environment-blocker.json`; it cannot satisfy protocol conformance,
and mixed run IDs or generations cannot form one passing manifest.

The normal unit tests use a fake `multipass` executable. Real Multipass is an
opt-in external lane and must complete source/cache verification, both rootless
probes, offline enforcement, all four sanitized Plan 045 records, aggregate
validation, atomic export, clean destruction, a fresh rebuild, and snapshot
restore before it can contribute evidence. A blocked or reference-only run is
not NTCP2 support evidence.
