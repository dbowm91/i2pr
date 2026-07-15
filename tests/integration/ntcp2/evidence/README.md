# Sanitized Plan 036 evidence

No mixed-router run is recorded in this checkout. The `i2pr` daemon still
keeps live activation disabled and Plan 035 exposes ownership/I/O seams rather
than a complete wire-level handshake/data-phase driver. The required Java I2P
and i2pd lanes therefore remain a recorded blocker, not a skipped success.

When an authorized operator runs the lane, retain only a tab-separated record
with the header below and SHA-256 hashes of sanitized logs/configuration. Do
not commit the logs, captures, RouterInfo values, identities, addresses,
private keys, static keys, ephemeral keys, or payload bytes.

```text
scenario_id|date_utc|i2pr_commit|reference|reference_version|reference_revision|artifact_sha256|configuration_sha256|direction|address_family|padding_profile|expected|actual_typed_result|evidence_sha256|known_deviation|reproduction
```

The record must distinguish `passed`, `rejected`, `skipped_ipv6`, and
`blocked_missing_driver` outcomes. A run is not milestone evidence until both
directions and both required reference implementations have passed the
applicable scenario matrix.
