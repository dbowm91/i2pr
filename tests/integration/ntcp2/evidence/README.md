# Sanitized Plan 038/040 evidence

No mixed-router run is recorded in this checkout. The `i2pr` daemon still
keeps live activation disabled and the dedicated `i2pr-interop` binary is only
a runtime/protocol composition seam; it reports a typed blocker until the
complete wire-level handshake/data-phase driver exists. Environment smoke and
reference crosscheck results validate the harness, not i2pr support.

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

The record must distinguish `passed`, `rejected`, `skipped_ipv6`,
`blocked_missing_driver`, `blocked_host_contract`, and `failed_cleanup`
outcomes. A run is not milestone evidence until both directions and both
required reference implementations have passed the applicable scenario matrix.
Raw run roots are deleted after sanitation, including keys, identities,
RouterInfo, configurations, logs, namespaces, and process state.
