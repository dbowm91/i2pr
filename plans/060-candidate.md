# Plan 060 candidate record

Status: retired by Plan 062.

Plan 060 is no longer active execution authority. The Plan 060
candidate was `declared-not-executable` on this host and is now
retired by the Plan 062 evidence-contract and architecture
correction pass. Future candidates must descend from the Plan 065
implementation floor or later, must use the Plan 062 v4 trigger
schema, the Plan 062 reference-event v1 schema, the Plan 062 v3
observation schema, and the 64-hex SHA-256 Router Hash contract.

The historical Plan 060 candidate SHA was the implementation-floor
commit `359f408a73882bdd5bf03f21da4f4bd7e7feb878` (`interop: add
source-locked i2pd direct trigger helper`) plus every committed
Plan 058, Plan 059, and Plan 060 documentation, helper, and test
addition through the closure commit. That historical candidate is
preserved for audit only. It is **not** an `executed` candidate
and cannot produce a verified Milestone 3 certificate.

Plan 060 inherited the rejected Java support-topology premise
recorded by Plan 058 in `docs/adr/0021-minimal-java-support-topology.md`.
ADR 0021 (`Rejected`) and ADR 0022 (`Accepted`, Plan 062) supersede
that premise with the Plan 062 two-process direct transport driver
architecture. The Plan 060 declared-not-executable candidate was
frozen before the Plan 062 schema corrections, the 64-hex SHA-256
Router Hash contract, the v4 trigger schema, the reference-event
v1 schema, and the v3 observation schema were committed. A
candidate frozen before that implementation floor cannot be the
authoritative source for the four-direction Milestone 3 closure.

Plan 062 WP5 documents the retirement:

- the Plan 060 candidate is rejected as a future execution
  authority by the candidate record validator and the static
  boundary checker;
- the Plan 062 v4 trigger schema, v3 observation schema, and
  reference-event v1 schema are the only allowlisted active
  schemas for new bundles;
- the future candidate implementation floor is Plan 065 closure
  or later (per Plan 066);
- v3 trigger records and generic-phrase observations remain
  readable for historical inspection but cannot contribute to a
  new passing bundle.

The historical Plan 060 implementation surface
(`tests/integration/ntcp2/harness/plan060.py`,
`tests/integration/ntcp2/harness/test_plan060.py`, the Plan 060
static boundary checks) is preserved as an audit record. Any change
that re-enables Plan 060 as active execution authority must be
re-justified in a new plan-of-record.

The Plan 058 candidate record validator schema
(`i2pr-interop-candidate-v1`) and the Plan 060 declared-not-executable
status remain accurate historical records. The candidate cannot
produce a verified Milestone 3 certificate and is not an
`active_candidate_record`. NTCP2 remains experimental and
non-advertised; Milestone 3 stays open.

A future candidate may be cut only after the Plan 065
implementation floor closes, the Plan 046 rootless sealed-namespace
lane or the Plan 048/049 Multipass recovery lane becomes runnable,
and ADR 0021 is either Accepted by a future pinned Java revision or
superseded by an ADR that authorizes a different execution path.

## Implementation floor and executed source commit

The Plan 060 implementation floor is the Plan 059 closure
implementation commit:

```text
implementation_floor_commit = 359f408a73882bdd5bf03f21da4f4bd7e7feb878
```

The candidate frozen here descends from `359f408` (or a successor
commit) and may not inherit any pre-`359f408` source state.

The closure-record commit is the commit that lands the Plan 060
helper module, the Plan 060 test matrix, the static boundary
checker extension, and this closure document. The Plan 060
candidate record declares:

```text
executed_source_commit = 359f408a73882bdd5bf03f21da4f4bd7e7feb878
closure_record_commit = <the commit that lands plans/060-candidate.md
                          and plans/060-closure.md>
```

The executed source commit and the closure record commit are
recorded separately. A documentation successor commit is never
substituted for the executed source commit.

## Required measured digests

The Plan 060 candidate carries the bounded digest table that the
helper module produces. Every digest is the SHA-256 of the
committed artifact or the implementation-floor reference. The
qualification summary status is the typed `blocked` marker recorded
by the Plan 059 qualification seam.

```json
{
  "helper_source_lock_sha256": "4b784580888e6658b683d51f1d78d3b97876022fe788d1064a6d3abfa769a381",
  "helper_cpp_source_sha256": "c0716caf83eebd61da25688d2d9e0e0af81fbf7aa4f9e3d4abdbfe5f8a416af7",
  "helper_python_driver_sha256": "23fa67af296fe85cdb03cf267743d448e31d0e475127991c9fc2e60425ffe7a9",
  "observation_catalog_sha256": "92b8d2e23826877ad2e7f8b73d4f2cbc4bdd752bacebec6b90fdce43a75fb275",
  "i2pd_qualification_receipt_sha256": "e63d7cfda7390686e277c0d3b9022b4f28c11758e86965ad30cdcc5545b39561",
  "java_qualification_receipt_sha256": "70712c881eb011fec38e05ccc236b2697f00c66890c6a6fa6b6dcc4144cf0a6b",
  "qualification_summary_sha256": "dfb12a48fdbbce5741ddcb830de354d4da6827ec10ad06287d34b2997feae06f",
  "qualification_summary_status": "blocked",
  "references_lock_sha256": "943af1f7af3ba5f3df52c499cfd386be4b76cb2f650218c174981b114f4121ef"
}
```

## Lane lock

The Plan 060 plan-of-record requires exactly one execution lane to
be selected for a candidate. The Plan 058 two-lane contract
applies: a direct-host lane requires a positive direct-host probe
outcome; a guest lane requires a positive guest probe outcome
inside the Plan 048/049 Multipass guest.

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
rather than `rootless_sandbox_available`. The Plan 060 plan-of-record
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
  blocker for the pinned Java I2P 2.12.0 revision. Plan 059
  closed with this blocker; Plan 060 inherits it.

## Why this candidate is declared-not-executable

Plan 060 cannot freeze an executed source commit while:

1. ADR 0021 is Rejected. The four-direction contract cannot close
   against the pinned Java I2P 2.12.0 revision; the
   `java-to-i2pr-ipv4` direction is a typed blocker.
2. The Plan 046 rootless sealed-namespace lane returns
   `blocked_unprivileged_user_namespace` on this host.
3. The Plan 048/049 Multipass recovery lane is the canonical
   external path but cannot complete on this constrained host
   (per Plan 051).
4. The Plan 059 qualification receipts mark every reference
   observation as `qualified = false` because the runtime
   demonstration requires the external lane.

A pure environment failure may permit retry from the same candidate
only when no source/configuration/reference/helper/catalog digest
changed and the failed run never produced an accepted certificate
component. None of these conditions hold on this host; the
candidate is declared-not-executable and a future candidate must be
cut from a different source commit only after Plan 046 or the
Plan 048/049 lane is runnable.

## Why this candidate is not the Plan 056 candidate

The Plan 056 candidate (`fbf2cdb9ec12d35c7b7422c412e09d6db2d2d0cf`)
was retired by Plan 058 and is forbidden for future external
execution. The Plan 056 candidate was frozen before the Plan 059
reference-side implementation surface was implemented; a candidate
frozen before the implementation floor cannot be the authoritative
source for the four-direction contract.

The Plan 058 candidate record validator refuses the Plan 056
candidate (`status: retired`) for any Plan 060 tooling and refuses
the inherited Plan 057 execution contract. Plan 060 starts from the
Plan 059 implementation floor and may not inherit any pre-`359f408`
source state.

## Plan 060 candidate record schema marker

The Plan 060 candidate carries the Plan 058 schema marker for
auditability:

```text
schema = i2pr-interop-candidate-v1
```

The on-disk status is the explicit Plan 060 close-status
`declared-not-executable`. The Plan 058 validator's `declared`
enum is reserved for candidates that pass every Plan 060 freeze
invariant and may execute under either lane; this candidate does
not pass the freeze-readiness checklist on this host, so the
close-status is the Plan 060 typed absence marker.

## Status

The Plan 060 candidate is **retired** by Plan 062. The Plan 060
declared-not-executable record is preserved verbatim as an audit
trail. The closure record is `plans/060-closure.md`. NTCP2 remains
experimental and non-advertised; Milestone 3 remains open. A
future candidate may be cut only after the Plan 065 implementation
floor closes, the Plan 046 rootless sealed-namespace lane or the
Plan 048/049 Multipass recovery lane becomes runnable, and ADR 0021
is either Accepted by a future pinned Java revision or superseded
by an ADR that authorizes a different execution path.