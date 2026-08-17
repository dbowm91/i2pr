# Plan 115 status amendment: Emissary Q0 re-open

- Date: 2026-08-17.
- Status: **reopened-targeted-completion-emissary-q0-pending**.
- Corrective plan:
  [`plans/115-completion-emissary-native-q0.md`](115-completion-emissary-native-q0.md).
- Historical closure record:
  [`plans/115-status.md`](115-status.md).

## Authority

This amendment supersedes only the Plan 115 conclusion that the current
environment has no bounded independent short-build consumer seam.

The implementation and evidence recorded in `plans/115-status.md` remain valid
for the canonical i2pr I2NP bridge and local validation. Its Branch E gate is no
longer authoritative because the allowed Emissary fallback was not exhausted.

Current authoritative state:

```text
plan_111                          = retained-core-crypto-corrected
plan_112                          = passed-outbound-pre-delivery-closure
plan_113                          = passed-inbound-reference-reconciliation
plan_114                          = passed-terminal-routing-chain-correction
plan_115_local_bridge             = passed
plan_115_external_q0              = reopened-emissary-native-consumer-pending
short_build_local_outbound        = strict-established
short_build_local_inbound         = strict-established
canonical_i2np_bridge             = locally-conformant-no-double-prefix
qualified_live_delivery           = deferred
normal_daemon_ntcp2               = disabled-and-unenableable
ntcp2                              = experimental-non-advertised
```

## Corrected reference finding

Pinned upstream Emissary revision:

```text
repository = https://github.com/eepnet/emissary.git
revision   = 9b43484a21d5a1291c4881cdae62a36c527f8c0f
package    = emissary-core 0.4.0
```

At that revision, Emissary already contains a bounded native short-build test
seam:

- `emissary-core/src/tunnel/tests/mod.rs` provides
  `TestTransitTunnelManager<MockRuntime>` and exposes its router hash and X25519
  public key to unit tests.
- `TestTransitTunnelManager::handle_short_tunnel_build()` delegates directly to
  the production `TransitTunnelManager::handle_short_tunnel_build()` method.
- The production handler performs native target-record lookup, Noise-N request
  decryption, short request parsing, admission, key derivation, reply-record
  transformation, and forwarding/OBEP reply construction.
- Existing Emissary unit tests invoke this path without a daemon, public router,
  network namespace, VM, or authenticated transport session.

This is the bounded consumer Plan 115 originally permitted.

## Corrected continuation rule

Plan 116 local tunnel-data-plane construction must not remain blocked on an
environment-dependent live transport lane.

After executing the bounded Emissary Q0 pass:

```text
Q0 passes
  -> Plan 116 local data plane unblocked

Q0 reaches native Emissary processor and localizes a protocol defect
  -> one narrow protocol correction
  -> rerun the same focused Q0
  -> Plan 116

Q0 cannot execute because of reference build/tooling limits after the fixed
attempt budget, without demonstrating a protocol defect
  -> external Q0 evidence deferred
  -> Plan 116 local data plane unblocked
  -> mixed-router milestone exit remains blocked
```

Q1 authenticated transport delivery and Q2 live return-to-Established are
explicitly deferred from this corrective pass. They are not prerequisites for
implementing the local transport-neutral tunnel data plane.

## Non-regression constraints

This amendment does not authorize:

- NTCP2 changes;
- SSU2 work;
- daemon transport activation;
- namespace/container/VM work;
- another i2pd or Java adapter;
- Python harness reconstruction;
- public-network testing;
- changes to Plans 109-114 crypto without a native reference-localized defect.

The next executor should start with
`plans/115-completion-emissary-native-q0.md`, not with the historical Branch E
successor language in `plans/115-status.md`.
