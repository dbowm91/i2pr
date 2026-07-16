# Plan 048 Multipass recovery environment

This directory is the disposable recovery lane for the Plan 046 rootless
sealed-namespace evidence harness. It is not a production router setup and it
does not change the invoking host's AppArmor or user-namespace policy.

The single source of truth is `environment.toml`: Ubuntu 24.04 amd64,
`i2pr-interop-rootless`, 4 CPUs, 8 GiB RAM, 40 GiB disk, `/home/i2ptest` as
the ordinary execution boundary, and `target/interop/cache` as the canonical
reference-cache root. Source and cache are transferred as immutable archives;
host mounts are not authoritative inputs.

Preparation is the only network-enabled phase. Cloud-init installs the locked
build dependencies, applies the two permissive sysctls inside the guest, and
creates `i2ptest` without sudo, device, Docker, LXD, or ambient capabilities.
After `prepare-offline.sh`, a guest nftables output policy denies non-loopback
egress. The rootless probe runs before every direction and a failed probe is a
typed blocker. The four directions run in fixed order:

```text
bash scripts/interop/multipass/run-evidence-lane.sh --all
bash scripts/interop/multipass/run-evidence-lane.sh --all \
  --run-id plan048-example --destroy-after-export
```

The first command deliberately leaves the VM for inspection. Destruction is
explicit because exported evidence must survive it:

```text
bash scripts/interop/multipass/export-evidence.sh --run-id <run-id>
bash scripts/interop/multipass/destroy.sh
```

The lower-level lifecycle is also available when a staged run is needed:

```text
bash scripts/interop/multipass/create.sh
bash scripts/interop/multipass/run-evidence-lane.sh --prepare
bash scripts/interop/multipass/snapshot.sh --name source-and-cache-ready
bash scripts/interop/multipass/run-evidence-lane.sh --probe
bash scripts/interop/multipass/run-evidence-lane.sh --run
bash scripts/interop/multipass/run-evidence-lane.sh --export --run-id <run-id>
```

Creation polls the supported `multipass info --format json` state and the
cloud-init completion marker with the manifest timeout. The installed
Multipass client does not expose a `wait-ready` command, so readiness is not
delegated to an unavailable subcommand.

Lifecycle operations are narrow and reject unknown instance, snapshot,
scenario, path, and command arguments. `verify-clean-host.sh` records and
compares only sanitized Multipass/host-policy state. The host-side evidence
directory is never inside the VM and is preserved by `destroy.sh`.

The normal unit tests use a fake `multipass` executable. Real Multipass is an
opt-in external lane and must complete source/cache verification, both rootless
probes, offline enforcement, all four sanitized Plan 045 records, aggregate
validation, atomic export, clean destruction, a fresh rebuild, and snapshot
restore before it can contribute evidence. A blocked or reference-only run is
not NTCP2 support evidence.
