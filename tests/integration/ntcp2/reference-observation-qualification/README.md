# Plan 059 reference observation qualification receipts

This directory hosts the machine-readable qualification records for
the pinned Java I2P 2.12.0 and i2pd 2.60.0 reference receiver
observations. Plan 059 Workstream D requires one qualification
record per reference, per semantic level. Each record carries:

- `reference`, `revision`, `semantic_level`
- `source_path`, `symbol`, `observation_kind`,
  `exact_marker_or_counter`
- `source_excerpt_sha256`
- `runtime_control_run_id`
- `positive_count`, `negative_control_count`
- `sanitization_rule`
- `qualified` boolean

## Current status

The local checkout produces the typed absence receipts only:

| Reference | File | Status |
| --- | --- | --- |
| i2pd 2.60.0 | `i2pd-2.60.0.json` | `blocked-runtime-demonstration-requires-external-lane` |
| Java I2P 2.12.0 | `java_i2p-2.12.0.json` | `blocked-runtime-demonstration-requires-external-lane` |

Both receipts carry `qualified = false` for every semantic level
because the runtime demonstration requires the Plan 046 rootless
sealed-namespace lane or the Plan 048/049 Multipass recovery lane.
The current host is the Plan 046 `apparmor_restrict_on` negative
baseline; the controls cannot be exercised on this host.

The Java receipt additionally records the Plan 058 ADR 0021
rejection. The Java support topology is forbidden under the current
four-direction contract; the `java-to-i2pr-ipv4` direction remains a
typed blocker. The Java runtime-control evidence requires a future
ADR-accepted topology or a different pinned Java revision.

## Summary

| Reference | Blockers | Qualified markers | Unqualified markers |
| --- | --- | --- | --- |
| i2pd 2.60.0 | `blocked_unprivileged_user_namespace` | 0 | 3 |
| Java I2P 2.12.0 | `blocked_java_support_topology_rejected` | 0 | 3 |

Plan 060 cannot claim receiver-observation qualification under these
receipts. The Plan 059 closure record
(`plans/059-status.md`) records this typed absence as the closure
contract.

## Required external controls

When the runtime qualification lane becomes available, the
qualification receipts must demonstrate the following controls:

- valid decrypt+decode positive (handshake + data frame + I2NP
  decode observed);
- handshake-only negative (no data frame observed);
- malformed-frame negative (AEAD failure observed);
- valid frame carrying invalid I2NP negative (decrypt observed,
  decode not observed);
- stale pre-run log marker ignored by the cursor;
- wrong correlation nonce/message ID does not satisfy the run;
- duplicate unrelated messages do not satisfy the correlation.

Receipt updates may not weaken or remove markers without a
follow-up plan that updates the catalog and the source-inspection
record.
