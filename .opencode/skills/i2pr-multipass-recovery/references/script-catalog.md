# Plan 048/049/050 Multipass script catalog

This is the per-script dependency graph and the lifecycle state machine for
the Multipass recovery lane. The authoritative source of the lane is
`scripts/interop/multipass/`. Each entry lists the inputs it consumes, the
records it writes, and which scripts may call it.

## Lifecycle state machine

`scripts/interop/multipass/lifecycle.py` owns the state transitions:

```text
reserved
  └─→ launching
        └─→ provisioning
              └─→ provisioned
                    ├─→ source_ready
                    ├─→ cache_ready
                    └─→ source_and_cache_ready
                          └─→ probe_passed
                                └─→ offline_ready
                                      └─→ running
                                            ├─→ exporting
                                            │     └─→ exported
                                            ├─→ stopped
                                            └─→ blocked
  blocked          (typed blocker recorded)
  abandoned        (operator stop recorded)
destroying
  └─→ destroyed
        └─→ reserved  (only on a verified-owned recreation)
```

Any path that is not in the diagram above is rejected by
`lifecycle.py:transition`. Every transition is gated by an ownership proof
(`lifecycle.py:verify_ownership`) and serialized through the per-instance
lock at `target/interop/multipass/state/.instance-locks/<instance-name>.lock`.

The terminal states for a successful lane are `exported` (evidence written)
or `exported` then `destroyed` (with `--destroy-after-export`). The terminal
states for a failed lane are `blocked` (typed blocker recorded) or
`abandoned` (operator stop recorded). A pre-router failure becomes a sanitized
`environment-blocker.json` under `target/interop/evidence/multipass/<run-id>/`
and **cannot** become a protocol record.

## Reviewed environment

```text
environment_id    = "i2pr-plan048-rootless-v1"     # plans/048 reviewed contract, not an instance name
image             = "24.04"
cpus              = 2
memory            = "2G"
disk              = "30G"
guest_admin_user  = "ubuntu"
guest_exec_user   = "i2ptest"
guest_repo_root   = "/home/i2ptest/i2pr"
```

Per-run state lives at:

```text
target/interop/multipass/state/<run-id>/lifecycle.json
target/interop/multipass/state/<run-id>/<phase>.log
target/interop/multipass/state/<run-id>/snapshot-<name>.json
target/interop/multipass/state/.instance-locks/<instance-name>.lock
```

Evidence is installed at:

```text
target/interop/evidence/multipass/<run-id>/environment-blocker.json   (pre-router failure)
target/interop/evidence/multipass/<run-id>/multipass-base-verify.json
target/interop/evidence/multipass/<run-id>/multipass-guest-probe-only.json
target/interop/evidence/multipass/<run-id>/mixed-<ts>-<n>-<hash>-<ref>.json
```

## Script ↔ Python module dependencies

| Script | Python helpers | Records written | Reads |
|---|---|---|---|
| `create.sh` | `lifecycle.py`, `config.py`, `host_state.py` | `lifecycle.json` | `environment.toml`, current `git rev-parse HEAD` |
| `prepare-offline.sh` | `lifecycle.py` | (mutates `lifecycle.json`) | guest nftables via `multipass exec` |
| `probe.sh` | `host_state.py` | (writes probe outcome into `lifecycle.json`) | `multipass exec` for in-guest probe |
| `snapshot.sh` | `lifecycle.py` | `snapshot-<name>.json` | `multipass snapshot` |
| `transfer-source.sh` | `source_tree.py`, `lifecycle.py` | source manifest sidecar | `git archive`, `sha256sum` |
| `transfer-cache.sh` | `lifecycle.py`, `config.py` | cache manifest sidecar | `target/interop/cache` |
| `verify-clean-host.sh` | `host_state.py` | `host-baseline.json` or comparison | `multipass list --format json`, `nft list ruleset` |
| `verify-base.sh` | `cloud_init_status.py` | `multipass-base-verify.json` | in-guest `/usr/local/sbin/i2pr-multipass-verify-base` output |
| `selective-purge.sh` | `lifecycle.py`, `host_state.py` | (mutates `lifecycle.json`) | ownership contract, `multipass list --format json` |
| `destroy.sh` | `lifecycle.py`, `host_state.py` | (mutates `lifecycle.json`) | ownership contract |
| `restore.sh` | `lifecycle.py` | (mutates `lifecycle.json`) | `multipass restore` |
| `run-direction.sh` | — | one Plan 045 mixed record per direction | guest-side `dispatch-gate.sh` |
| `run-matrix.sh` | — | four Plan 045 mixed records (one per direction) | `run-direction.sh` × 4 |
| `run-evidence-lane.sh` | `lifecycle.py`, `config.py`, `host_state.py`, `cloud_init_status.py` | depends on the operation chain | every script listed above |
| `cloud-init-status.sh` | `cloud_init_status.py` | (writes JSON snapshot) | `multipass exec cloud-init status --long` |
| `dispatch-gate.sh` | (uses guest-side harness) | `<profile>-<step>.log` | `multipass exec` |
| `dispatch-gate.sh` | (guest-side) `build-references.sh`, `cache-manifest.py`, `run-gate.sh`, `validate-evidence.py`, `aggregate-evidence.py`, `cleanup.sh`, `verify-clean-host.sh` | per-profile logs | `target/interop/cache`, `references.lock.toml` |
| `export-evidence.sh` | `export.py`, `records.py` | sanitized bundle in `target/interop/evidence/multipass/<run-id>/` | `target/interop/multipass/state/<run-id>/` |
| `status.sh` | `lifecycle.py`, `host_state.py` | (prints sanitized JSON) | `lifecycle.json` |

### Python module ownership

| Module | Owns |
|---|---|
| `config.py` | Reads `environment.toml`; surfaces a single key/value or the manifest sha256. |
| `lifecycle.py` | Run-ID validation, instance-name derivation, atomic lifecycle JSON, ownership proof, state transitions, **no** Multipass or guest calls. |
| `host_state.py` | Reads `multipass list --format json`, normalizes instance state, runs the host rootless probe. |
| `source_tree.py` | Source-manifest schema generator/verifier. |
| `sidecars.py` | Build-sidecar writer (sources + cache hashes). |
| `records.py` | Canonical record schemas (`multipass-environment-blocker`, `multipass-base-verify`, `multipass-guest-probe-only`, `multipass-directional-record`). |
| `cloud_init_status.py` | Cloud-init failure taxonomy classifier. |
| `aggregate.py` | Aggregate-evidence builder for the Multipass lane. |
| `collect.py` | Sanitized-bundle collector. |
| `export.py` | Atomic sanitized export. |

## Static checks

```text
bash scripts/check-multipass-interop-boundary.sh     # canonical manifest, taxonomy, no rustup, no eval, no global purge
bash scripts/check-rootless-interop-boundary.sh      # rootless-owned file purity
bash scripts/check-ntcp2-interoperability.sh         # aggregate evidence validation
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_multipass.py'
```

Before handoff, the lane must pass **all four**. A failure in any one is a
typed blocker.

## Phase order reference

The `--all` operation chains the following phases in this order:

1. **reserve lifecycle state** (`reserved`) — atomic reservation before launch.
2. **launch / image fetch** (`launching`) — `multipass launch` from manifest image.
3. **cloud-init / provisioning** (`provisioning`) — read
   `cloud-init-status.sh`, classify, write `provisioning.json` + phase marker.
4. **base verify** (`provisioned`) — `verify-base.sh` writes
   `multipass-base-verify.json`.
5. **early guest probe** (`probe_passed`) — `probe.sh` must report
   `rootless_sandbox_available`.
6. **source transfer** (`source_ready`) — `transfer-source.sh` produces an
   exact git archive + sidecar manifest.
7. **cache transfer** (`source_and_cache_ready`) — `transfer-cache.sh`
   re-runs after every source transfer because `target/` is excluded.
8. **snapshot** — name limited to `provisioned` or `source-and-cache-ready`.
9. **offline transition** (`offline_ready`) — `prepare-offline.sh` applies
   the guest nftables output policy.
10. **final guest probe** (`probe_passed`) — re-run after offline; must
    still be `rootless_sandbox_available`.
11. **matrix** (`running`) — `run-matrix.sh` calls `run-direction.sh` for all
    four Plan 045 directions.
12. **aggregate / validate** — `validate-evidence.py`, `aggregate-evidence.py`.
13. **sanitized export** (`exported`) — `export-evidence.sh`.
14. **optional destroy-owned** (`destroyed`) — only on a verified-owned
    instance whose export has succeeded.

A pre-router blocker at any phase (1–10) writes a sanitized
`environment-blocker.json` and exits the lane.
