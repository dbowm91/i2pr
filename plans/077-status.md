# Plan 077 closure — constrained-host NTCP2 execution lane provisioning

## Status

Plan 077 closes on the local host with a truthful typed no-full-runtime-lane
result. The probe selected the reduced-scope inherited-descriptor capability,
but that capability was not executed or promoted: it cannot qualify normal
listener/dial transport-manager behavior. The manual remote workflow is
present as an optional candidate, but it has not been remotely qualified.

```text
full_runtime_lane = unavailable
reduced_scope_lane = available
selected_lane = inherited-descriptors-seccomp
reason_code = full_runtime_lane_unavailable
qualified = false
```

This closes the Plan 077 provisioning and selection implementation work. Plan
078 remains blocked until a full-runtime lane is qualified. No NTCP2 protocol
run, Level 1 pass, Level 2 result, or Level 3 certificate is claimed.

## Capability probe

The read-only probe is:

```text
bash scripts/interop/probe-constrained-host-lanes.sh
```

It writes the ignored, sanitized records:

```text
target/interop/lane/probe.json
target/interop/lane/qualification.json
```

The local result recorded at closure was:

```json
{
  "docker_cli_present": true,
  "docker_daemon_accessible": false,
  "qemu_system_present": false,
  "qemu_tcg_usable": false,
  "seccomp_no_new_privs_supported": true,
  "remote_workflow_present": true,
  "selected_lane": "inherited-descriptors-seccomp"
}
```

The Docker daemon was not accessible to the invoking user, QEMU system
emulation was absent, and the existing remote workflow is manual-only and
has no Plan 077 qualification receipt. Rootless namespace, bubblewrap,
rootless container, user-level private-network, and Multipass recovery paths
were not retried.

## Implemented contracts

- `execution_lane.py` owns the fail-closed probe, required lane ordering,
  strict common execution manifest, and sanitized qualification schema.
- The common manifest requires the exact source commit, reference revision,
  measured i2pr/i2pd/build-manifest digests, one allowlisted direction, a
  bounded run ID and relative result path, and a bounded timeout. Unknown
  fields and absolute/traversal paths are rejected.
- The qualification schema cannot mark a reduced-scope or unavailable lane
  as qualified. Empty artifact-digest tables are used for the no-run result;
  no synthetic digest is emitted.
- `check-constrained-host-lane-boundary.sh` verifies the implementation and
  forbids privilege escalation, host-network access, and Docker-socket use
  in the probe.
- No Docker/QEMU/remote entrypoint was added speculatively. The selected
  reduced-scope capability has no full-runtime packaging surface yet.

## Validation

```text
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py'
bash scripts/check-constrained-host-lane-boundary.sh
bash scripts/interop/probe-constrained-host-lanes.sh
```

The focused Plan 077 test matrix passes. The boundary check passes. The probe
and qualification records are generated under the ignored target directory;
the qualification is `qualified = false` with
`full_runtime_lane = unavailable`.

The broader repository validation and corrections are recorded in the change
handoff. NTCP2 remains experimental and non-advertised, and the Plan 078
precondition remains unsatisfied by design.
