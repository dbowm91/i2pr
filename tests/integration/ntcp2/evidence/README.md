# Sanitized Plan 038/040/041/043/044 evidence

No mixed-router run is recorded in this checkout. The `i2pr` daemon still
keeps live activation disabled. The dedicated `i2pr-interop` binary now
contains the bounded local runtime/protocol composition seam, including
listener/dial, authenticated-link promotion, and DeliveryStatus smoke; local
success remains driver validation only. Its inspect operation is limited to
strict disposable RouterInfo validation. Environment smoke and reference
crosscheck results validate the harness, not i2pr support. Plan 041
reference-pair records are control evidence only and do not replace the four
later i2pr-to-reference direction gates.

This tracked directory remains a documentation boundary. Authorized runs
write records only under `target/interop/evidence/`; never copy a run root or
raw artifact here. Retain only a JSON record with the
fields below and SHA-256 hashes of sanitized artifacts/configuration. Do not
commit the logs, captures, RouterInfo values, identities, addresses, private
keys, static keys, ephemeral keys, or payload bytes. The
`scripts/interop/validate-evidence.py` helper validates records mechanically;
no record is treated as success
merely because the directory is empty.

```text
schema|scenario_id|date_utc|i2pr_commit|reference|reference_version|reference_revision|artifact_sha256|installed_tree_sha256|configuration_sha256|namespace_topology_sha256|direction|address_family|deterministic_parameters|expected|actual_typed_result|resource_counters|process_counters|cleanup_result|evidence_sha256|known_deviation|reproduction
```

Plan 041 reference-pair records use schema 2:

```text
schema|scenario_id|date_utc|i2pr_commit|java_reference|java_version|java_revision|java_artifact_sha256|java_installed_tree_sha256|java_configuration_sha256|i2pd_reference|i2pd_version|i2pd_revision|i2pd_artifact_sha256|i2pd_installed_tree_sha256|i2pd_configuration_sha256|namespace_topology_sha256|private_network_id|direction_policy|router_info_validation|authenticated_link_observations|connection_counters|process_counters|expected_authenticated_link_count|actual_typed_result|cleanup_result|evidence_sha256|known_deviation|reproduction
```

The record must distinguish `passed`, `rejected`, `blocked`, `skipped_ipv6`,
`blocked_host_contract`, and `failed_cleanup`
outcomes. A run is not milestone evidence until both directions and both
required reference implementations have passed the applicable scenario matrix.
Raw run roots are deleted after sanitation, including keys, identities,
RouterInfo, configurations, logs, namespaces, and process state.

## Plan 043 aggregate manifest

The Plan 043 evidence gate consumes one sanitized `run-manifest.json` for the
lane. It must contain the schema version, i2pr commit, workflow run and
attempt, host-contract digest, lock digest, reference cache keys, artifact and
installed-tree hashes, expected scenario IDs, actual record filenames with
SHA-256 digests, per-gate dispositions, cleanup-verification disposition, and
the aggregate manifest digest. Every referenced JSON record is validated before
aggregation.

Validation fails closed when an expected record is absent, an unexpected record
is marked passed, a placeholder is present, a hash disagrees with build/cache
metadata, direction coverage is incomplete, cleanup is not clean (or an
explicitly allowed forced-cleanup negative test), or forbidden content appears
in the retained tree. The allowlist is limited to sanitized JSON records,
`target/interop/build/reference-build-summary.json`, and the aggregate
manifest. Do not upload source trees, cache directories, rendered configs,
run roots, raw logs, packet captures, RouterInfo, identities, keys, endpoints,
payloads, or absolute private paths.

Cleanup verification is a separate terminal disposition. `cleanup.sh` removes
owned processes, prefixed namespaces/veths, and secret-bearing run roots;
`verify-clean-host.sh` must independently reject any residual interop state,
forbidden retained file, or attributable global nftables/route/forwarding
change. A protocol pass with failed cleanup verification is not a pass.

The current checkout has no completed aggregate manifest or mixed-router
record. The shared worktree contains the clean-host verifier helper, but the
workflow has not yet produced an integrated aggregate run and verified
terminal disposition; these are Plan 043 blockers, not skipped successes.

## Plan 044 mixed evidence

Plan 044 mixed-router evidence extends schema-1 records with real counters
for authenticated-link count, frames sent/received, I2NP message aggregates,
admission/replay counters, process lifecycle counters, and cleanup
disposition. Gate archival uses gate-specific staging to prevent cross-gate
record relabeling. The aggregate manifest must include exactly the expected
records for the selected profile; missing, extra, mislabeled, or zero-valued
records fail the gate.

No completed mixed-router i2pr record is present in this checkout. These are
explicit blockers, not skipped successes.

## Plan 046 rootless sealed-namespace evidence

Plan 046 adds four fields to the mixed-router evidence record:
`topology_kind`, `privilege_model`, `sandbox_attestation_sha256`, and
`parent_network_state_unchanged`. The schema-2 field list is unchanged
otherwise. A passed record that violates any of the four new fields is
rejected.

The Plan 046 lane produces an `IsolationAttestation` containing the
sandbox sha256, the parent-network state digest pre/post, the kernel /
sysctl / capability snapshot at probe time, and the bounded probe
results. The sha256 of the attestation binds every passed mixed-router
record in the same run; aggregate validation requires all four
directional records to share the same attestation sha256 and byte-equal
parent-network-state pre/post digests.

Plan 046 closed with a typed host-level blocker on this checkout. The
on-host evidence directory `target/interop/evidence/handshake-smoke-rootless--host-blocked/`
records the kernel/sysctl/capability snapshot and the two identical
probe attestations (host shell and `ssh i2ptest@localhost` shell)
carrying `{"schema":1,"type":"rootless-sandbox-probe","outcome":"blocked_unprivileged_user_namespace"}`.
The Plan 046 closure is `plans/046-closure.md`; cross-host recovery
lives in `plans/047-cross-host-rootless-lane-expansion.md`.

## Plan 048/049 Multipass bundle

Plan 048 keeps the host blocker as a negative baseline and runs the recovery
lane inside a disposable Ubuntu 24.04 amd64 Multipass guest. Plan 049 requires
the environment record to identify the stable environment contract separately
from the run ID, concrete instance name digest, and instance generation. It
also links the lifecycle schema, ownership record, environment manifest,
cloud-init, exact source/tree, and canonical `target/interop/cache` hashes.
The guest runs as `i2ptest` only after ownership/policy checks, the guest probe,
and offline egress policy pass; the final guest probe is mandatory before
routers start. The host baseline probe is recorded separately and cannot
substitute for the guest result.

The sanitized bundle includes the environment record, lifecycle record, probe
records, their approved sidecar hashes, four directional records,
`aggregate.json`, and `manifest.json` only when the allowlist and current
schema permit them. Lifecycle state is authoritative for run/generation
attribution. Adoption, resume, recreation, destruction, and inspection are
explicit; a name-only match, unknown state, ownership mismatch, or
deleted-but-unpurged instance is a blocker. Global purge and silent mutation
are forbidden.

The exporter rejects unsafe filesystem objects, oversized or unexpected files,
hash mismatches, mixed run IDs or generations, missing environment attribution,
non-clean cleanup, and any direction that does not satisfy the existing Plan
045/046 pass predicates. A pre-router failure is written as sanitized
`environment-blocker.json`; it is not a protocol record and cannot satisfy
interoperability closure. The host evidence directory survives owned VM
destruction. No bundle advances NTCP2 support or Milestone 3 by itself.
