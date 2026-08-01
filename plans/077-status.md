# Plan 077 closure — constrained-host NTCP2 execution lane provisioning

## Status

Plan 077 closes on the local host twice. The first close (this section) is
the truthful typed no-full-runtime-lane result: the probe selected the
reduced-scope inherited-descriptor capability, but that capability cannot
qualify normal listener/dial transport-manager behavior. The second close
is a Lane D qualification through the Plan 048/049/050 Multipass recovery
lane, recorded in the `## Lane D qualification` section below.

The original no-full-runtime-lane result (preserved as the historical
plan-077 close status):

```text
full_runtime_lane = unavailable
reduced_scope_lane = available
selected_lane = inherited-descriptors-seccomp
reason_code = full_runtime_lane_unavailable
qualified = false
```

The new Lane D qualification recorded on 2026-08-01 by Plan 080
(`plans/080-status.md`):

```text
selected_lane = remote-manual
scope = full-runtime
full_runtime_lane = available
qualified = true
reason_code = lane-qualified
plan080_qualification_record_sha256 = 2ba91fe1a7f2c2309359d8e3fb2de199fac765b387272ef13d668a7ddb14063d
```

The two records coexist because the no-full-runtime-lane result is the
historical truth of the Plan 077 implementation work and the
Lane D qualification is the truth of the Plan 080 follow-up. The
qualification record at `target/interop/lane/qualification.json` always
holds the latest measured state; older snapshots are preserved for
audit.

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

## Lane D qualification (2026-08-01, recorded by Plan 080)

The Plan 080 follow-up promotion of the Plan 077 Lane D Multipass
recovery lane to a qualified full-runtime lane is recorded here. The
qualification record digest is the canonical close-status for the
Plan 080 lane pass; the direction result is a separate concern tracked
under `plans/080-status.md` and `plans/078-status.md`.

```text
plan080_qualification_record_sha256 = 2ba91fe1a7f2c2309359d8e3fb2de199fac765b387272ef13d668a7ddb14063d
plan080_qualification_path = target/interop/lane/qualification.json
plan080_guest_instance = i2pr-interop-plan080-20260801034138-c8bec3f5-g1-a1
plan080_guest_probe_outcome = rootless_sandbox_available
plan080_artifact_digests =
  i2pd_ntcp2_interop_driver_instrumented = f7ba4578c497a7af977163e16a1d2491aabdb451e7f19657ec3ac60fdb44ce80
  i2pd_ntcp2_interop_driver_control      = c948a01370c352677ae390ac7802a2bf9c98095d1fdef3b6e1d49561fc7902af
  i2pr_interop_host_binary               = 241b008621324f0e0349f654a119aa62cbdd1997cf490c757320ad853b596224
  host_i2pd_cache_sha256                 = 6f023032cc5c5f1306ca8bbc9358307717d5ef63702d554f2ae8661739507126
  host_cache_manifest_sha256             = f98d4c716b5345beb88ff4cab115fd4c5afd6d1612187db418352abe7b9a8713
```

The Lane D qualification does not promote Plan 077 to a passing release
state. The host remains on the Plan 046 negative baseline; the
qualified record is bound to the owned Multipass guest whose lifecycle
is bound to a sanitized environment manifest and an ownership token.
Plan 078 closed as `blocked-protocol-defect` on the same day; Plan 079
remains blocked. The Multipass guest is preserved for inspection via
`run-evidence-lane.sh --inspect` and may be re-entered by a future plan
that fixes the i2pr-launcher pre-protocol defect.
