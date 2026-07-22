---
name: i2pr-multipass-recovery
description: Operate, diagnose, or extend the Plan 048/049/050 Multipass recovery lane for NTCP2 interoperability evidence, including atomic lifecycle reservation, cloud-init taxonomy, base verification, the four Plan 045 directions, sanitized export, selective-purge remediation, and the Plan 051 dispatch-gate troubleshooting bridge. Use when an agent is asked to create, adopt, resume, recreate, or destroy a Multipass guest, run the evidence lane, classify a cloud-init failure, or troubleshoot host-side Plan 046 blockers inside a disposable Ubuntu 24.04 amd64 guest.
---

# I2PR Multipass Recovery (Plan 048/049/050/051)

Use this skill from the repository root for the disposable Multipass recovery
lane. **This is the Plan 046 follow-up lane for hosts that emit a typed
host-side blocker.** On a host with Multipass, this lane uses a disposable
Ubuntu 24.04 amd64 guest whose kernel policy and unprivileged user namespace
permission are configured to be permissive, so the Plan 046 sandbox probe
returns `rootless_sandbox_available` and the four Plan 045 mixed directions
can be executed. The host's AppArmor and user-namespace policy are **never**
changed; if they block the guest launch or the rootless probe, the blocker is
recorded and the dispatcher stops.

Read `AGENTS.md`, `plans/048-multipass-permissive-rootless-evidence-environment.md`,
`plans/049-multipass-lifecycle-ownership-corrective-pass.md`,
`plans/050-multipass-cloud-init-recovery.md`,
`plans/051-external-validation-troubleshooting.md`, the reviewed environment
manifest at `scripts/interop/multipass/environment.toml`, and the relevant
`docs/adr/` records before changing anything in this lane.

The canonical reference identifiers inside the guest are still `java_i2p` and
`i2pd`, with locked source commits
`2800040deee9bb376567b671ef2e9c34cf3e30b6` (Java I2P) and
`f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e` (i2pd). Abbreviated revisions are
not valid cache or evidence inputs.

## Reviewed environment contract

The manifest at `scripts/interop/multipass/environment.toml` is the source of
truth. Current defaults:

```text
environment_id              = "i2pr-plan048-rootless-v1"   # stable reviewed ID, not an instance name
image                        = "24.04"
cpus                         = 2
memory                       = "2G"
disk                         = "30G"
launch_timeout_seconds       = 1800
guest_admin_user             = "ubuntu"
guest_execution_user         = "i2ptest"
guest_repo_root              = "/home/i2ptest/i2pr"
guest_cache_root             = "/home/i2ptest/i2pr/target/interop/cache"
guest_evidence_root          = "/home/i2ptest/i2pr/target/interop/evidence"
required_architecture        = "x86_64"
required_os_id               = "ubuntu"
required_os_version          = "24.04"
required_rust_toolchain      = "1.95.0"
required_topology_kind       = "rootless-sealed-single-netns"
required_privilege_model     = "unprivileged-userns"
```

The `environment_id` is **not** an instance name and must never be reused as
one. Each execution has a separate safe run ID (8–48 lowercase ASCII chars,
digits, hyphens) and a derived concrete instance name (`<run-id>-g<generation>`
by default). The legacy hard-coded `i2pr-interop-rootless` is reserved for
transition consumers only and is never authoritative.

## Authoritative command surface

Run from the repository root. **One operation per invocation.** All commands
acquire the per-instance lifecycle lock at
`target/interop/multipass/state/.instance-locks/<instance-name>.lock` and
serialize state transitions.

### Canonical happy path

```text
bash scripts/interop/multipass/run-evidence-lane.sh --all
bash scripts/interop/multipass/run-evidence-lane.sh --all \
    --run-id <safe-id> --destroy-after-export
```

`--all` is interruption-safe. It chains `lifecycle reservation → cloud-init/
provisioning → guest ownership + policy verification → early guest rootless
probe → exact source archive transfer → verified cache transfer → source/
cache snapshot → guest-only offline transition → final guest probe → four
fixed mixed directions → validation → sanitized export → optional
destroy-owned`. The default `--all` keeps the guest on success; pass
`--destroy-after-export` to also destroy at the end of `export`.

### Read-only inspection and explicit recovery

```text
bash scripts/interop/multipass/run-evidence-lane.sh --inspect --run-id <safe-id>
```

`--inspect` is read-only and prints a sanitized JSON record with lifecycle
state, normalized Multipass state, ownership/contract status, generation,
source/cache readiness, host and guest probe outcomes, export status, and a
recommended next operation. It **never** prints ownership tokens, private
host paths, raw Multipass auth details, RouterInfo, endpoints, or raw logs.

```text
bash scripts/interop/multipass/run-evidence-lane.sh --all \
    --run-id <safe-id> --resume-owned
bash scripts/interop/multipass/run-evidence-lane.sh --all \
    --run-id <safe-id> --adopt-owned
bash scripts/interop/multipass/run-evidence-lane.sh --all \
    --run-id <safe-id> --recreate-owned
bash scripts/interop/multipass/run-evidence-lane.sh --destroy-owned \
    --run-id <safe-id>
bash scripts/interop/multipass/run-evidence-lane.sh --all \
    --run-id <safe-id> --keep-on-blocker
```

`--resume-owned`, `--adopt-owned`, `--recreate-owned`, and `--destroy-owned`
all require a complete ownership proof: matching host lifecycle state, stable
environment ID, run ID, generation, guest contract, ownership-token hash,
contract digests (environment, cloud-init, source, cache), and the expected
guest policy. **A name match alone is never ownership.** Stale or unrelated
instances are inspected and left untouched. Recreation is only allowed for a
proven-owned instance whose passing evidence has been exported; it never
issues global `multipass purge`; an unavailable selective purge is a typed
blocker requiring operator action.

### Staged lower-level operations

For a stepwise manual run:

```text
bash scripts/interop/multipass/create.sh                --run-id <safe-id>
bash scripts/interop/multipass/run-evidence-lane.sh     --prepare       --run-id <safe-id>
bash scripts/interop/multipass/snapshot.sh              --name source-and-cache-ready --run-id <safe-id>
bash scripts/interop/multipass/run-evidence-lane.sh     --probe         --run-id <safe-id>
bash scripts/interop/multipass/run-evidence-lane.sh     --run           --run-id <safe-id>
bash scripts/interop/multipass/run-evidence-lane.sh     --export        --run-id <safe-id>
bash scripts/interop/multipass/run-evidence-lane.sh     --destroy       --run-id <safe-id>
```

The created instance is named after `<run-id>-g<generation>`. Snapshots are
limited to `provisioned` and `source-and-cache-ready` and are bound to the
generation + environment manifest sha256.

### Cloud-init recovery and base verification (Plan 050)

```text
bash scripts/interop/multipass/cloud-init-status.sh --instance-name <name>
bash scripts/interop/multipass/verify-base.sh \
    --run-id <safe-id> --instance-name <name> \
    --output <evidence-output.json>
bash scripts/interop/multipass/run-evidence-lane.sh \
    --guest-probe-only --run-id <safe-id>
```

`verify-base.sh` runs `/usr/local/sbin/i2pr-multipass-verify-base` via
`multipass exec`, parses the JSON, writes a sanitized `multipass-base-verify`
record, and verifies the ownership contract file ownership/mode before any
router work. `cloud-init-status.sh` runs `cloud-init status --long`, captures
the four canonical services plus the boot-finished marker, classifies via
`scripts/interop/multipass/cloud_init_status.py`, and emits sanitized JSON.
`--guest-probe-only` runs create-adopt + cloud-init-status + verify-base +
probe and emits a single `multipass-guest-probe-only` record. The flag is
mutually exclusive with `--create`, `--prepare`, `--probe`, `--run`,
`--export`, `--all`, `--inspect`, `--adopt-owned`, `--resume-owned`,
`--recreate-owned`, and `--destroy-owned`, and forbids router launch, cache
transfer, and `run-matrix.sh` execution.

### Selective-purge remediation (Plan 050)

```text
bash scripts/interop/multipass/selective-purge.sh \
    --run-id <safe-id> --instance-name <name>
```

Confirms the instance is in `Deleted` state and the ownership contract matches
`environment_manifest_sha256` before any `multipass purge <instance>`
(per-instance). Unowned collisions, unsupported client versions, or missing
manifests return typed blockers (`ownership_not_proven`,
`selective_purge_not_supported`, `resource_already_absent`) without mutating
global Multipass state.

### Plan 051 dispatch-gate troubleshooting

The Plan 051 entrypoint lets an agent run the canonical Plan 040/043 gate
scripts inside an owned Multipass guest via `multipass exec`. It does **not**
advertise NTCP2 support, does not satisfy Plan 045 directional predicates, and
does not close Milestone 3 by itself. Profiles:

```text
bash scripts/interop/multipass/dispatch-gate.sh \
    --profile environment-smoke \
    --run-id <safe-id> --instance-name <name>
bash scripts/interop/multipass/dispatch-gate.sh \
    --profile reference-crosscheck-ipv4 \
    --run-id <safe-id> --instance-name <name>
bash scripts/interop/multipass/dispatch-gate.sh \
    --profile handshake-smoke \
    --run-id <safe-id> --instance-name <name>
bash scripts/interop/multipass/dispatch-gate.sh \
    --profile handshake-smoke-rootless \
    --run-id <safe-id> --instance-name <name>
bash scripts/interop/multipass/dispatch-gate.sh \
    --profile full \
    --run-id <safe-id> --instance-name <name>
bash scripts/interop/multipass/dispatch-gate.sh \
    --profile evidence-validation \
    --run-id <safe-id> --instance-name <name>
bash scripts/interop/multipass/dispatch-gate.sh \
    --profile cleanup-verification \
    --run-id <safe-id> --instance-name <name>
```

Each step's stdout/stderr is captured to
`target/interop/multipass/state/<run-id>/<profile>-<step>.log` and the lane
returns the failing exit status. `--online` opts back into network-enabled
behavior inside the guest; the default is offline.

## Script responsibilities

| Script | Responsibility |
|---|---|
| `scripts/interop/multipass/environment.toml` | Reviewed environment contract. Source of truth for environment ID, image, sizes, paths, required toolchain, and required topology. |
| `scripts/interop/multipass/cloud-init.yaml` | Base cloud-init unit. Installs only the declared system packages, writes `provisioning.json`, drops a `base-packages.complete` phase marker, applies the two permissive sysctls (`apparmor_restrict_unprivileged_userns=0`, `unprivileged_userns_clone=1`), creates `i2ptest` with **no** sudo / device / Docker / LXD / ambient capabilities, and exposes `/usr/local/sbin/i2pr-multipass-verify-base`. Must **not** install `rustup` or any host toolchain. |
| `scripts/interop/multipass/common.sh` | Shared bash helpers: manifest read, lifecycle lock, typed blocker formatter, environment-blocker JSON writer. Sources `multipass`, `findmnt`, `sha256sum`, `mktemp` etc. |
| `scripts/interop/multipass/host_state.py` | Reads host Multipass state, runs the host rootless probe, normalizes outcome. |
| `scripts/interop/multipass/lifecycle.py` | Atomic lifecycle reservation, run-id derivation, instance-name derivation, ownership/contract verification, state transitions. |
| `scripts/interop/multipass/sidecars.py` | Build-sidecar manifest for cache/source. |
| `scripts/interop/multipass/records.py` | Schema for `multipass-guest-probe-only`, `multipass-base-verify`, `multipass-environment-blocker`, `multipass-directional-record`. |
| `scripts/interop/multipass/aggregate.py` | Aggregate-evidence builder for the lane. |
| `scripts/interop/multipass/collect.py` | Sanitized bundle collector. |
| `scripts/interop/multipass/export.py` | Atomic sanitized export. |
| `scripts/interop/multipass/create.sh` | Reserves lifecycle state, derives a fresh collision-resistant name, polls `multipass info --format json` + cloud-init completion marker. Never invokes a non-existent `multipass wait-ready`. |
| `scripts/interop/multipass/destroy.sh` | Per-instance selective purge gated on ownership proof. Never invokes global `multipass purge`. |
| `scripts/interop/multipass/prepare-offline.sh` | Applies the guest nftables output policy (deny non-loopback egress). Idempotent. |
| `scripts/interop/multipass/probe.sh` | Guest-side rootless probe. Emits `rootless_sandbox_available` or a typed blocker. |
| `scripts/interop/multipass/run-evidence-lane.sh` | Top-level orchestrator. Validates environment manifest, run ID, instance name, generation, lifecycle state, and dispatches lifecycle operations. |
| `scripts/interop/multipass/run-direction.sh` | Runs a single Plan 045 mixed direction inside the guest. |
| `scripts/interop/multipass/run-matrix.sh` | Convenience wrapper that calls `run-direction.sh` for all four Plan 045 directions. |
| `scripts/interop/multipass/snapshot.sh` | Generates/stages `provisioned` and `source-and-cache-ready` snapshots. |
| `scripts/interop/multipass/status.sh` | Reads the latest guest probe, ownership, contract, and offline state for `--inspect`. |
| `scripts/interop/multipass/transfer-source.sh` | Exact git archive → sha256 → extract → `source_tree.py` manifest → verify pipeline. Requires `I2PR_MULTIPASS_INSTANCE_NAME` and `I2PR_MULTIPASS_RUN_ID` env vars. |
| `scripts/interop/multipass/transfer-cache.sh` | Re-transfers the canonical `target/interop/cache` after every source transfer (which excludes `target/`). |
| `scripts/interop/multipass/source_tree.py` | Manifest generator/verifier (`schema, commit, archive_sha256, tree_sha256, archive_format`). |
| `scripts/interop/multipass/cloud-init-status.sh` | Captures `cloud-init status --long` + 4 services + boot-finished marker. |
| `scripts/interop/multipass/cloud_init_status.py` | Cloud-init failure taxonomy classifier (`retry_safe`, `recommended_action`). |
| `scripts/interop/multipass/verify-base.sh` | Runs the in-guest `/usr/local/sbin/i2pr-multipass-verify-base` and verifies ownership file mode/owner. |
| `scripts/interop/multipass/verify-clean-host.sh` | Sanitized host comparison before/after the lane. |
| `scripts/interop/multipass/selective-purge.sh` | Ownership-gated single-instance `multipass purge` (when supported). |
| `scripts/interop/multipass/dispatch-gate.sh` | Plan 051 troubleshooting bridge. Dispatches canonical Plan 040/043 profiles inside the guest via `multipass exec`. |
| `scripts/interop/multipass/restore.sh` | Restores a `source-and-cache-ready` snapshot before the matrix. |
| `scripts/interop/multipass/export-evidence.sh` | Validates the sanitized bundle and atomically installs it under `target/interop/evidence/multipass/<run-id>/`. |

## Static boundary checker

`scripts/check-multipass-interop-boundary.sh` enforces the canonical manifest,
the sanitized cloud-init taxonomy, the phase markers, the absence of `rustup`
in `cloud-init.yaml`, the absence of `eval` in lifecycle scripts, and the
absence of any global `multipass purge` form in normal paths. Run it after
**any** edit to anything under `scripts/interop/multipass/`,
`tests/integration/ntcp2/harness/{rootless_topology,interop_topology,
rootless_supervisor,rootless_inner_runner}.py`, or
`.github/workflows/ntcp2-interop-rootless.yml`.

## Test surface

The Multipass layer has dedicated unit tests at
`tests/integration/ntcp2/harness/test_multipass.py`. They exercise the
lifecycle/ownership contract, the sanitized records, the typed blocker
taxonomy, the snapshot binding, and the export pipeline using a fake
`multipass` executable. The normal suite must stay green:

```text
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_multipass.py'
bash scripts/check-multipass-interop-boundary.sh
```

A missing Multipass daemon, a guest policy mismatch, a failed rootless probe,
an offline-enforcement failure, a cache/source mismatch, a cleanup failure, or
an evidence-validation failure is a typed blocker. **Never** convert a typed
blocker or a reference-only result into a passing evidence record or a
support-row status. Plan 049 / 050 do not advance `specs/support.toml` or
close Milestone 3.

## Safety and development rules

- Never change the host's AppArmor, user-namespace policy, sysctls, or
  capabilities. The host may be in `apparmor_restrict_on` as a negative
  baseline.
- Never use a host mount as authoritative source, cache, or evidence inside
  the guest. Source and cache always move through verified archives.
- Never accept an arbitrary guest command. Every executor goes through
  `multipass exec` with explicit allowlisted arguments.
- Never silently fall back to the privileged topology (`privileged-dual-netns-veth`)
  or to a global `multipass purge`. Both are typed blockers.
- Never reuse the legacy `i2pr-interop-rootless` instance name outside the
  reviewable `legacy-plan048` run ID.
- The guest execution user `i2ptest` has **no** sudo and **no** ambient
  capabilities. Direct commands to `multipass exec` from the launcher's
  orchestrator user; do not promote `i2ptest` to obtain capabilities.
- Add negative-path unit tests for any new lifecycle, contract, or evidence
  state.
- Before handoff: `bash scripts/check-multipass-interop-boundary.sh`,
  `python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_multipass.py'`,
  `cargo fmt --all --check`, `cargo check --workspace --all-targets`,
  `bash scripts/check-dependency-direction.sh`,
  `bash scripts/check-runtime-boundaries.sh`.
- For the host-side sandbox lane, hand off to `i2pr-rootless-sandbox`. For
  the canonical harness workflow (host prep, references, scenarios, evidence,
  cleanup), hand off to `i2pr-ntcp2-interop`.

## Reference

Consult [script-catalog.md](references/script-catalog.md) for the per-script
script + python module dependency graph and the lifecycle state machine.
