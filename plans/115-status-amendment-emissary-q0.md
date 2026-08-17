# Plan 115 status amendment: Emissary Q0 re-open

- Date: 2026-08-17.
- Status: **reopened-targeted-completion-emissary-q0-pending**.
- Corrective plan:
  [`plans/115-completion-emissary-native-q0.md`](115-completion-emissary-native-q0.md).
- Historical evidence:
  [`plans/115-status.md`](115-status.md).

This amendment supersedes only the historical Branch E conclusion that this
host has no bounded independent short-build consumer. The Plan 115 bridge and
local validation evidence remain valid.

Pinned upstream Emissary:

```text
repository = https://github.com/eepnet/emissary.git
revision   = 9b43484a21d5a1291c4881cdae62a36c527f8c0f
package    = emissary-core 0.4.0
```

That revision already exposes the needed test-only native path:

```text
TestTransitTunnelManager<MockRuntime>
 -> TransitTunnelManager::handle_short_tunnel_build
```

The handler performs target-record lookup, Noise-N request decryption, short
request parsing, admission, key derivation, reply-record transformation, and
OBEP reply construction. Existing Emissary tests invoke it without a daemon or
network transport.

Current authority:

```text
plan_111_to_114             = retained-passed
plan_115_local_bridge       = passed
plan_115_external_q0        = reopened-emissary-native-consumer-pending
short_build_local_outbound  = strict-established
short_build_local_inbound   = strict-established
Q1                          = deferred
Q2                          = deferred
normal_daemon_ntcp2         = disabled-and-unenableable
ntcp2                        = experimental-non-advertised
```

Continuation rule:

```text
Q0 pass                         -> Plan 116 unblocked
native protocol defect          -> one narrow correction -> same Q0 -> Plan 116
reference/build blocker only    -> external evidence deferred -> Plan 116 unblocked
```

Only a demonstrated native protocol defect may temporarily block Plan 116.
Namespace/VM/transport limitations may defer interoperability claims but must
not stop transport-neutral tunnel-data-plane implementation.
