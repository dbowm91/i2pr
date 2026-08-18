# Plan 116 handoff

- Status: **ready-for-implementation**
- Date: 2026-08-18
- Plan-of-record:
  [`plans/116-local-tunnel-data-plane.md`](116-local-tunnel-data-plane.md)
- Predecessor:
  [`plans/115-status.md`](115-status.md)
- Corrected predecessor result:
  `passed-emissary-q0-construction-and-obep-reply-only`
- Successor **only after Plan 116 passes**: Plan 117 live exploratory/NetDB
  integration.

## Start here

Plan 115 Q0 passed. Do not do more external short-build validation before this
plan.

The next executable work is the local tunnel data plane:

```text
real established build state
 -> TunnelData preprocessing
 -> fragmentation/reassembly
 -> AES tunnel layer transforms
 -> participant/endpoint roles
 -> deterministic outbound + inbound exploratory pair
```

The implementation must remain runtime-neutral and transport-neutral.

## Current authoritative state

```text
plan_111                          = retained-core-crypto-corrected
plan_112                          = passed-outbound-pre-delivery-closure
plan_113                          = passed-inbound-reference-reconciliation
plan_114                          = passed-terminal-routing-chain-correction
plan_115                          = passed-emissary-q0-construction-and-obep-reply-only
Q0_native_emissary               = passed
Q1_authenticated_transport       = deferred
Q2_external_return_established   = deferred
plan_116                          = ready-for-implementation
plan_117                          = blocked-until-plan116-local-data-plane-passes
normal_daemon_ntcp2               = disabled-and-unenableable
ntcp2                             = experimental-non-advertised
```

## The first defect to fix

Do not start by writing AES code.

`ShortBuildRegistrar::admit()` in `crates/i2pr-tunnel/src/short_state.rs` is
still a placeholder. It returns a fabricated `TunnelSlot::from_raw(0)` success
without inserting usable tunnel state, and the current `ExploratoryPool`
contains only public metadata. The state machine retains the derived
`LayerKeys`, but successful registration currently loses them.

Plan 116A must therefore establish real ownership first:

```text
ShortBuildStateMachine reaches Established
 -> one-time move of per-hop LayerKeys + validated path metadata
 -> real EstablishedTunnel owner
 -> real ExploratoryPool insertion
 -> usable inbound/outbound entry
 -> removal/expiry drops secrets
```

No production-established pool entry may exist without the key/routing material
needed by the data plane.

## Execution order

### 116A — established state and registrar

Implement the `EstablishedTunnel` / `EstablishedHop` equivalent and one-time
success transfer.

Required distinctions:

```text
creator logical tunnel ID
first remote hop receive tunnel ID
inbound gateway receive tunnel ID
local inbound receive tunnel ID
OBEP build-reply terminal route
OBEP TunnelData delivery destination
```

Do not alias these merely because earlier placeholder pool APIs use one
`tunnel_id` field.

After A:

- real pool counts change;
- inbound ReplyPath uses first-hop gateway receive tunnel ID;
- keys are retained only while the tunnel is usable;
- no fabricated slot-0 registration remains.

### 116B — pure TunnelData format and crypto

Reuse `i2pr-proto::TunnelDataMessage` and `TunnelGatewayMessage`.

Implement in `i2pr-tunnel`:

```text
1008-byte decrypted payload builder/parser
LOCAL / ROUTER / TUNNEL instructions
first/follow-on fragmentation
bounded reassembly
checksum + nonzero padding
AES-256 ECB/CBC/ECB layer transform
duplicate tag = IV XOR first encrypted block
bounded replay window
```

Use the existing `LayerKeys::layer_key()` and `iv_key()`.

The AES transform is the deployed/current tunnel transform:

```text
working_iv = AES-ECB-ENC(ivKey, received_iv)
new_data   = AES-CBC-ENC(layerKey, working_iv, received_data)
next_iv    = AES-ECB-ENC(ivKey, working_iv)
```

Outbound creator preprocessing and local inbound endpoint processing use the
inverse transform over the hops in reverse path order.

Do not implement Proposal 153 ChaCha layer encryption.

### 116C — local role composition

Implement runtime-neutral role state:

```text
OutboundGateway (local creator)
Participant
OutboundEndpoint
InboundGateway
Inbound Participant
InboundEndpoint (local creator)
```

Participant state must know only its own receive ID, next router/tunnel, keys,
previous-peer lock, replay window, and expiry. Do not leak creator path vectors
into transit state.

The key acceptance trajectory is:

```text
local OBGW
 -> outbound participant(s)
 -> OBEP
 -> TUNNEL delivery / TunnelGateway
 -> inbound IBGW
 -> inbound participant(s)
 -> local inbound endpoint
 -> exact original standard I2NP message
```

Run this deterministically in memory. No sockets or transport handshake.

## Protocol constants that must not drift

```text
TunnelData body total                  = 1028 bytes
outer tunnel ID                        = 4 bytes
IV                                     = 16 bytes
encrypted/decrypted data region        = 1008 bytes
follow-on fragment sequence            = 1..63
maximum fragments total                = 64
checksum                               = SHA256(post-zero payload || IV)[0..4]
padding                                = random nonzero bytes before one 0x00
```

First-fragment delivery modes required now:

```text
00 LOCAL
01 TUNNEL
10 ROUTER
11 invalid
```

Delay/extended/reserved bits remain unsupported and must fail closed.

## Dependency guidance

Keep `i2pr-tunnel` runtime-neutral.

For established AES layer crypto:

- promote existing RustCrypto `aes` 0.8.4 to a production dependency;
- use a minimal RustCrypto `cbc` dependency for no-padding AES-256 CBC;
- use the `aes` block traits directly for the single-block ECB IV operations;
- do not add OpenSSL/native crypto;
- do not hand-roll AES or a padding scheme.

Do not add an `i2pr-tunnel -> i2pr-transport` dependency merely for local role
tests. Emit a small semantic router-delivery action and adapt it to
`EncodedI2npMessage` / `DeliveryRequest` later in the runtime composition
boundary.

## Fixed scope boundary

Forbidden during Plan 116:

```text
NTCP2 edits or activation
SSU2
Q1/Q2
Emissary/i2pd/Java runtime validation
rootless namespaces
Docker / Multipass / VM work
Python interop harness work
public network execution
generic router dispatcher
LeaseSet / garlic / streaming / SAM / I2CP
Milestone 11 transit admission policy expansion
```

A deterministic transit role used to prove the local tunnel trajectory is in
scope; public-router transit participation is not.

## Acceptance summary

Do not close Plan 116 unless:

1. established builds produce real usable pool entries with keys;
2. fake registrar success is removed;
3. secrets are dropped on removal/expiry/failure;
4. TunnelData format/checksum/padding is canonical;
5. LOCAL/ROUTER/TUNNEL delivery instructions work;
6. bounded fragmentation/reassembly handles out-of-order and duplicate cases;
7. AES layer transforms round-trip over multiple hops;
8. previous-peer locking and duplicate suppression work;
9. deterministic outbound ROUTER delivery succeeds;
10. deterministic outbound-to-inbound TUNNEL delivery returns the exact
    original I2NP message locally;
11. malformed/unknown/expired inputs are bounded and fail closed;
12. full workspace/boundary validation is green except explicitly documented
    pre-existing historical-harness blockers;
13. no live interoperability claim is made;
14. closure records `plan_116 = passed-local-tunnel-data-plane`.

## After Plan 116

Only after this plan passes should Plan 117 become executable.

Plan 117 will be the point to revisit the smallest real transport lane and test:

```text
real independent tunnel hop(s)
 -> working TunnelData
 -> outbound exploratory DatabaseLookup
 -> inbound exploratory response
 -> NetDB validation/persistence
 -> RouterInfo publication verification
```

If transport is still blocked in this environment at that time, classify that
as Plan 117 integration evidence. Do not reopen Plans 109-116.