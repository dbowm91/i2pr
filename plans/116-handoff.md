# Plan 116 handoff

- Status: **correction-required-local-data-plane-not-closed**
- Date: 2026-08-18
- Original plan:
  [`plans/116-local-tunnel-data-plane.md`](116-local-tunnel-data-plane.md)
- **Execute now**:
  [`plans/116-completion-correction.md`](116-completion-correction.md)
- Current status authority:
  [`plans/116-status.md`](116-status.md)
- Predecessor:
  [`plans/115-status.md`](115-status.md)
- Plan 117: **blocked until this local completion pass succeeds**.

## Start here

Do not start Plan 117 and do not perform more external short-build/transport validation.

The first Plan 116 implementation pass created useful modules but left core correctness defects and then ignored 17 failing Plan 116 tests. The current task is to correct and finish those local Rust surfaces.

Current implementation floor:

```text
91d3a8569ee20d71ab7a4ae27b6c54a1e5009429
```

Current high-level state:

```text
plan_115                          = passed-emissary-q0-construction-and-obep-reply-only
Q0_native_emissary               = passed
Q1_authenticated_transport       = deferred
Q2_external_return_established   = deferred
plan_116                          = correction-required-local-data-plane-not-closed
plan_117                          = blocked-until-plan116-passes
normal_daemon_ntcp2               = disabled-and-unenableable
ntcp2                             = experimental-non-advertised
```

## Retain the scaffolding

Do not rewrite the subsystem from scratch. Correct and complete:

```text
established.rs  established secret/path ownership
data.rs         Tunnel Message builder/parser + fragmentation
fragment.rs     bounded reassembly
layer.rs        AES forward/inverse + duplicate window
roles.rs        runtime-neutral gateways/participants/endpoints
```

## Mandatory correction order

### 1. Wire format first

Fix:

```text
checksum = SHA256(bytes after zero delimiter || IV)[0..4]
```

The checksum excludes padding and excludes the zero delimiter.

Then separate:

```text
unfragmented initial record:
  bit3 = 0
  Message ID absent

fragmented first record:
  bit3 = 1
  Message ID present

follow-on:
  sequence 1..63
```

Finish automatic complete-message fragmentation before role E2E work.

### 2. AES correctness second

Forward participant/IBGW/OBEP transform:

```text
ECB-ENC(ivKey)
CBC-ENC(layerKey)
ECB-ENC(ivKey)
```

Creator inverse:

```text
ECB-DEC(ivKey)
CBC-DEC(layerKey)
ECB-DEC(ivKey)
```

OBEP must apply the forward transform once before parsing plaintext.

### 3. Remove fake randomness

Production-facing gateway methods must accept caller-injected CSPRNG state.

Require:

- fresh 16-byte IV per cell;
- CSPRNG-sourced nonzero padding;
- no deterministic IV constants;
- no zero-only RNG implementing `CryptoRng`;
- no repeated 64-byte padding pattern;
- no index-derived padding fallback.

### 4. Wire established ownership into the real pool

The inbound remote hop vector is:

```text
IBGW -> Participant*
```

The local inbound endpoint is creator-side state, not a synthetic remote hop.

Remove fake `u32::MAX` / zero-hash next-hop sentinels in favor of typed optional routing state.

Then implement:

```text
ShortBuildStateMachine Established
 -> one-time take established material
 -> ShortBuildRegistrar
 -> real ExploratoryPool secret-bearing entry
 -> actual TunnelSlot
```

`ShortBuildRegistrar::admit()` must no longer return fabricated slot 0.

Inbound reply-path selection must use the first remote IBGW's receive tunnel ID.

### 5. Finish bounded reassembly

Implement real caller-time expiry, aggregate retained-byte accounting, capacity checks before mutation, idempotent identical duplicates, conflicting-duplicate cleanup, and processing of every fragment record in a cell.

### 6. Finish role trajectories

Required active local trajectories:

```text
OBGW -> participant -> OBEP -> ROUTER
```

and:

```text
outbound OBGW
 -> outbound participant(s)
 -> OBEP
 -> TUNNEL delivery / TunnelGateway
 -> inbound IBGW
 -> inbound participant(s)
 -> local inbound endpoint
 -> exact original standard I2NP bytes
```

Also require a fragmented/out-of-order end-to-end case.

## Ignored-test rule

The 17 tests added as:

```rust
#[ignore = "Plan 116 provisional scaffolding: see plans/116-status.md"]
```

must not remain ignored at closure.

Do not make CI green by ignoring, deleting, weakening, or feature-gating Plan 116 correctness tests.

At closure:

```bash
rg -n 'Plan 116 provisional scaffolding' crates/i2pr-tunnel/src
```

must return zero matches.

If a test assumption is proven wrong by the official specification, replace it with the correct normative test and record why.

## Fixed scope boundary

Forbidden during this pass:

```text
Emissary/i2pd/Java runtime validation
NTCP2 correction or activation
SSU2
Q1/Q2
rootless namespaces
Docker / Multipass / VMs
Python interoperability harnesses
public I2P network
new generic router dispatcher
garlic / LeaseSet / streaming / SAM / I2CP
Plan 117 execution
```

This is now an ordinary local Rust protocol/data-plane correction. Environment limitations do not block it.

## Closure bar

Do not advance until all detailed criteria in
[`plans/116-completion-correction.md`](116-completion-correction.md) pass.

Minimum terminal state:

```text
plan_116_provisional_ignored_tests = 0
wire_unfragmented_roundtrip         = passed
wire_fragmented_roundtrip           = passed
wire_checksum_rule                  = passed
layer_one_hop_roundtrip             = passed
layer_multihop_outbound             = passed
layer_multihop_inbound              = passed
obep_forward_transform              = passed
production_rng_injected             = passed
real_registrar_pool_insertion       = passed
inbound_reply_path_first_hop_id     = passed
reassembly_out_of_order             = passed
reassembly_expiry                   = passed
reassembly_aggregate_bound          = passed
outbound_router_trajectory          = passed
outbound_to_inbound_trajectory      = passed
fragmented_e2e_trajectory           = passed
workspace_validation                = passed
plan_116                             = passed-local-tunnel-data-plane
```

Only then is Plan 117 the next executable line of work.
