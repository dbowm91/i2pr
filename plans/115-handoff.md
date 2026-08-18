# Plan 115 handoff

- Status: **passed-emissary-q0-construction-and-obep-reply-only**
- Q0 completion date: **2026-08-18**
- Plan-of-record:
  [`plans/115-qualified-independent-short-build-consumption-and-external-delivery.md`](115-qualified-independent-short-build-consumption-and-external-delivery.md)
- Corrective/completion plan:
  [`plans/115-completion-emissary-native-q0.md`](115-completion-emissary-native-q0.md)
- Closure authority:
  [`plans/115-status.md`](115-status.md)
- Immediate successor:
  [`plans/116-local-tunnel-data-plane.md`](116-local-tunnel-data-plane.md)
- Successor handoff:
  [`plans/116-handoff.md`](116-handoff.md)

## Closure result

Pinned upstream Emissary at revision
`9b43484a21d5a1291c4881cdae62a36c527f8c0f` independently consumed the exact
production-generated i2pr short-build message:

```text
ShortBuildStateMachine::prepare
 -> deliver_action
 -> ShortBuildI2npBridge
 -> standard I2NP type 25
 -> Emissary Message::parse_standard
 -> TestTransitTunnelManager::handle_short_tunnel_build
 -> native OBEP TunnelGateway + Garlic reply path
```

This is sufficient Q0 independent-consumer evidence to continue local Milestone
5 router construction.

Q1 authenticated router transport and Q2 returned live reply-to-`Established`
remain deferred. They are integration evidence for the later live checkpoint,
not prerequisites for Plan 116.

## Current authoritative state

```text
plan_111                         = retained-core-crypto-corrected
plan_112                         = passed-outbound-pre-delivery-closure
plan_113                         = passed-inbound-reference-reconciliation
plan_114                         = passed-terminal-routing-chain-correction
plan_115                         = passed-emissary-q0-construction-and-obep-reply-only
Q0_native_emissary               = passed
Q1_authenticated_transport       = deferred
Q2_external_return_established   = deferred
plan_116_local_data_plane        = unblocked-and-next
plan_117_live_integration        = blocked-until-plan116-passes
normal_daemon_ntcp2              = disabled-and-unenableable
ntcp2                             = experimental-non-advertised
```

## Next executor instruction

Do **not** go directly to Plan 117.

Do **not** resume the historical Branch E instruction to wait for a
rootless/Multipass-capable host.

Do **not** reopen Plans 109-115, the old NTCP2 harness, i2pd adapter work, or
namespace/VM work.

Start with [`plans/116-handoff.md`](116-handoff.md).

Plan 116 fixes the real remaining Milestone 5 local product gap:

```text
real established tunnel ownership
 -> TunnelData preprocessing
 -> fragmentation/reassembly
 -> AES layer transformations
 -> tunnel role processing
 -> deterministic outbound + inbound exploratory pair
```

Only after that local data plane passes should Plan 117 revisit a real transport
lane and live exploratory/NetDB evidence.

## Evidence bookkeeping note

The temporary Emissary Q0 checkout/test was deleted after execution as intended.
The closure record retains the pinned reference revision, test name, framing
lengths, message digests, and native decision. The temporary reference-test
patch SHA-256 requested by the planning document was not retained.

Classify that as an **evidence-bookkeeping limitation only**. It does not
invalidate the observed native Emissary acceptance and does not authorize a Q0
rerun or another validation pass.
