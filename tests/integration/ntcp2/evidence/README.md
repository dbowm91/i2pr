# Sanitized Plan 038/040/041 evidence

No mixed-router run is recorded in this checkout. The `i2pr` daemon still
keeps live activation disabled and the dedicated `i2pr-interop` binary is only
a runtime/protocol composition seam: its listen/dial operations report a typed
blocker until the complete wire-level handshake/data-phase driver exists, while
its inspect operation is limited to strict disposable RouterInfo validation.
Environment smoke and reference crosscheck results validate the harness, not
i2pr support. Plan 041
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
