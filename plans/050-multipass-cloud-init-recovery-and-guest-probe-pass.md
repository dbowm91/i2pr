# Plan 050: Multipass cloud-init recovery and guest-probe closure pass

## Objective

Close the remaining pre-protocol blocker in the Multipass evidence lane by diagnosing, classifying, correcting, and safely recovering guest cloud-init failures until a fresh lifecycle-owned guest completes provisioning and returns the authoritative guest result:

```text
rootless_sandbox_available
```

Plan 049 corrected instance naming, lifecycle ownership, collision handling, and destructive-operation safety. A real target-host attempt then reached a fresh Multipass guest with verified root-owned ownership records, but stopped before the guest rootless probe with:

```text
blocked_cloud_init_failed
```

The completed Plan 050 phase must:

- preserve the Plan 049 ownership and lifecycle model;
- identify the exact cloud-init stage and failure class without exporting raw guest logs;
- replace the generic cloud-init blocker with deterministic typed outcomes;
- make provisioning verifiable and safely resumable for an owned guest;
- separate provisioning completion from package-install and toolchain readiness;
- prove the guest-only user-namespace policy is active;
- verify the `i2ptest` execution user remains non-sudo and capability-free;
- run the rootless probe inside the guest before any source-cache transfer or router process;
- produce a sanitized guest-environment evidence record for success or failure;
- provide safe operator guidance for an owned deleted-but-unpurged instance without using global `multipass purge`;
- stop the phase after guest-probe closure; do not restart the four-direction protocol matrix as part of this corrective pass.

Plan 050 is a provisioning and environment-validation pass. It does not advertise NTCP2 support, does not satisfy Plan 045 directional predicates, and does not close Milestone 3.

## Starting repository state

This plan starts from `main` commit:

```text
f5d56af4a96cc9f6dd27f83255125a0186ca925e
interop: complete Multipass lifecycle ownership corrective pass
```

Relevant current outcomes:

```text
host baseline probe = blocked_unprivileged_user_namespace
fresh Multipass instance allocation = passed
host lifecycle reservation = passed
guest ownership files = present
ownership verification = passed
cloud-init completion = blocked_cloud_init_failed
guest rootless probe = not reached
router process startup = not reached
owned deleted generation cleanup = blocked_deleted_instance_requires_purge
```

Relevant files include:

- `plans/045-ntcp2-mixed-router-proof-closure-corrective-pass.md`
- `plans/046-rootless-sealed-namespace-evidence-lane.md`
- `plans/047-cross-host-rootless-lane-expansion.md`
- `plans/048-multipass-permissive-rootless-evidence-environment.md`
- `plans/048-status.md`
- `plans/049-multipass-lifecycle-ownership-corrective-pass.md`
- `plans/049-status.md`
- `plans/049-closure.md`
- `docs/adr/0017-rootless-sealed-namespace-interop-evidence.md`
- `docs/adr/0018-multipass-rootless-interop-environment.md`
- `scripts/check-multipass-interop-boundary.sh`
- `scripts/interop/multipass/cloud-init.yaml`
- `scripts/interop/multipass/environment.toml`
- `scripts/interop/multipass/lifecycle.py`
- `scripts/interop/multipass/common.sh`
- `scripts/interop/multipass/create.sh`
- `scripts/interop/multipass/status.sh`
- `scripts/interop/multipass/probe.sh`
- `scripts/interop/multipass/run-evidence-lane.sh`
- `scripts/interop/multipass/destroy.sh`
- `scripts/interop/multipass/records.py`
- `tests/integration/ntcp2/harness/test_multipass.py`

The implementing agent must inventory the exact current script behavior before editing and must extend the canonical Plan 049 path rather than creating a parallel provisioning stack.

## Controlling boundaries

1. Do not change the target host's AppArmor or user-namespace policy.
2. The host `blocked_unprivileged_user_namespace` outcome remains an informational negative baseline and must not gate guest launch.
3. The guest probe is the authoritative user-namespace capability result for the Multipass lane.
4. Do not weaken Plan 049 ownership proof, lifecycle locking, generation tracking, or destructive-operation restrictions.
5. Do not adopt, resume, stop, delete, purge, or mutate an unowned instance.
6. Do not invoke global `multipass purge` from normal automation.
7. Do not export raw `/var/log/cloud-init.log`, `/var/log/cloud-init-output.log`, journald output, package-manager logs, shell traces, environment variables, SSH configuration, host paths, or endpoint-bearing diagnostics.
8. Do not run router processes during Plan 050 acceptance execution.
9. Do not transfer the Java I2P or i2pd caches before the guest rootless probe passes.
10. Do not transfer the authoritative source archive before the minimal guest environment and probe contract passes, unless a tiny probe-only payload is required and separately identified.
11. The `i2ptest` user must remain without sudo rights, privileged groups, file capabilities, ambient capabilities, or setuid helpers.
12. Guest administrative provisioning may use the default Multipass administrative account and root-owned cloud-init modules.
13. A degraded, timed-out, or partially completed cloud-init run is not success even if selected packages appear installed.
14. A passing guest probe cannot override a failed ownership, policy, cleanup, or attestation check.
15. Cleanup failure overrides environment success.
16. Plan 050 does not advance `specs/support.toml`.

## Defect statement

The current lifecycle collapses all cloud-init failures into:

```text
blocked_cloud_init_failed
```

That result is fail-closed but not actionable. It does not establish:

- whether cloud-init timed out or reached an explicit error state;
- which cloud-init stage failed;
- whether package installation, sysctl application, user creation, ownership-contract writing, Rust toolchain setup, or final validation failed;
- whether the guest is safely resumable;
- whether cloud-init itself is complete but a repository wrapper misclassified its status;
- whether a retry would be idempotent;
- whether a rebuild is required;
- whether the guest policy is already sufficient for the rootless probe.

Plan 050 introduces a strict provisioning state contract and a sanitized diagnostic surface.

## Architectural decision

Split guest preparation into three explicit layers:

```text
Layer 1: base cloud-init
  operating-system packages
  guest sysctls
  i2ptest account and permissions
  ownership contract files
  administrative helper installation

Layer 2: post-cloud-init verification
  cloud-init terminal status
  package/tool presence
  sysctl values
  user/group/capability contract
  ownership contract and filesystem modes
  no unexpected mounts or active routers

Layer 3: rootless guest probe
  run as i2ptest
  no sudo
  no router process
  no source/cache transfer dependency
  emit rootless_sandbox_available or a typed blocker
```

Cloud-init should remain minimal and deterministic. Long-running or retry-prone setup should move to an explicit, owned, idempotent post-cloud-init provisioning command where practical.

## Deliverable 1: Capture a sanitized cloud-init status record

Add a canonical guest inspection command that reads structured or bounded cloud-init status and writes a sanitized record.

Preferred inputs:

```text
cloud-init status --long
cloud-init status --format json
systemctl is-active cloud-init-local.service
systemctl is-active cloud-init.service
systemctl is-active cloud-config.service
systemctl is-active cloud-final.service
/var/lib/cloud/instance/boot-finished presence
```

Use structured output where supported. Normalize version differences in one parser.

Required sanitized fields:

```text
schema_version
run_id
instance_generation
cloud_init_state
cloud_init_stage
failure_class
failed_module
exit_status_class
boot_finished_present
cloud_init_version
elapsed_bucket
retry_safe
recommended_action
```

Allowed `cloud_init_state` values:

```text
running
done
degraded
error
disabled
timeout
unknown
```

Allowed `cloud_init_stage` values:

```text
local
network
config
final
post-verify
unknown
```

Do not retain free-form command output. Map known modules and errors to fixed taxonomy values.

## Deliverable 2: Replace the generic blocker taxonomy

Introduce precise typed outcomes:

```text
blocked_cloud_init_timeout
blocked_cloud_init_terminal_error
blocked_cloud_init_degraded
blocked_cloud_init_status_unparseable
blocked_cloud_init_package_failure
blocked_cloud_init_network_failure
blocked_cloud_init_sysctl_failure
blocked_cloud_init_user_creation_failure
blocked_cloud_init_group_contract_failure
blocked_cloud_init_ownership_contract_failure
blocked_cloud_init_toolchain_failure
blocked_cloud_init_filesystem_permission_failure
blocked_cloud_init_service_failure
blocked_cloud_init_post_verify_failure
blocked_cloud_init_resume_unsafe
```

Retain `blocked_cloud_init_failed` only as a compatibility alias for old evidence. New runs must emit the specific result.

Each blocker must carry a fixed remediation class:

```text
retry-status
resume-provisioning
recreate-owned
repair-package-source
repair-cloud-init-config
operator-inspection-required
```

No remediation field may contain a raw command, host path, package mirror URL, username, or arbitrary guest output.

## Deliverable 3: Minimize and stage cloud-init

Audit `cloud-init.yaml` and divide operations by failure sensitivity.

Cloud-init should perform only the operations required to establish a controlled guest:

- install a bounded base package set;
- apply guest-only sysctls;
- create and lock `i2ptest`;
- remove `i2ptest` from privileged groups;
- establish root-owned ownership and environment-contract files;
- install versioned repository helper files required for post-verify/probe;
- write explicit phase markers atomically;
- leave the guest online only for the provisioning phase.

Move long or retry-prone operations out of cloud-init where practical, including:

- Rust toolchain installation;
- large reference-router dependency installation;
- reference builds;
- cache imports;
- full repository validation;
- offline transition;
- protocol scenario preparation.

These belong to later explicit phases after the minimal rootless guest probe closes.

Cloud-init must not clone the repository, fetch router sources, or execute the NTCP2 harness.

## Deliverable 4: Add explicit provisioning phase markers

Create root-owned, immutable-by-`i2ptest` phase markers under:

```text
/var/lib/i2pr-interop/provisioning/
```

Suggested markers:

```text
base-packages.complete
sysctls.complete
i2ptest.complete
ownership-contract.complete
cloud-init.complete
post-verify.complete
rootless-probe.complete
```

Each marker must be a small JSON record containing:

```text
schema_version
run_id
instance_generation
phase
result
contract_digest
completed_at_utc
```

Markers must be written via temporary file plus atomic rename.

The host lifecycle record must not advance merely because a marker exists; it must validate ownership, file type, owner, mode, run ID, generation, and digest.

## Deliverable 5: Add idempotent post-cloud-init verification

Implement one owned guest verification entrypoint, for example:

```text
/usr/local/libexec/i2pr-interop/verify-base-environment
```

It must be safe to run repeatedly and must verify:

- cloud-init reached an allowed terminal state;
- required base packages are installed;
- `kernel.unprivileged_userns_clone = 1`;
- `kernel.apparmor_restrict_unprivileged_userns = 0` when that sysctl exists;
- AppArmor is not globally disabled merely to satisfy the lane;
- `i2ptest` exists with locked password;
- `i2ptest` has the expected home and shell;
- `i2ptest` is absent from sudo, adm, docker, lxd, systemd-journal, disk, kvm, and other privileged groups;
- `sudo -n true` fails as `i2ptest`;
- `CapInh`, `CapPrm`, `CapEff`, and `CapAmb` are zero for an `i2ptest` shell;
- no unexpected file capabilities exist on repository-owned helpers;
- ownership contract files are root-owned and non-writable by `i2ptest`;
- no unexpected host mounts are attached;
- no router or harness process is active;
- no default-offline policy has been installed prematurely.

The verifier must emit only a compact typed JSON record.

## Deliverable 6: Add safe resume semantics for provisioning

Extend `--resume-owned` so it can recover only when:

- host/guest ownership proof succeeds;
- instance generation matches;
- guest state is running or can be started safely;
- no router process has run;
- no protocol evidence exists for the generation;
- cloud-init failure is in an allowlisted retry-safe class;
- phase markers are internally consistent;
- source/cache preparation has not begun;
- the guest has no unexpected mounts or snapshots.

Resume behavior:

1. inspect cloud-init terminal state;
2. wait only if state is still `running`, with a bounded deadline;
3. classify any terminal failure;
4. rerun only the idempotent post-cloud-init provisioning or verification step;
5. never rerun arbitrary cloud-init user-data wholesale;
6. rerun ownership and guest-policy verification;
7. proceed to the guest rootless probe only after all checks pass.

Unsafe or ambiguous resume must emit:

```text
blocked_cloud_init_resume_unsafe
```

and recommend explicit owned recreation.

## Deliverable 7: Add an explicit minimal guest-probe operation

Add a lifecycle operation such as:

```text
bash scripts/interop/multipass/run-evidence-lane.sh \
  --guest-probe-only \
  --run-id <safe-id>
```

This operation must:

- require complete ownership proof;
- require successful cloud-init and post-verify records;
- run as `i2ptest`;
- use the repository's canonical rootless probe logic or a content-addressed probe-only payload;
- avoid source archive and reference-cache transfer;
- start no router process;
- write a sanitized guest probe record;
- update lifecycle state only after record validation.

Expected success:

```json
{"schema":1,"type":"rootless-sandbox-probe","outcome":"rootless_sandbox_available"}
```

The record must also bind:

```text
environment_id
run_id
instance_name_digest
instance_generation
ownership_record_sha256
cloud_init_record_sha256
post_verify_record_sha256
```

## Deliverable 8: Separate host baseline and guest capability evidence

Ensure the environment evidence schema contains distinct fields:

```text
host_baseline_probe_outcome
guest_rootless_probe_outcome
```

Rules:

- host baseline may be `blocked_unprivileged_user_namespace`;
- guest must be `rootless_sandbox_available` for Plan 050 success;
- host and guest outcomes must never overwrite each other;
- the aggregate validator must reject a guest-success claim copied from the host field;
- a missing guest field is not equivalent to a blocker or success;
- no protocol record may be emitted during Plan 050.

## Deliverable 9: Add owned deleted-instance remediation

The current host has an owned deleted-but-unpurged generation. Add a safe operator-facing inspection and remediation path.

Requirements:

- identify the exact owned run and generation through lifecycle state;
- prove the deleted resource belongs to the Plan 049 environment contract;
- detect whether the installed Multipass version supports selective purge or recover/delete operations for one instance;
- never execute global purge automatically;
- emit one of:

```text
selective_purge_supported
selective_purge_not_supported
resource_already_absent
ownership_not_proven
```

Where selective purge is supported, require an explicit destructive flag such as:

```text
--purge-owned-deleted --run-id <safe-id>
```

Where unsupported, write exact operator documentation that explains the unavoidable global scope and requires manual review of all deleted instances before any global purge. Automation must continue to refuse the operation.

This cleanup work is secondary to guest-probe closure and must not block creation of a new uniquely named guest.

## Deliverable 10: Add failure-preserving guest retention policy

Support explicit behavior on provisioning blockers:

```text
--keep-on-blocker
--destroy-owned-on-blocker
```

Default behavior should preserve a newly failed owned guest long enough for sanitized inspection unless policy or resource constraints dictate otherwise.

Requirements:

- never snapshot a secret-bearing or protocol-run guest;
- provisioning-only failed guests may be retained with lifecycle state `blocked`;
- retained guests must be ownership-verifiable;
- no automatic adoption on the next run;
- inspection remains read-only;
- destruction remains explicit and ownership-gated.

## Deliverable 11: Add deterministic tests

Extend `tests/integration/ntcp2/harness/test_multipass.py` or split focused tests if necessary.

Required unit and fake-Multipass cases:

1. cloud-init running then done;
2. cloud-init running then timeout;
3. terminal error in local stage;
4. terminal error in config stage;
5. terminal error in final stage;
6. degraded status;
7. unsupported JSON format with safe fallback;
8. unparseable status fails closed;
9. missing boot-finished marker;
10. package-install failure classification;
11. sysctl failure classification;
12. user/group contract failure classification;
13. ownership-contract failure classification;
14. post-verify failure classification;
15. retry-safe resume;
16. unsafe resume after source transfer marker;
17. resume with generation mismatch;
18. resume with active router process;
19. successful guest-probe-only flow;
20. guest probe blocked after successful cloud-init;
21. host negative baseline plus guest success;
22. missing guest outcome rejected;
23. no protocol records written by Plan 050;
24. selective purge supported for owned resource;
25. selective purge unsupported;
26. unowned deleted resource left untouched;
27. retained blocker guest inspection;
28. destruction after blocker requires ownership proof.

Tests must use fake or simulated Multipass responses. They must not require a local Multipass daemon in the normal test suite.

## Deliverable 12: Add static boundary checks

Extend `scripts/check-multipass-interop-boundary.sh` to fail when Plan 050-owned files:

- invoke global `multipass purge` in an automated path;
- export raw cloud-init logs;
- copy journald output into evidence;
- run routers from guest-probe-only mode;
- transfer reference caches before guest probe success;
- collapse host and guest probe fields;
- weaken ownership checks for resume or cleanup;
- execute arbitrary guest commands from user input;
- use `eval`;
- add the execution user to privileged groups;
- disable AppArmor globally;
- treat cloud-init degraded/error states as success.

## Deliverable 13: Documentation reconciliation

Update:

- `plans/048-status.md`
- `plans/049-status.md`
- `docs/adr/0018-multipass-rootless-interop-environment.md`
- `docs/architecture/interop-apparatus.md`
- `docs/private-testnet.md`
- `docs/security-model.md`
- `tests/integration/ntcp2/README.md`
- `tests/integration/ntcp2/evidence/README.md`
- `scripts/interop/multipass/README.md`
- `AGENTS.md`
- `.opencode/skills/i2pr-ntcp2-interop/SKILL.md`
- `.opencode/skills/i2pr-ntcp2-interop/references/operations.md`
- `specs/CONFORMANCE.md`

Documentation must state:

- Plan 049 fixed lifecycle ownership and collision handling;
- Plan 050 addresses cloud-init and guest-probe closure only;
- the host blocker remains expected and informational;
- no source/cache transfer or router execution is required for Plan 050 acceptance;
- external interoperability evidence remains unclaimed;
- NTCP2 remains experimental and non-advertised.

## Implementation sequence

### Work package 1: Reproduce and classify

- inspect the retained owned failed guest read-only;
- collect only transient operator diagnostics;
- identify the cloud-init stage/module/failure class;
- do not commit raw diagnostics;
- add the sanitized classification parser and record schema.

### Work package 2: Minimize cloud-init

- remove long-running and nonessential work from cloud-init;
- add explicit phase markers;
- preserve ownership-contract creation;
- add bounded completion deadlines.

### Work package 3: Post-verify and resume

- implement idempotent base-environment verification;
- add retry-safe resume classification;
- prevent unsafe resume after later-phase contamination.

### Work package 4: Guest-probe-only path

- add the minimal probe operation;
- bind records to ownership and provisioning evidence;
- ensure no source/cache/router activity.

### Work package 5: Cleanup remediation

- add owned deleted-resource inspection;
- support selective purge only where Multipass provides an instance-scoped operation;
- document manual global-scope risk otherwise.

### Work package 6: Tests and static checks

- complete fake-Multipass coverage;
- extend boundary checker;
- run shell syntax and Python tests.

### Work package 7: Target-host closure attempt

Run the minimal ladder only:

```text
fresh generated run
→ lifecycle reservation
→ guest launch
→ ownership verification
→ cloud-init terminal success
→ post-cloud-init verification
→ guest rootless probe
→ sanitized environment evidence export
→ explicit owned cleanup or retained-debug state
```

Do not continue to source/cache transfer or router execution in this phase.

## Required validation

Run at minimum:

```bash
python3 -m unittest discover \
  -s tests/integration/ntcp2/harness \
  -p 'test_*.py'

bash scripts/check-multipass-interop-boundary.sh
bash scripts/check-rootless-interop-boundary.sh
bash -n scripts/interop/multipass/*.sh

cargo fmt --all --check
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps

bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundary.sh
bash scripts/check-ntcp2-interoperability.sh
```

The target-host execution must use a fresh run ID and a generated concrete instance name.

## Acceptance criteria

Plan 050 local implementation is complete when:

1. generic cloud-init failure is replaced by precise typed classification;
2. cloud-init status parsing is version-tolerant and fail-closed;
3. cloud-init is minimized to deterministic base-environment work;
4. phase markers are ownership- and generation-bound;
5. post-cloud-init verification is idempotent;
6. resume is allowed only for retry-safe owned states;
7. guest-probe-only mode starts no router and transfers no reference cache;
8. host and guest probe fields are distinct;
9. deleted-resource remediation remains ownership-gated and non-global by default;
10. deterministic tests and static checks pass;
11. documentation is reconciled.

Plan 050 external closure is complete only when one fresh target-host run produces all of:

```text
ownership_verified
cloud_init_state = done
post_verify = passed
guest_rootless_probe_outcome = rootless_sandbox_available
router_process_count = 0
reference_cache_transferred = false
protocol_record_count = 0
cleanup_result = clean or explicitly retained-owned-debug-instance
```

The exported environment bundle must contain:

- lifecycle record;
- ownership-record digest;
- sanitized cloud-init status record;
- post-verify record;
- guest rootless probe record;
- manifest with SHA-256 for every retained file;
- no raw logs or sensitive material.

## Stop conditions

Stop and emit a typed blocker if:

- ownership proof fails;
- instance generation is ambiguous;
- cloud-init status cannot be parsed safely;
- cloud-init remains running beyond the deadline;
- cloud-init reaches degraded or error state;
- post-verify detects an unexpected privilege or mount;
- resume safety cannot be established;
- guest sysctls do not match the permissive contract;
- `i2ptest` has sudo or nonzero capabilities;
- guest probe does not return an allowed typed outcome;
- any router process appears;
- reference cache transfer occurs before probe closure;
- evidence sanitation or manifest validation fails;
- cleanup attempts would affect an unowned resource.

## Non-claims

Plan 050 does not:

- prove Java I2P interoperability;
- prove i2pd interoperability;
- run the four Plan 045 directions;
- validate NTCP2 data exchange;
- close Plan 045 external evidence;
- advance the protocol support ledger;
- close Milestone 3.

Its sole external success claim is that the lifecycle-owned Multipass guest can be provisioned reproducibly and can run the rootless sealed-namespace probe as an ordinary non-sudo user.

## Handoff requirements

The implementing agent must leave:

- focused commits with clear behavioral scope;
- `plans/050-status.md` with exact commands and typed outcomes;
- `plans/050-closure.md` only if the stated local or external criteria are met and clearly distinguished;
- no raw guest logs committed;
- no unowned Multipass resource mutated;
- exact remaining blocker if `rootless_sandbox_available` is not achieved;
- explicit instance/run identifiers for any retained owned debug guest;
- confirmation that no router process ran during the Plan 050 target-host attempt.
