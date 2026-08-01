# Plan 080: Multipass lane prequalification for Plan 078

## Status and dependencies

- Status: closed (lane-qualified, Plan 078 closed-as-blocked-protocol-defect).
- Parent roadmap: Plan 074.
- Requires: Plans 075 and 076 closed; Plan 077 closed with a documented no-full-runtime-lane result.
- Closure record: `plans/080-status.md`.
- Plan type: environment capability re-provisioning, Multipass guest lifecycle, and full-runtime lane qualification.

Plan 078 is currently `blocked-preflight` because no full-runtime lane was
qualified. Plan 077 closed with `selected_lane = inherited-descriptors-seccomp`,
`scope = reduced-scope-diagnostic`, `qualified = false`,
`full_runtime_lane = unavailable`. The inherited-descriptor lane is explicitly
not full transport-manager qualification and cannot satisfy Plan 078's
preconditions.

## Objective

Promote the Plan 048/049/050 Multipass recovery lane into a qualified
full-runtime lane on the current host, thereby unblocking Plan 078's two
primary i2pd directions. The Multipass guest must provide an isolated execution
environment where real i2pr and real pinned i2pd processes can communicate over
loopback with no public-network reachability, bounded state, and deterministic
cleanup.

This plan does not execute NTCP2 protocol runs. It does not claim any
interoperability result. It prepares the environment so that Plan 078 may begin.

## Known environmental constraints

Treat these as fixed unless a fresh capability probe disproves them:

- AppArmor kernel policy blocks unprivileged user/network namespaces
  (`apparmor_restrict_unprivileged_userns=1`);
- rootless `unshare`, bubblewrap, rootless Podman/Docker, and user-level
  `PrivateNetwork` are not viable;
- Docker CLI is present but the daemon is inaccessible to the `sugarwookie`
  user;
- QEMU system emulation (`qemu-system-x86_64`) is absent;
- Multipass 1.16.3 is installed with the QEMU driver but QEMU itself is
  not installed;
- LXD snap is installed but the daemon is not initialized;
- the runner must not invoke sudo or alter host security policy;
- existing Multipass instances without ownership contracts must not be
  mutated.

## Required lane-selection order

The Plan 074 lane priority remains authoritative:

```text
1. existing accessible rootful Docker daemon, one container, --network none
2. QEMU system emulation using TCG, one guest, -nic none
3. inherited connected TCP descriptors plus no_new_privs/seccomp
4. manually triggered dedicated remote Linux runner or GitHub Actions job
5. typed no-full-runtime-lane blocker
```

On this host, Lanes A and B are unavailable. Lane D (manually triggered
remote runner) is the only full-runtime lane that works without sudo or
host-policy changes. The Plan 048/049/050 Multipass recovery lane is the
canonical implementation of Lane D on this host.

## Preconditions

Before any Plan 080 work, verify:

- Plan 075 is closed (runner integrity and evidence correction).
- Plan 076 is closed (real pinned i2pd library and direct driver
  construction).
- Plan 077 is closed with `full_runtime_lane = unavailable`.
- The working tree is clean (`git diff --check` passes).
- `gh` authentication is not required for any Plan 080 operation.
- The Plan 048/049 Multipass lifecycle scripts are present and the
  boundary checker passes.

## Work packages

### WP1. Read-only inspection of the stale instance

Status: complete.

An unowned existing Multipass instance
`i2pr-interop-plan049-20260721105135-3d7e7a68-g1` was running on the host.
Plan 049 forbids implicit mutation of unowned instances. A read-only
inspection was performed via `run-evidence-lane.sh --inspect`. The inspection
returned `blocked_host_state_without_instance` because the instance has no
host lifecycle record and no ownership contract.

The sanitized evidence record is at:

```text
target/interop/evidence/multipass/plan080-stale-instance-inspection/20260801034110/inspection.json
```

The instance was not adopted, resumed, modified, or destroyed. The
recommended action is to leave the stale instance untouched and launch a
fresh instance with a distinct collision-resistant name.

### WP2. Selective-purge remediation

Status: skipped.

Per Plan 049, selective-purge is only valid when the target instance is in
`deleted-unpurged` state with a matching ownership contract. The stale
instance `i2pr-interop-plan049-20260721105135-3d7e7a68-g1` is in a running
state, not `deleted-unpurged`, and has no ownership contract. Any mutation is
forbidden. Recorded as `skipped-due-to-unowned-running-state`.

No global `multipass purge` was issued. No instance state was changed.

### WP3. Launch a fresh Plan 048/049/050 owned instance

Status: in-progress.

Launch a fresh owned Multipass instance through the canonical
`run-evidence-lane.sh --all` chain with an explicit run ID and
`--keep-on-blocker`. The chain must:

- allocate a fresh collision-resistant instance name distinct from every
  prior name including the stale instance;
- reserve lifecycle state atomically before launch;
- write a host lifecycle record with the ownership contract;
- provision the guest through cloud-init with the declared system packages;
- verify base package installation and provisioning phase markers;
- run the rootless sandbox probe inside the guest;
- attempt the four Plan 045 directions through `run-matrix.sh`;
- produce a sanitized diagnostic bundle.

The active run ID is `plan080-20260801034138-c8bec3f5`. The background
log is at `/tmp/opencode/plan080-run-all.log`.

If the chain returns a typed blocker, that blocker is the truthful Plan 080
outcome for this work package. Do not retry, re-provision, or silently
change the blocker.

### WP4. Re-run Plan 077 capability probe with guest lane

Status: pending.

After the fresh guest is available, re-run the Plan 077 capability probe
and update the qualification record at
`target/interop/lane/qualification.json`. If the guest probe returns
`rootless_sandbox_available` and the wrapper attestation contract is
satisfied, the qualification record moves to:

```text
selected_lane = remote-manual
scope = full-runtime
qualified = true
full_runtime_lane = available
```

If the guest probe returns a typed blocker, the qualification record retains
`qualified = false` and the blocker is recorded.

### WP5. Re-open Plan 078 with the qualified lane

Status: pending.

If WP4 produces `qualified = true`, update `plans/078-status.md` from
`blocked-preflight` to `ready`. The two i2pd directions are then executable
through the guest lane:

```text
bash scripts/interop/run-ntcp2-loopback-smoke.sh \
  --direction i2pr-to-i2pd-ipv4 \
  --reference-driver <path> \
  --reference-build-manifest <path> \
  --reference-source-lock <path> \
  --output <smoke-record.json> \
  --source-commit <40-lowercase-hex>

bash scripts/interop/run-ntcp2-loopback-smoke.sh \
  --direction i2pd-to-i2pr-ipv4 \
  --reference-driver <path> \
  --reference-build-manifest <path> \
  --reference-source-lock <path> \
  --output <smoke-record.json> \
  --source-commit <40-lowercase-hex>
```

If WP4 produces a typed blocker, Plan 078 remains `blocked-preflight` and
Plan 080 closes with that blocker.

### WP6. Control-build comparison (Phase 4 of Plan 078)

Status: pending.

After a successful instrumented direction in Plan 078, repeat a bounded
control run using the uninstrumented `i2pd_ntcp2_interop_driver_control`
binary. The control run may rely on i2pr-side evidence plus
process/transport outcome for observer neutrality. Block Plan 078 closure
when the instrumented build succeeds and the control build fails at
protocol level.

### WP7. Closure

Status: pending.

Write `plans/080-status.md` with the exact selected lane, qualification
record digest, run ID, commands, results, and any typed blocker. Update
`plans/078-status.md` to reflect the qualified lane or the remaining
blocker. Flip
`plans/079-repeated-i2pd-development-validation-and-continuation-decision.md`
from `planned` to `active-pending-plan078-close` only when Plan 078 is
unblocked.

## Required artifacts

- `plans/080-multipass-lane-prequalification-for-plan-078.md` (this file)
- `plans/080-status.md` (closure record)
- `tests/integration/ntcp2/harness/plan080.py` (helper module exporting
  `plan080_typed_blocker`, `plan080_close_status`,
  `plan080_lane_qualification_digest`, `plan080_guest_inspect_record`,
  `plan080_qualification_writer`)
- `tests/integration/ntcp2/harness/test_plan080.py` (test matrix)
- `scripts/interop/probe-constrained-host-lanes.sh` (extended with
  `--lane-from-guest <path>`)
- `scripts/check-ntcp2-interoperability.sh` (extended for Plan 080
  invariants)

## Acceptance criteria

Plan 080 closes only when:

- (a) the new Plan 077 qualification record is `qualified = true` with
  `full_runtime_lane = available`; OR
- (b) the lane returns a typed blocker;
- (c) at most one launch attempt was made;
- (d) no host policy change was made;
- (e) no daemon activation was made;
- (f) no global `multipass purge` was issued.

A typed blocker may close Plan 080 as the truthful outcome. It does not
authorize a retry, a fallback to a weaker lane, or a silent policy change.

## Validation commands

```text
bash scripts/check-multipass-interop-boundary.sh
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan080.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_multipass.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_loopback_smoke.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_loopback_smoke_record.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_reference_event.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_i2pd_direct_driver.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan065.py'
bash scripts/check-ntcp2-vectors.sh
cargo fmt --all --check
cargo check --workspace --all-targets
cargo test --workspace
git diff --check
```

## Non-goals

Plan 080 does not:

- repair the i2pd driver;
- repair the runner;
- execute real interoperability qualification;
- enable public networking;
- add broad CI;
- install privileged services;
- weaken Level 3 release requirements;
- retry rootless namespace or bubblewrap paths;
- retry Multipass QEMU driver installation.

## Stop rules

Stop and record a blocker when:

- the lane loses no-public-network guarantees;
- the tested binary differs from its verified digest;
- a real event cannot be distinguished from a synthetic test fixture;
- the observer changes protocol behavior;
- required correction exceeds the bounded NTCP2/driver/runner scope;
- cleanup cannot own all spawned state.

## Small-model execution guidance

- Read this plan and the Plan 074/077/078 plan-of-record before changing
  anything.
- Never modify the host AppArmor configuration or sysctls.
- Never issue `multipass purge` on any instance.
- Make at most one launch attempt. If it returns a typed blocker, that is
  the truthful Plan 080 outcome.
- Treat the stale instance as read-only background state. Do not adopt,
  resume, recreate, or destroy it.
- Do not proceed to Plan 078 protocol execution until the qualification
  record is `qualified = true`.
- Do not claim a full-runtime lane exists when the probe returned a typed
  blocker.
- Preserve the exact run ID and ownership contract for every evidence
  record.
