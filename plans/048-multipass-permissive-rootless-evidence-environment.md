# Plan 048: Multipass permissive rootless evidence environment

## Objective

Create a repeatable, disposable Ubuntu 24.04 amd64 Multipass environment that permits the Plan 046 rootless sealed-namespace lane to execute as an ordinary non-sudo user, while leaving the target host's AppArmor, user-namespace, network, and privilege policy unchanged.

The completed phase must give an implementing agent one deterministic workflow that:

- verifies Multipass is installed and usable on the target system;
- launches a named Ubuntu 24.04 amd64 instance with fixed CPU, memory, disk, and cloud-init inputs;
- applies the permissive user-namespace policy only inside the guest;
- installs all build and interoperability dependencies during guest provisioning;
- creates a dedicated ordinary execution user with no sudo rights;
- transfers or checks out an exact i2pr revision;
- imports and verifies the pinned Java I2P and i2pd reference caches without fetching during scenario execution;
- proves the guest belongs to a compatible Plan 047 host category;
- runs the repository's rootless probe before any router process;
- executes the four Plan 045 directional NTCP2 scenarios without sudo;
- exports only sanitized evidence and manifests to the host;
- records enough environment metadata to reproduce the run;
- supports stop, snapshot, restore, rebuild, and destroy operations;
- leaves no persistent relaxation on the target host.

This plan is an environment-orchestration and evidence-gathering plan. It does not weaken Plan 045, Plan 046, or Plan 047 pass predicates. It does not advertise NTCP2 support and does not close Milestone 3 by itself.

## Starting repository state

This plan starts from `main` commit:

```text
d67bfcadedf21f35d2949f77f72ab68d79d3b526
```

Relevant current plans and records:

- `plans/045-ntcp2-mixed-router-proof-closure-corrective-pass.md`
- `plans/046-rootless-sealed-namespace-evidence-lane.md`
- `plans/046-status.md`
- `plans/046-closure.md`
- `plans/047-cross-host-rootless-lane-expansion.md`
- `docs/adr/0017-rootless-sealed-namespace-interop-evidence.md`
- `scripts/interop/probe-rootless-sandbox.sh`
- `scripts/interop/rootless-enter.sh`
- `scripts/check-rootless-interop-boundary.sh`
- `tests/integration/ntcp2/harness/rootless_supervisor.py`
- `tests/integration/ntcp2/harness/rootless_inner_runner.py`
- `tests/integration/ntcp2/harness/rootless_topology.py`

The current host is the negative baseline:

```text
kernel.unprivileged_userns_clone = 1
kernel.apparmor_restrict_unprivileged_userns = 1
probe outcome = blocked_unprivileged_user_namespace
```

Plan 048 must create a guest in the recovery category:

```text
host.apparmor-restrict-off
kernel.unprivileged_userns_clone = 1
kernel.apparmor_restrict_unprivileged_userns = 0
probe outcome = rootless_sandbox_available
```

## Controlling boundaries

The implementing agent must preserve these rules:

1. Do not change the target host's AppArmor configuration or user-namespace sysctls.
2. All permissive kernel policy changes occur only inside the disposable Multipass guest.
3. The i2pr scenario process runs as an ordinary guest user with no sudo permission.
4. Guest provisioning may use sudo through cloud-init or the default Multipass administrative user; the actual probe and evidence lane may not.
5. Do not run the rootless evidence scripts as guest root.
6. Do not grant file capabilities, setuid wrappers, ambient capabilities, Docker group membership, LXD group membership, or passwordless sudo to the execution user.
7. Do not use a privileged container as a substitute for the VM.
8. Reference preparation and execution remain separate. Scenario execution must be offline.
9. Do not mount the host repository into the guest for the authoritative evidence run. Transfer an immutable source archive or clone and verify an exact commit inside the guest.
10. Do not treat Multipass shared mounts as an evidence-integrity boundary.
11. Do not retain raw RouterInfo, private keys, I2NP bodies, frame plaintext, logs, packet captures, private paths, or endpoint-bearing diagnostics in exported evidence.
12. Cleanup or evidence-validation failure overrides protocol success.
13. A failed rootless probe is a typed blocker, not a skipped success.
14. Do not silently fall back to the privileged dual-netns backend.
15. Destroying the guest must not delete the host-side exported evidence directory.

## Multipass assumptions

The target host must provide a working Multipass installation and sufficient virtualization support.

The orchestrator must use current supported Multipass commands rather than shelling into undocumented daemon internals. The intended lifecycle uses:

```text
multipass version
multipass find
multipass launch
multipass wait-ready
multipass info
multipass exec
multipass transfer
multipass snapshot
multipass restore
multipass stop
multipass delete
multipass purge
```

Canonical instance launch shape:

```bash
multipass launch 24.04 \
  --name i2pr-interop-rootless \
  --cpus 4 \
  --memory 8G \
  --disk 40G \
  --timeout 1800 \
  --cloud-init scripts/interop/multipass/cloud-init.yaml
```

Multipass supports named instances, explicit CPU, memory, disk, timeout, and cloud-init inputs. Plan 048 must use those explicit values and must not rely on Multipass defaults.

## Deliverable 1: Add a Multipass environment ADR

Create:

```text
docs/adr/0018-multipass-rootless-interop-environment.md
```

The ADR must record:

- why the existing host remains the negative baseline;
- why a VM is preferred over changing host-wide AppArmor policy;
- why Multipass is selected for repeatable lifecycle management;
- the guest-only sysctl changes;
- the split between administrative provisioning and ordinary-user execution;
- why host mounts are not authoritative source or evidence inputs;
- the exact VM image, resource, and naming policy;
- source-transfer and reference-cache-transfer policy;
- online preparation versus offline execution phases;
- snapshot and rebuild policy;
- exported evidence boundary;
- supported host platforms and explicit blockers;
- relationship to Plans 046 and 047.

Update ADR indexes and `docs/architecture/interop-apparatus.md` in the same commit.

## Deliverable 2: Add the Multipass directory layout

Create:

```text
scripts/interop/multipass/
  README.md
  cloud-init.yaml
  common.sh
  create.sh
  status.sh
  transfer-source.sh
  transfer-cache.sh
  prepare-offline.sh
  probe.sh
  run-direction.sh
  run-matrix.sh
  export-evidence.sh
  snapshot.sh
  restore.sh
  destroy.sh
  verify-clean-host.sh
```

All scripts must use:

```bash
set -euo pipefail
```

They must reject unknown arguments, duplicate arguments, empty values, unsafe paths, unknown instance names, and unknown scenario IDs.

No script may use `eval` or accept arbitrary command strings.

## Deliverable 3: Define one canonical environment manifest

Create:

```text
scripts/interop/multipass/environment.toml
```

It must be the single source of truth for:

```toml
schema = 1
instance_name = "i2pr-interop-rootless"
image = "24.04"
cpus = 4
memory = "8G"
disk = "40G"
launch_timeout_seconds = 1800
guest_admin_user = "ubuntu"
guest_execution_user = "i2ptest"
guest_repo_root = "/home/i2ptest/i2pr"
guest_cache_root = "/home/i2ptest/i2pr/target/interop/cache"
guest_evidence_root = "/home/i2ptest/i2pr/target/interop/evidence"
required_architecture = "x86_64"
required_os_id = "ubuntu"
required_os_version = "24.04"
required_rust_toolchain = "1.95.0"
required_topology_kind = "rootless-sealed-single-netns"
required_privilege_model = "unprivileged-userns"
```

The actual cache path must be reconciled against the repository implementation before this file is finalized. Do not preserve both `target/interop/cache` and `target/interop/build/cache` as competing canonical paths.

Add a strict parser or query utility so shell scripts do not duplicate these values.

## Deliverable 4: Implement deterministic cloud-init provisioning

Create `cloud-init.yaml` with these responsibilities:

### Package installation

Install the exact required guest packages from Ubuntu 24.04 repositories:

```text
build-essential
clang
cmake
pkg-config
git
curl
ca-certificates
python3
python3-venv
openjdk-21-jdk-headless
ant
iproute2
util-linux
jq
rsync
openssh-client
tar
xz-utils
```

Add any package demonstrably required by the pinned Java I2P or i2pd build, but keep the list explicit and documented.

### Guest-only sysctls

Create:

```text
/etc/sysctl.d/90-i2pr-rootless-interop.conf
```

with:

```text
kernel.unprivileged_userns_clone = 1
kernel.apparmor_restrict_unprivileged_userns = 0
```

Run `sysctl --system` and fail provisioning if either effective value differs.

Do not disable AppArmor globally.

### Execution user

Create `i2ptest` with:

- home directory;
- `/bin/bash` shell;
- locked password;
- no sudo group membership;
- no `adm`, `docker`, `lxd`, or device-access group membership;
- a private home directory;
- no authorized keys unless explicitly injected by a reviewed local file.

The default `ubuntu` account remains the provisioning account. The evidence scripts execute through:

```text
multipass exec <instance> -- sudo -iu i2ptest -- <fixed command>
```

The use of sudo here is the Multipass administrative boundary used to select the ordinary guest user. The command executed as `i2ptest` must not invoke sudo and must have zero effective, permitted, inheritable, and ambient capabilities.

### Rust toolchain

Install rustup for `i2ptest`, then install exactly:

```text
1.95.0
rustfmt
clippy
```

Do not rely on the host Rust installation.

### Provisioning marker

Write a root-owned immutable provisioning record under:

```text
/var/lib/i2pr-interop/provisioning.json
```

It must contain:

- cloud-init schema version;
- image release;
- architecture;
- package-list digest;
- effective sysctls;
- Rust toolchain version;
- Java version;
- Ant version;
- CMake version;
- compiler version;
- provisioning completion timestamp.

Do not include secrets, network identifiers, or host paths.

## Deliverable 5: Implement idempotent instance creation

Create `create.sh`.

Required behavior:

1. Verify `multipass` exists.
2. Record `multipass version`.
3. Verify the requested Ubuntu 24.04 image is available.
4. Refuse to overwrite an existing instance unless `--replace` is explicitly provided.
5. On `--replace`, stop and delete only the canonical instance name, then purge deleted state.
6. Launch with explicit image, resources, timeout, and cloud-init.
7. Wait for readiness.
8. Wait for cloud-init completion using a bounded guest command.
9. Verify provisioning marker presence and schema.
10. Verify guest OS and architecture.
11. Verify effective sysctls.
12. Verify `i2ptest` exists and is not sudo-capable.
13. Verify tool versions.
14. Write a host-side creation record under:

```text
target/interop/multipass/<instance>/creation.json
```

The script must cleanly classify:

```text
blocked_multipass_missing
blocked_multipass_daemon_unavailable
blocked_image_unavailable
blocked_instance_name_collision
blocked_launch_failed
blocked_cloud_init_failed
blocked_wrong_guest_os
blocked_wrong_guest_architecture
blocked_guest_policy_mismatch
blocked_execution_user_privileged
```

## Deliverable 6: Add source preparation and immutable transfer

Create `transfer-source.sh`.

The authoritative mode must:

1. Require a clean host checkout.
2. Require an exact 40-character commit SHA.
3. Verify `git rev-parse HEAD` equals the requested SHA.
4. Verify no tracked or untracked changes are present, except allowlisted generated directories.
5. Create a deterministic source archive using sorted paths and normalized metadata.
6. Exclude `.git`, `target`, evidence, caches, local credentials, editor state, and other generated data.
7. Compute the archive SHA-256.
8. Transfer it with `multipass transfer` into a guest staging directory.
9. Extract as `i2ptest` into the canonical guest repository path.
10. Write the exact commit SHA and source archive SHA-256 into a guest source manifest.
11. Verify the extracted tree digest using a repository-provided deterministic tree-hash utility.

A secondary developer convenience mode may use `git clone`, but it must not be accepted for authoritative retained evidence unless the exact commit and resulting tree digest match the source manifest.

Do not use `multipass mount` for the authoritative source tree. Multipass mounts map a host directory into the guest and introduce host filesystem state into the run.

## Deliverable 7: Reconcile and transfer the reference cache

Create `transfer-cache.sh` and `prepare-offline.sh`.

Before implementation, determine the one canonical cache root by tracing:

- `scripts/interop/cache-manifest.py`;
- `scripts/interop/build-references.sh`;
- `scripts/interop/offline-reuse.sh`;
- `_cache_for()` in the harness;
- Plan 043 build summaries;
- Plan 047 cache references.

Update Plan 047 documentation if its example path is wrong.

The transfer flow must:

1. Require the host-side reference cache manifest.
2. Verify pinned versions and revisions.
3. Verify artifact and installed-tree hashes before transfer.
4. Package the cache deterministically.
5. Transfer with `multipass transfer`.
6. Extract as `i2ptest` into the canonical guest cache root.
7. Verify every digest again inside the guest.
8. Run the repository's offline-reuse validation.
9. Reject any network fetch or cache miss during execution preparation.
10. Produce a sanitized cache-import receipt.

Typed failures:

```text
blocked_reference_cache_missing
blocked_reference_cache_manifest_invalid
blocked_reference_cache_revision_mismatch
blocked_reference_cache_hash_mismatch
blocked_reference_cache_transfer_failed
blocked_reference_cache_offline_reuse_failed
```

## Deliverable 8: Implement guest compatibility and privilege verification

Create `status.sh` and `probe.sh`.

`status.sh` must report, in typed JSON:

- Multipass instance state;
- guest OS/version;
- guest architecture;
- effective sysctls;
- AppArmor enabled state;
- execution-user group list;
- execution-user capability fields;
- execution-user sudo result;
- repository commit and tree digest;
- reference-cache verification state;
- latest rootless probe outcome.

`probe.sh` must execute as `i2ptest`:

```bash
bash scripts/interop/probe-rootless-sandbox.sh \
  --attestation-path target/interop/evidence/environment-probe/probe.json
```

Then it must execute:

```bash
bash scripts/interop/rootless-enter.sh \
  --probe \
  --attestation-output target/interop/evidence/environment-probe/wrapper-attestation.json
```

Both must return:

```text
rootless_sandbox_available
```

The wrapper attestation must validate and have a nonzero content hash.

Do not run any router process unless this deliverable passes.

## Deliverable 9: Add explicit online and offline phases

The orchestrator must track two phases:

### Preparation phase

Network may be available for:

- Multipass image retrieval;
- apt package installation through cloud-init;
- rustup installation;
- optional exact repository clone;
- pinned reference-cache construction if not transferred.

### Execution phase

The scenario execution phase must be offline.

Implement one of these reviewed controls:

1. Preferred: stop the instance, disconnect or place its Multipass NIC into a host-controlled isolated network if supported by the target platform, restart, and verify no egress.
2. Acceptable first implementation: apply guest firewall policy administratively before execution that denies all non-loopback egress except the Multipass control path required by `multipass exec`, then verify it with route and connection probes.
3. Alternate: run all authoritative commands through a preloaded guest script, stop the guest NIC administratively, execute from the guest console/control channel where supported, then restore only after evidence export.

The implementing agent must validate which control is reliable for the target Multipass driver. Do not assume host `multipass exec` remains available after arbitrary guest-network removal.

At minimum, the rootless inner namespace must still prove:

- loopback only;
- no default route;
- no external interface;
- failed external route lookup;
- failed external connect probe.

Record the selected VM-level offline enforcement mode in evidence metadata.

## Deliverable 10: Implement directional and matrix runners

Create `run-direction.sh` and `run-matrix.sh`.

Allowed scenarios:

```text
i2pr-to-java-ipv4
java-to-i2pr-ipv4
i2pr-to-i2pd-ipv4
i2pd-to-i2pr-ipv4
```

Canonical reference mapping:

```text
i2pr-to-java-ipv4 -> java_i2p
java-to-i2pr-ipv4 -> java_i2p
i2pr-to-i2pd-ipv4 -> i2pd
i2pd-to-i2pr-ipv4 -> i2pd
```

Do not infer unknown references with a default branch.

`run-direction.sh` must:

1. Verify environment status.
2. Verify source manifest.
3. Verify cache manifest.
4. Re-run rootless probe or verify a fresh bounded probe.
5. Create a direction-specific evidence directory.
6. Execute `rootless-enter.sh` as `i2ptest`.
7. Require a validated isolation attestation.
8. Require one sanitized mixed-router record.
9. Validate evidence.
10. Verify cleanup.
11. Record a typed outcome.

`run-matrix.sh` must:

1. Run directions in a fixed order.
2. Stop immediately on a host/isolation/reference-control failure.
3. Continue to collect typed protocol failures only where cleanup remains clean and the plan explicitly permits comparison.
4. Require all four records for a passing matrix.
5. Require the same source commit, tree digest, reference revisions, topology kind, privilege model, and environment-manifest digest across the matrix.
6. Produce a content-addressed manifest.
7. Run `scripts/check-ntcp2-interoperability.sh` against the exported set.

## Deliverable 11: Add environment attestation to exported evidence

Do not modify the protocol evidence schema casually. Prefer a separate environment record linked by digest.

Create a sanitized record such as:

```json
{
  "schema": 1,
  "type": "multipass-interop-environment",
  "instance_image": "ubuntu-24.04",
  "architecture": "x86_64",
  "resource_profile": "4cpu-8g-40g",
  "cloud_init_sha256": "...",
  "environment_manifest_sha256": "...",
  "provisioning_record_sha256": "...",
  "source_commit": "...",
  "source_tree_sha256": "...",
  "reference_cache_manifest_sha256": "...",
  "userns_clone": 1,
  "apparmor_restrict_unprivileged_userns": 0,
  "execution_user_privileged": false,
  "rootless_probe_outcome": "rootless_sandbox_available",
  "offline_enforcement": "..."
}
```

Each direction record or the aggregate manifest must link to this record by SHA-256.

Do not retain:

- instance IP address;
- host username;
- guest UID/GID numeric values;
- SSH keys;
- Multipass daemon paths;
- host filesystem paths;
- raw provisioning logs;
- raw cloud-init logs.

## Deliverable 12: Implement evidence export

Create `export-evidence.sh`.

Use `multipass transfer` to copy the sanitized evidence bundle from the guest into:

```text
target/interop/evidence/multipass/<run-id>/
```

Required exported files:

```text
environment.json
environment.json.sha256
probe.json
probe.json.sha256
<four directional sanitized records>
aggregate.json
manifest.json
```

The export script must:

- transfer into a temporary host directory;
- validate every file before final placement;
- reject symlinks, devices, sockets, FIFOs, hardlink surprises, and unexpected files;
- reject files over explicit size limits;
- compute host-side hashes independently;
- compare against the guest manifest;
- atomically rename the validated temporary directory into place;
- never export raw run directories or logs.

## Deliverable 13: Add snapshots and deterministic rebuild verification

Create `snapshot.sh` and `restore.sh`.

Required snapshots:

```text
provisioned
source-and-cache-ready
```

`provisioned` is taken after cloud-init, sysctl, user, package, and toolchain validation.

`source-and-cache-ready` is taken after exact source and reference-cache verification but before any scenario run.

Snapshot scripts must:

- refuse snapshot creation when the guest is in an unknown state;
- attach a comment containing the environment-manifest digest and commit SHA;
- verify the snapshot appears in Multipass metadata;
- never snapshot after private scenario state has been generated unless the run root is securely removed first.

`restore.sh` must:

- restore only a known snapshot name;
- re-run status and probe checks after restore;
- reject a restored guest whose source or cache digest differs.

Repeatability requirement:

1. Run the matrix from a fresh instance.
2. Destroy the instance.
3. Recreate from the same cloud-init and inputs.
4. Run the matrix again.
5. Restore `source-and-cache-ready` and run a third time.

All three runs must have the same environment-manifest digest, source commit/tree digest, reference revisions, topology kind, privilege model, and scenario catalog. Run IDs, timestamps, process IDs, generated identities, and cryptographic nonces may differ.

## Deliverable 14: Implement safe destruction

Create `destroy.sh`.

It must:

1. Refuse unknown instance names.
2. Verify exported evidence exists or require `--discard-unexported`.
3. Stop the canonical instance.
4. Delete it.
5. Purge deleted Multipass state only when explicitly requested or when no unrelated deleted instances would be affected.
6. Verify the instance no longer appears in `multipass list`.
7. Preserve host-side source archives, cache archives, and evidence.
8. Write a host-side destruction receipt.

Do not run broad host cleanup commands.

## Deliverable 15: Add host-state verification

Create `verify-clean-host.sh` for the Multipass orchestration layer.

Before creation, record:

- existing Multipass instance names;
- Multipass networks metadata;
- relevant host sysctls;
- host AppArmor restriction value;
- host route digest;
- host link digest;
- host firewall digest where readable without escalation;
- target evidence directory state.

After destruction, verify:

- unrelated Multipass instances are unchanged;
- host sysctls and AppArmor restriction are unchanged;
- host route/link/firewall digests are unchanged except documented Multipass-managed transient state;
- only expected evidence and orchestration records were added.

The checker must distinguish stable host state from Multipass daemon-managed implementation details and avoid false claims where data is not readable.

## Deliverable 16: Add static and simulated tests

Add tests for:

- environment manifest parsing;
- fixed instance-name enforcement;
- replacement safeguards;
- cloud-init package and sysctl requirements;
- execution-user group restrictions;
- exact source-commit verification;
- dirty-tree rejection;
- source archive normalization;
- cache-path reconciliation;
- cache hash mismatch;
- Multipass command construction;
- no arbitrary guest command injection;
- scenario/reference mapping;
- typed probe blocker handling;
- evidence-transfer symlink rejection;
- oversized evidence rejection;
- manifest mismatch;
- snapshot-name enforcement;
- unexported-evidence destruction refusal;
- unrelated-instance preservation;
- host sysctl non-mutation;
- repeated-create/destroy state-machine behavior.

Add a fake Multipass executable for tests. Do not require a real hypervisor in normal unit tests.

Add an opt-in integration test marker for hosts where Multipass is installed.

## Deliverable 17: Add a one-command orchestrator

Create:

```text
scripts/interop/multipass/run-evidence-lane.sh
```

Supported operations:

```text
--create
--prepare
--probe
--run
--export
--destroy
--all
```

`--all` must perform:

```text
host baseline
create
cloud-init validation
source transfer
cache transfer and offline validation
snapshot source-and-cache-ready
rootless probe
offline transition
four-direction matrix
evidence validation
export
online state remains disabled until run completion
instance cleanup verification
optional destroy
host post-state verification
```

Require an explicit `--destroy-after-export` flag before deleting the VM.

No operation may accept an arbitrary shell command.

## Deliverable 18: Documentation and skill reconciliation

Update:

- `README.md`
- `AGENTS.md`
- `CONTRIBUTING.md`
- `GUARDRAILS.md`
- `docs/architecture/interop-apparatus.md`
- `docs/private-testnet.md`
- `docs/security-model.md`
- `docs/protocol-support.md`
- `specs/CONFORMANCE.md`
- `tests/integration/ntcp2/README.md`
- `tests/integration/ntcp2/evidence/README.md`
- `.opencode/skills/i2pr-ntcp2-interop/SKILL.md`
- `.opencode/skills/i2pr-ntcp2-interop/references/operations.md`
- `plans/047-cross-host-rootless-lane-expansion.md`

Document:

- current host remains the AppArmor-restricted negative baseline;
- Multipass guest is the permissive recovery host;
- exact creation and destruction commands;
- preparation versus execution phases;
- ordinary-user execution boundary;
- cache transfer and canonical path;
- probe-first requirement;
- snapshot policy;
- evidence export and sanitation;
- repeatability proof;
- non-claims.

## Required implementation sequence

Use these work packages in order.

### Work package 1: Contract and ADR

- ADR 0018.
- Environment manifest.
- Directory layout.
- Canonical cache-path decision.
- Documentation of privilege and trust boundaries.

### Work package 2: Provisioning

- Cloud-init.
- Create/status scripts.
- Guest policy and user validation.
- Toolchain validation.
- Typed blockers.

### Work package 3: Immutable inputs

- Source archive and transfer.
- Reference cache transfer and verification.
- Offline-reuse validation.
- Source/cache manifests.

### Work package 4: Probe and execution

- Probe script.
- Online/offline phase transition.
- Direction runner.
- Matrix runner.
- Evidence validators.

### Work package 5: Export and lifecycle

- Evidence export.
- Snapshot/restore.
- Destroy safeguards.
- Host-state verification.

### Work package 6: Tests and documentation

- Fake Multipass test harness.
- Opt-in real Multipass integration test.
- Static boundary checks.
- Documentation reconciliation.

### Work package 7: External execution

- Fresh create/run/export/destroy.
- Fresh rebuild repetition.
- Snapshot restore repetition.
- Cross-run comparison.
- Closure record.

## Local validation ladder

Before launching a real instance:

```bash
bash -n scripts/interop/multipass/*.sh
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py'
bash scripts/check-rootless-interop-boundary.sh
bash scripts/check-ntcp2-interoperability.sh
cargo +1.95.0 fmt --all --check
cargo +1.95.0 check --locked --workspace --all-targets
cargo +1.95.0 test --locked --workspace
cargo +1.95.0 clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo +1.95.0 doc --locked --workspace --no-deps
git diff --check
```

The fake-Multipass tests must pass without Multipass installed.

## External execution ladder

On the target host:

```text
1. Verify host baseline and Multipass availability.
2. Create the instance from cloud-init.
3. Verify cloud-init completion and guest policy.
4. Transfer exact source.
5. Transfer and verify reference cache.
6. Create source-and-cache-ready snapshot.
7. Run rootless probe.
8. Enter offline execution phase.
9. Run i2pr-to-java-ipv4.
10. Run java-to-i2pr-ipv4.
11. Run i2pr-to-i2pd-ipv4.
12. Run i2pd-to-i2pr-ipv4.
13. Validate aggregate evidence.
14. Export sanitized evidence.
15. Verify guest cleanup.
16. Destroy the instance.
17. Verify host post-state.
18. Repeat from a fresh instance.
19. Repeat from the source-and-cache-ready snapshot.
```

Stop immediately if:

- the guest sysctl policy is wrong;
- the execution user has sudo or host-like capabilities;
- source or cache hashes mismatch;
- the rootless probe is not `rootless_sandbox_available`;
- the VM cannot be placed into the selected offline mode;
- isolation attestation fails;
- cleanup fails;
- evidence validation fails;
- exported evidence differs from the guest manifest;
- host policy changes outside Multipass-managed state.

## Required evidence bundle

A successful Plan 048 execution must export:

```text
target/interop/evidence/multipass/<run-id>/
  environment.json
  environment.json.sha256
  probe.json
  probe.json.sha256
  i2pr-to-java-ipv4.json
  java-to-i2pr-ipv4.json
  i2pr-to-i2pd-ipv4.json
  i2pd-to-i2pr-ipv4.json
  aggregate.json
  manifest.json
  lifecycle.json
```

Every passed direction must retain the Plan 045 and Plan 046 requirements:

```text
topology_kind = rootless-sealed-single-netns
privilege_model = unprivileged-userns
sandbox_attestation_sha256 = nonzero
parent_network_state_unchanged = true
cleanup_result = clean
authenticated observations on both sides
directional sender and receiver observations
real nonzero required hashes
```

The Multipass environment record must prove the guest recovery category and source/cache identity without exposing host-specific secrets.

## Acceptance criteria

Plan 048 implementation is complete only when:

1. One command can create the documented guest from nothing but Multipass, repository inputs, cloud-init, and optional verified cache archive.
2. The target host's AppArmor and user-namespace policy remain unchanged.
3. The guest has the required permissive sysctls.
4. `i2ptest` has no sudo rights or ambient capabilities.
5. The repository rootless probe returns `rootless_sandbox_available` as `i2ptest`.
6. Source and reference inputs are tied to exact hashes.
7. Execution occurs offline under the documented enforcement mode.
8. All four directional scenarios produce valid sanitized records.
9. The aggregate checker accepts the matrix.
10. Evidence exports atomically and validates on the host.
11. The instance can be destroyed without losing evidence.
12. A fresh rebuild reproduces the environment contract.
13. A snapshot restore reproduces the environment contract.
14. The restricted original host remains a negative-baseline environment.
15. No support ledger row advances automatically.

## Closure policy

Create:

```text
plans/048-closure.md
```

only after the full external execution ladder succeeds.

The closure must include:

- exact Plan 048 implementation commits;
- Multipass version;
- guest image identity;
- environment-manifest digest;
- cloud-init digest;
- source commit and tree digest;
- reference revisions and cache-manifest digest;
- rootless probe result;
- offline enforcement mode;
- four directional outcomes;
- aggregate outcome;
- exported manifest digest;
- fresh-rebuild comparison;
- snapshot-restore comparison;
- destruction and host post-state result;
- explicit Milestone 3 and NTCP2 non-claims.

A typed blocker may be recorded in `plans/048-status.md`, but it does not satisfy this plan's environment-creation or protocol-evidence objective.
