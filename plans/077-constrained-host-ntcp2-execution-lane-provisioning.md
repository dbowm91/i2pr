# Plan 077: constrained-host NTCP2 execution lane provisioning

## Status and dependencies

- Status: closed with a typed no-full-runtime-lane result.
- Parent roadmap: Plan 074.
- Requires Plans 075 and 076 closed.
- Must close before Plan 078 live execution.
- Plan type: environment capability selection, isolated execution packaging, and lane prequalification.

## Objective

Provide one executable, reproducible lane for running the real i2pr and real pinned i2pd test driver without relying on unprivileged user/network namespaces or Multipass.

The lane must support both real process roles, host or guest loopback communication, no public-network reachability during execution, bounded state, sanitized result export, and deterministic cleanup.

## Known environmental constraints

Treat these as fixed unless a fresh capability probe disproves them:

- AppArmor/kernel policy blocks unprivileged user/network namespaces;
- rootless `unshare`, bubblewrap, rootless Podman/Docker, and user-level `PrivateNetwork` are not viable;
- Multipass is unreliable or unavailable under current resource/policy conditions;
- the runner must not invoke sudo or alter host security policy automatically;
- ordinary build preparation may occur separately from isolated execution.

## Required lane-selection order

### Lane A: existing accessible rootful Docker daemon

Probe:

```bash
docker info
```

Use only when the current user already has working daemon access. Do not install Docker, modify daemon configuration, or add group membership from the runner.

Execution design:

- build or load one pinned image during preparation;
- record immutable image digest;
- run both i2pr and i2pd inside one container;
- use `--network none` so the container contains loopback but no external interface;
- use a read-only root filesystem where practical;
- mount one bounded result directory or use a result volume;
- use tmpfs for mutable router state where practical;
- disable all unnecessary capabilities and set `no-new-privileges`;
- enforce CPU, memory, PID, and wall-clock limits;
- no package installation or source fetch during execution.

Both processes must share one container because separate `--network none` containers do not share loopback.

Required prequalification:

- container sees only loopback in the intended network namespace;
- no default route or DNS server usable for egress;
- both binaries and manifests match expected digests;
- a trivial two-process loopback TCP control passes;
- result export and cleanup pass;
- container cannot produce Level 3 authority merely because Docker is used.

### Lane B: QEMU TCG guest with no NIC

Use when Docker daemon access is unavailable.

Required design:

```text
qemu-system-<arch>
  -accel tcg
  -snapshot
  -nic none
```

The guest image must be prepared separately and contain all required binaries/libraries/scripts. The qualification boot must add no network device.

Prefer:

- a small Ubuntu/Debian guest matching the target architecture or a documented emulated architecture;
- serial console entry and result output;
- immutable base image with snapshot/disposable overlay;
- a small read-only input disk or initramfs containing exact artifacts;
- a dedicated result disk or serial-framed sanitized JSON output;
- explicit guest memory, CPU, and timeout bounds;
- no host networking backend, user networking, tap, bridge, or virtio-net device.

Required prequalification:

- guest enumerates only loopback and no non-loopback network interface;
- guest route table has no public/default route;
- both binaries and manifests verify in guest;
- loopback TCP control passes;
- serial/result extraction is deterministic;
- shutdown returns and disposable overlay is removed;
- repeated boot does not depend on external services.

### Lane C: inherited connected descriptors plus seccomp

This is a reduced-scope diagnostic lane, not full transport-manager qualification.

Use only when neither Lane A nor Lane B is available and an immediate protocol-level answer is valuable.

Design:

1. parent creates a loopback TCP connection and retains both connected endpoints;
2. parent passes one endpoint to i2pr and one to a test-only i2pd adapter;
3. both children set `PR_SET_NO_NEW_PRIVS`;
4. both install a seccomp filter denying new network creation/connection operations after descriptor adoption;
5. NTCP2 handshake and data phase run over inherited descriptors.

This lane may prove:

- SessionRequest/SessionCreated/SessionConfirmed interoperability;
- Noise transcript and authentication;
- encrypted framing and I2NP conversion;
- exact DeliveryStatus correlation.

It does not prove:

- normal bind/listen/connect transport-manager behavior;
- i2pd peer selection and address admission;
- duplicate-link policy through normal runtime plumbing.

Records must use a distinct lane/scope marker and cannot satisfy full Level 1 runtime validation without an explicit ADR update.

### Lane D: manually triggered remote Linux runner

Use when no local full-runtime lane exists.

Preferred implementations:

- a dedicated user-controlled Ubuntu VM/host;
- a manually triggered GitHub Actions `workflow_dispatch` job;
- another ephemeral Linux runner with documented isolation controls.

The remote lane must:

- pin exact repository commit and reference source;
- verify every artifact digest;
- prepare dependencies before execution isolation;
- run inside a root-owned network namespace with no public route, or one Docker `--network none` container;
- upload sanitized records only;
- avoid per-push/per-PR execution;
- remain optional and manually invoked.

Do not expose private keys, raw RouterInfo identity material beyond approved sanitized digests, Noise state, or raw protocol captures in public artifacts.

### Lane E: no executable full-runtime lane

When A, B, and D are unavailable, record:

```text
full_runtime_lane = unavailable
reduced_scope_lane = available|unavailable
reason_codes = [...]
```

Do not loop indefinitely through rootless namespace or Multipass recovery attempts.

## Deliverables

### D1. Capability probe

Create a short script, recommended:

```text
scripts/interop/probe-constrained-host-lanes.sh
```

It must inspect only; it must not install, configure, or mutate privileged services.

Report JSON with:

```text
docker_cli_present
docker_daemon_accessible
qemu_system_present
qemu_tcg_usable
seccomp_no_new_privs_supported
remote_workflow_present
selected_lane
reason_codes
```

Unknown or ambiguous results select no lane rather than guessing.

### D2. Common execution manifest

Define one compact manifest consumed by Docker/QEMU/remote entry scripts:

```text
source_commit
reference_revision
i2pr_binary_sha256
i2pd_binary_sha256
reference_build_manifest_sha256
direction
run_id
result_output
execution_timeout_seconds
```

No secret values. Unknown fields fail.

### D3. Lane-specific preparation and entry scripts

Keep scripts narrow. Recommended surfaces:

```text
scripts/interop/docker/build-ntcp2-interop-image.sh
scripts/interop/docker/run-ntcp2-interop-none.sh
scripts/interop/qemu/build-ntcp2-interop-guest.sh
scripts/interop/qemu/run-ntcp2-interop-no-nic.sh
scripts/interop/guest/run-ntcp2-interop-direction.sh
```

Only create scripts for the selected full-runtime lane plus reusable guest entry logic. Do not implement all possible lanes speculatively.

### D4. Lane prequalification record

Create a record such as:

```text
target/interop/lane/qualification.json
```

and at closure document sanitized results in `plans/077-status.md`.

Required fields:

```text
selected_lane
scope
host_or_image_metadata
artifact_digests
loopback_only_proven
no_public_interface_proven
control_connection_passed
result_export_passed
cleanup_passed
qualified
reason_code
```

## Work packages

### WP1. Probe and select

Run the capability probe once. Select the first viable full-runtime lane in required order. Record all skipped reasons.

### WP2. Package exact artifacts

Package only exact built binaries, libraries, manifests, source locks, runner scripts, and minimal runtime dependencies. Verify inside the lane.

### WP3. Prove no-public-network execution

Docker:

- inspect interfaces/routes/resolver;
- ensure `--network none`;
- reject unexpected interface.

QEMU:

- launch with `-nic none`;
- inspect guest interfaces/routes;
- reject any non-loopback interface.

Remote:

- record namespace/container commands and route/interface proof.

### WP4. Run non-protocol controls

- two-process loopback TCP control;
- process deadline control;
- forced cleanup control;
- result extraction control;
- immutable artifact digest verification.

### WP5. Documentation and closure

Create `plans/077-status.md` with exact selected lane and commands. Do not claim NTCP2 interoperability.

## Acceptance criteria

Plan 077 closes only when:

- Plans 075 and 076 artifacts are used;
- capability selection follows the required order;
- one full-runtime lane is qualified, or a truthful typed no-lane result is recorded;
- lane execution has no public network interface/route during the run;
- both real binaries can start inside the lane;
- exact artifact digests verify inside the lane;
- loopback TCP control, deadline, result export, and cleanup pass;
- no real NTCP2 pass is claimed;
- rootless namespace and Multipass retries are absent.

A typed no-lane result may close the planning implementation work but blocks Plan 078 until a lane is supplied.

## Validation commands

Exact commands depend on selected lane. At minimum:

```bash
bash scripts/interop/probe-constrained-host-lanes.sh
bash scripts/check-ntcp2-loopback-smoke-boundary.sh
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py'
cargo fmt --all --check
cargo check --workspace --all-targets
cargo test --workspace
git diff --check
```

## Non-goals

Plan 077 does not:

- repair the i2pd driver;
- repair runner event semantics;
- execute the real interoperability qualification;
- enable public networking;
- add broad CI;
- install Docker or QEMU automatically with privilege;
- weaken Level 3 release requirements.

## Stop rules

Stop and record a blocker when:

- Docker requires privileged host changes not already available;
- QEMU cannot run under TCG or resource limits are inadequate;
- result export requires adding a guest network device;
- remote execution cannot protect secrets or guarantee no public route;
- lane packaging changes the tested binary after digest verification.

## Small-model execution guidance

- Probe before writing scripts.
- Implement only the selected full-runtime lane.
- Keep preparation and execution separate.
- Verify network absence inside the container/guest, not only from host command arguments.
- Run TCP controls before NTCP2.
- Never treat the inherited-descriptor lane as equivalent to normal listener/dialer runtime.
