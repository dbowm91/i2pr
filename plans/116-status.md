# Plan 116 status: final local closure pending

- Status: **final-closure-pending**
- Date: **2026-08-18**
- Current implementation floor: `78f1024c47ca5ba110656e9fe2936ca2719c319f`
- Original plan: [`116-local-tunnel-data-plane.md`](116-local-tunnel-data-plane.md)
- First completion/correction pass: [`116-completion-correction.md`](116-completion-correction.md)
- **Execute now**: [`116-final-closure.md`](116-final-closure.md)
- Predecessor: [`115-status.md`](115-status.md)
- Plan 117: **blocked until the final local closure pass succeeds**.

## Status correction

The `78f1024c...` implementation made substantial and valid Plan 116 progress, but the prior status file closed Plan 116 too aggressively.

The following work is retained as passed local implementation evidence:

```text
Tunnel Message checksum rule             = corrected
unfragmented vs fragmented-first wire    = corrected
follow-on sequence encoding              = corrected
CSPRNG padding/IV injection              = corrected
participant AES forward                  = corrected
creator AES inverse                      = corrected
OBEP final-layer direction               = corrected
remote inbound topology                  = corrected
optional hop-next representation         = corrected
bounded reassembly implementation        = substantially implemented
ignored provisional Plan116 tests        = removed
outbound ROUTER local trajectories        = passed
```

The following closure claims are **not yet supported by the code at the implementation floor**:

```text
real state-machine -> material transfer   = not wired
canonical registrar from real build       = not proven
production pool material-only invariant   = violated by placeholder APIs
automatic fragment capacity boundaries    = incorrect/incompletely tested
fragmented delivery metadata retention    = incomplete
outbound -> inbound exact-byte trajectory = not executed end-to-end
fragmented cross-tunnel exact-byte path   = not proven
```

Therefore the earlier token:

```text
plan_116 = passed-local-tunnel-data-plane
```

is superseded by:

```text
plan_116 = final-closure-pending
plan_117 = blocked-on-plan116
```

This is a status correction only. It does not invalidate the successful Plan 115 Emissary Q0 or the valid Plan 116 wire/AES corrections.

---

## Remaining blocker F1 — real established-material transfer

`ShortBuildStateMachine` already retains the validated `ShortBuildPath` and one `HopCryptoContext` per real hop through `StatePhase::Established`, including derived `LayerKeys`.

However, it does not yet expose the required success-only one-time transfer:

```text
ShortBuildStateMachine Established
 -> take real path + LayerKeys
 -> EstablishedTunnel
 -> EstablishedMaterial
 -> ShortBuildRegistrar::admit_material
 -> ExploratoryPool
```

The current legacy `ShortBuildRegistrar::admit()` cannot perform this transfer and must not report fake successful registration semantics.

---

## Remaining blocker F2 — production placeholder pool entries

`ExploratoryPool::register_inbound()` and `register_outbound()` remain normal public production methods and construct synthetic `EstablishedMaterial` through `build_placeholder_established()`.

These APIs use placeholder router hashes and synthetic layer keys and therefore violate the Plan 116 invariant that an established production pool entry must contain real build-derived material.

The placeholder path must be removed from production compilation or restricted to `#[cfg(test)]`.

---

## Remaining blocker F3 — fragment sizing + fragmented delivery state

`fragment_complete_message()` must include all record overhead when computing body capacity:

```text
unfragmented LOCAL   overhead 3
unfragmented ROUTER  overhead 35
unfragmented TUNNEL  overhead 39
fragmented first LOCAL   overhead 7
fragmented first ROUTER  overhead 39
fragmented first TUNNEL  overhead 43
follow-on                 overhead 7
record budget per cell = 1003
```

Every generated fragment sequence must successfully pass through `build_cells()` and reassembly at boundary lengths.

The OBEP must also retain the first fragment's `DeliveryInstruction` until reassembly completes. It must not synthesize LOCAL delivery for a fragmented ROUTER/TUNNEL message.

---

## Remaining blocker F4 — true outbound-to-inbound trajectory

The current test named `outbound_to_inbound_tunnel_trajectory` proves the outbound OBEP selects the inbound gateway router/tunnel, but then stops.

The required Plan 116 closure trajectory remains:

```text
original standard I2NP bytes
 -> OutboundGatewayRole
 -> outbound participant(s)
 -> OutboundEndpointRole
 -> TunnelGatewayMessage
 -> InboundGatewayRole
 -> inbound participant(s)
 -> LocalInboundEndpointRole
 -> exact original standard I2NP bytes
```

A fragmented variant must also complete and return exact bytes.

---

## Fixed anti-loop scope

The final closure pass does **not** reopen:

```text
Emissary/i2pd/Java runtime testing
NTCP2 activation/correction
SSU2
Q1/Q2
rootless namespaces
Docker/Multipass/VMs
Python interop harnesses
public I2P network
Plan 117 execution
```

This is an ordinary local Rust closure pass.

---

## Mandatory current token table

```text
plan_115_Q0                         = passed-emissary-native-consumer
Q1_authenticated_transport         = deferred
Q2_external_return_established     = deferred
plan_116_wire_format               = corrected-local
plan_116_aes_layer                 = corrected-local
plan_116_rng                       = corrected-local
plan_116_state_machine_material    = pending-final-closure
plan_116_pool_real_material_only   = pending-final-closure
plan_116_fragment_boundaries       = pending-final-closure
plan_116_fragment_delivery_state   = pending-final-closure
plan_116_outbound_router           = passed-local
plan_116_outbound_to_inbound       = pending-final-closure
plan_116_fragmented_cross_tunnel   = pending-final-closure
plan_116                           = final-closure-pending
plan_117                           = blocked-on-plan116
qualified_external_delivery        = blocked-on-host-execution-lane
normal_daemon_ntcp2                = disabled-and-unenableable
ntcp2                              = experimental-non-advertised
```

---

## Closure authority

Do not restore:

```text
plan_116 = passed-local-tunnel-data-plane
```

until every criterion in [`116-final-closure.md`](116-final-closure.md) passes with active tests and the required exact-byte outbound-to-inbound trajectories execute through the actual inbound roles.
