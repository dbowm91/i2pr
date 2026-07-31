# Plan 066 candidate record

Status: declared-not-executable.

Plan 066 is the fresh-candidate and authoritative NTCP2 two-run
closure pass. The plan starts only after Plan 065 closes with one
complete independently verified four-direction live diagnostic
bundle. The Plan 066 candidate is descended from the Plan 065
implementation floor and inherits the corrected Plan 062 v4 trigger
schema, reference-event v1 schema, and v3 observation schema, the
qualified Plan 063 Java driver, the qualified Plan 064 i2pd driver
with its observer patch, and the Plan 065 canonical mixed-runner
with the per-run DeliveryStatus correlation counters.

On this host (the Plan 046 `apparmor_restrict_on` negative baseline
plus the Plan 051 resource constraints) the Plan 046 rootless
sealed-namespace probe returns `blocked_unprivileged_user_namespace`
and the Plan 048/049 Multipass recovery lane cannot complete. ADR
0021 was Rejected by Plan 058, so the `java-to-i2pr-ipv4` direction
remains a typed blocker for the pinned Java I2P 2.12.0 revision
under the current four-direction contract. Plan 066 therefore closes
on this host with the typed environment blocker
`blocked_execution_lane_unavailable` and the candidate is
`declared-not-executable`. The Plan 066 candidate record is
preserved verbatim for audit.

The Plan 060 implementation surface remains mandatory; any change
that re-enables Plan 060 as active execution authority must be
re-justified in a new plan-of-record. The Plan 066 implementation
surface is also mandatory; any change that removes or weakens the
Plan 066 helper module, the Plan 066 test matrix, the static
boundary checker extension, or the freeze-readiness invariants
must be re-justified in a new plan-of-record and must not silently
weaken the Milestone 3 evidence gate.

## Implementation floor and executed source commit

The Plan 066 implementation floor is the Plan 065 closure
implementation commit:

```text
implementation_floor_commit = 450c0cf2fc1e015ce052e0387723d6c83b3cd746
```

The `executed_source_commit` is the same implementation-floor commit
on this host. The candidate record carries the exact committed
state; the documentation successor commit is the closure-record
commit and is recorded separately in `plans/066-closure.md`. No
documentation successor commit is substituted for the executed
binary source.

```text
executed_source_commit = 450c0cf2fc1e015ce052e0387723d6c83b3cd746
closure_record_commit = <the commit that lands plans/066-candidate.md
                          and plans/066-closure.md>
```

The candidate carries the Plan 058 candidate record schema marker:

```text
schema = i2pr-interop-candidate-v1
```

The on-disk status is the explicit Plan 066 close-status
`declared-not-executable`. The Plan 058 validator's `declared` enum
is reserved for candidates that pass every Plan 066 freeze
invariant and may execute under either lane; this candidate does not
pass the freeze-readiness checklist on this host, so the close-status
is the Plan 066 typed absence marker.

## Required measured digests

The Plan 066 candidate carries the bounded 23-row digest table that
the helper module `tests/integration/ntcp2/harness/plan066.py`
produces. Every digest is a full 64 lowercase hex SHA-256 of the
committed artifact or the implementation-floor reference. The
qualification summary status is the typed `blocked` marker recorded
by the Plan 059 qualification seam and propagated through Plan 060.

```json
{
  "java_driver_source_sha256": "221a7ad85ef7a40e632b38a03208ebc957c100cbc5040d51aacfd2f83e4384f7",
  "java_driver_classpath_manifest_sha256": "9e6ac79de60ed418343f8eda0e0951b6c41366836371cdcaf8827586045b6b63",
  "java_driver_source_lock_sha256": "71ac76a84da13bc824d6de351078ec233cb413cfc2130507a2eb00f7e451748b",
  "java_qualification_receipt_sha256": "a5eec42773d2860df4fe3e9043bd62d702a02a433d667a75642d562ae7738dd9",
  "i2pd_driver_cpp_source_sha256": "b81e1822ac0c94a59a593bc67d078fed92cf2488f47ef159bafb7b1785144ff9",
  "i2pd_observer_header_sha256": "77ca510439ac430362f15a0dd0006318963d22810d91f077ee20e7a13066d12b",
  "i2pd_observer_source_sha256": "1be6e5db85aed063ba5d88aa252801b1e26275a0120c5e21628def0c2cbf6b28",
  "i2pd_observer_patch_sha256": "5a09558238cb898272fae38a717a2f02e05d561c5ffa1f8d85084eeb790bc1d1",
  "i2pd_driver_source_lock_sha256": "b0fc554be33b2aee11b6845fd50df24bf82f747dc02bbe6b4e8eb250f5d357c5",
  "i2pd_qualification_receipt_sha256": "2f93bb4e15e4dea4b89beafbad89f44fc373b305b59194e603a35993369f8795",
  "scenario_renderer_sha256": "7bd214f6ed3dffbe9177301ee32d6dab9dc0ffbdda085f49ce83bef6c2bbb809",
  "main_runner_sha256": "db22a81caf7cb38dbcf6d73d7f47b465faa26c2ea44b0b16e89d3953f6daf9ee",
  "status_module_sha256": "fb88f74f12e7887aeddd30f5ad3267ad97a6da29ea27b7ff7159efea8f033ee0",
  "launcher_protocol_sha256": "0736bc3eddc4f89ee0ff5c25374d2b27acdc35b08589d200f19653eca238c585",
  "launcher_renderer_sha256": "fec80d3a728cbc1c14b89f0590512be9807626299274946b2c06d63f413a48e4",
  "mixed_runner_sha256": "5bbae4372d1c37702093b116b509c3b042b7bf30581f675c8231e31a31df4b2d",
  "plan052_pipeline_sha256": "debf8f6d30cdaa38bd22333ae279061c6e35c1f5d428a56d39e076a784e3c448",
  "run_identity_sha256": "050fc6ab2becb23e84d1290fd0a52583db3b7b93b921bc2264948bbf9d0de965",
  "evidence_bundle_sha256": "37123915490da3c0ccec13f1d9bba2c66e6d73cb413b739eb0ff15cee31f8d9a",
  "bundle_verifier_sha256": "4614450d22adb67f9b701f2a7b1424acec6354e3935e61ea4f650389bb175d21",
  "references_lock_sha256": "943af1f7af3ba5f3df52c499cfd386be4b76cb2f650218c174981b114f4121ef",
  "plan059_helper_cpp_source_sha256": "0276da134f0ffdada01f3bc618c93677ed1310c0aac18911f32cd6d1dbc6476c",
  "plan059_helper_python_driver_sha256": "23fa67af296fe85cdb03cf267743d448e31d0e475127991c9fc2e60425ffe7a9"
}
```

No digest in this table equals the typed-absence placeholder
`"0" * 64` because the Plan 066 helper module deliberately
substitutes an environment marker when a file is absent rather
than writing the placeholder; every Plan 066 candidate field is
the SHA-256 of the on-disk artifact.

## Lane lock

The Plan 066 plan-of-record inherits the Plan 058/060 two-lane
contract and requires exactly one execution lane to be selected for
a candidate. A direct-host lane requires a positive direct-host
probe outcome; a guest lane requires a positive guest probe outcome
inside the Plan 048/049 Multipass guest. Cross-lane combinations
are forbidden.

The lane lock for this host is:

```json
{
  "lane_kind": "guest",
  "outer_host_baseline": "blocked_unprivileged_user_namespace",
  "guest_probe_outcome": "blocked_execution_lane_unavailable",
  "direct_host_probe_outcome": "",
  "environment_manifest_sha256": "0000000000000000000000000000000000000000000000000000000000000000",
  "vm_manager_version": "multipass-1.16.3",
  "notes": "Plan 048/049 Multipass guest cannot complete on this constrained host (per Plan 051)."
}
```

The guest probe outcome is `blocked_execution_lane_unavailable`
rather than `rootless_sandbox_available`. The Plan 066 plan-of-record
explicitly requires `rootless_sandbox_available` immediately before
any router process; this host cannot reach that state, so the
candidate is `declared-not-executable`.

## Typed blockers carried by this candidate

- `blocked_execution_lane_unavailable` — the Plan 046 direct-host
  probe and the Plan 048/049 Multipass guest probe both return
  typed blockers on this host (Plan 046 host-side
  `apparmor_restrict_on` negative baseline plus Plan 051 resource
  constraints).
- `blocked_java_support_topology_rejected` — ADR 0021 was Rejected
  by Plan 058; the `java-to-i2pr-ipv4` direction remains a typed
  blocker for the pinned Java I2P 2.12.0 revision. Plan 059 closed
  with this blocker; Plan 060 retired it forward; Plan 066 inherits
  it.

## Why this candidate is declared-not-executable

Plan 066 cannot freeze an executed source commit while:

1. ADR 0021 is Rejected. The four-direction contract cannot close
   against the pinned Java I2P 2.12.0 revision; the
   `java-to-i2pr-ipv4` direction is a typed blocker.
2. The Plan 046 rootless sealed-namespace lane returns
   `blocked_unprivileged_user_namespace` on this host.
3. The Plan 048/049 Multipass recovery lane is the canonical
   external path but cannot complete on this constrained host
   (per Plan 051).
4. The Plan 063 and Plan 064 qualification receipts mark every
   reference observation as `qualified = false` because the runtime
   demonstration requires the external lane.

A pure environment failure may permit retry from the same candidate
only when no source/configuration/reference/driver/schema/observer/
verifier digest changed and the failed run never produced an accepted
certificate component. None of these conditions hold on this host;
the candidate is declared-not-executable and a future candidate must
be cut from a different source commit only after Plan 046 or the
Plan 048/049 lane is runnable.

## Why this candidate is not the Plan 056 or Plan 060 candidate

The Plan 056 candidate (`fbf2cdb9ec12d35c7b7422c412e09d6db2d2d0cf`)
was retired by Plan 058 and the Plan 060 candidate was retired by
Plan 062; both are forbidden for future external execution. Both
were frozen before the Plan 065 implementation floor and the
corrected Plan 062 v4 trigger schema, reference-event v1 schema,
and v3 observation schema were committed. The Plan 058 candidate
record validator refuses both retrospective candidates for any
Plan 066 tooling. Plan 066 starts from the Plan 065 implementation
floor and may not inherit any pre-`450c0cf` source state.

## Status

The Plan 066 candidate is **declared-not-executable**. The
Plan 066 declared-not-executable record is preserved verbatim as
an audit trail. The closure record is `plans/066-closure.md`. NTCP2
remains experimental and non-advertised; Milestone 3 remains open.
A future candidate may be cut only after the Plan 066 implementation
floor closes, the Plan 046 rootless sealed-namespace lane or the
Plan 048/049 Multipass recovery lane becomes runnable, and ADR 0021
is either Accepted by a future pinned Java revision or superseded
by an ADR that authorizes a different execution path.
