# Plan 116 handoff

- Status: **final-closure-pending**
- Date: 2026-08-18
- Original plan: [`116-local-tunnel-data-plane.md`](116-local-tunnel-data-plane.md)
- Prior correction: [`116-completion-correction.md`](116-completion-correction.md)
- **Execute now**: [`116-final-closure.md`](116-final-closure.md)
- Current status authority: [`116-status.md`](116-status.md)
- Predecessor: [`115-status.md`](115-status.md)
- Plan 117: **blocked until this final local closure pass succeeds**.

## Start here

Do not start Plan 117 and do not reopen external short-build/transport validation.

The `78f1024c...` corrective implementation fixed most of the wire/AES/data-plane defects, but a post-implementation audit found four remaining closure blockers plus one directly coupled fragmented-delivery bug.

Current high-level state:

```text
plan_115_Q0                     = passed-emissary-native-consumer
plan_116_wire_format            = corrected-local
plan_116_aes_layer              = corrected-local
plan_116_rng                    = corrected-local
plan_116_outbound_router        = passed-local
plan_116                        = final-closure-pending
plan_117                        = blocked-on-plan116
Q1_authenticated_transport      = deferred
Q2_external_return_established  = deferred
normal_daemon_ntcp2             = disabled-and-unenableable
ntcp2                           = experimental-non-advertised
```

## Retain the implementation

Do not rewrite these modules:

```text
short.rs        short-build state machine and retained hop contexts
short_state.rs  registrar
established.rs  established secret/path ownership
pool.rs         exploratory pool + TunnelEntry
data.rs         Tunnel Message framing/fragment creation
fragment.rs     bounded reassembly
layer.rs        AES layer crypto
roles.rs        runtime-neutral tunnel roles
```

Correct the remaining seams in place.

---

## Closure order

### F1 — wire the successful short-build state into real established material

`ShortBuildStateMachine` already owns:

```text
validated ShortBuildPath
HopCryptoContext[]
  -> LayerKeys for each real hop
StatePhase::Established after all replies accept
```

Implement the success-only one-time transfer described in `116-final-closure.md`:

```text
machine reaches Established
 -> take path + retained LayerKeys once
 -> EstablishedTunnel
 -> EstablishedMaterial
 -> ShortBuildRegistrar::admit_material
 -> real pool entry + real TunnelSlot
```

Required topology mapping:

```text
outbound remote hops = Participant* -> OBEP
inbound remote hops  = IBGW -> Participant*
```

The local inbound endpoint remains separate creator-side state.

The terminal inbound remote hop's `next_tunnel` is the local inbound receive tunnel.

The outbound OBEP has no fixed data-plane next hop.

The old material-less `ShortBuildRegistrar::admit()` must either be removed or fail with a typed material-required error. It must not fabricate `Duplicate` or insertion success.

### F2 — enforce material-only established pool entries in production

Remove the production path:

```text
register_inbound/register_outbound
 -> build_placeholder_established
 -> synthetic peers/keys
 -> Established pool entry
```

Move metadata-only helpers under `#[cfg(test)]` or migrate tests to deterministic real `EstablishedMaterial` fixtures.

Production established entries must enter only through material-bearing registration.

### F3 — fix automatic fragmentation boundaries

Record budget after checksum + delimiter:

```text
1003 bytes
```

Exact overheads:

```text
unfragmented LOCAL    3
unfragmented ROUTER  35
unfragmented TUNNEL  39
first LOCAL           7
first ROUTER         39
first TUNNEL         43
follow-on             7
```

Therefore exact payload capacities are:

```text
unfragmented LOCAL   1000
unfragmented ROUTER   968
unfragmented TUNNEL   964
first LOCAL           996
first ROUTER          964
first TUNNEL          960
follow-on              996
```

Do not merely assert that fragmentation returns `First + FollowOn` enums. Every accepted generated fragment set must pass:

```text
fragment_complete_message
 -> build_cells
 -> parse
 -> reassemble
 -> exact original bytes
```

for LOCAL, ROUTER, and TUNNEL boundary sizes.

### F4 — retain fragmented delivery instructions

A fragmented first record owns the complete message's `DeliveryInstruction`.

The OBEP currently loses this state and may synthesize LOCAL after follow-on completion. Remove that behavior.

A fragmented message must complete as the original:

```text
LOCAL  -> LOCAL
ROUTER -> ROUTER / exact router
TUNNEL -> TunnelGateway / exact router + tunnel
```

Delete the unspecified-delivery fallback once the reassembler retains the first-fragment metadata correctly.

### F5 — execute the full outbound-to-inbound trajectory

The terminal acceptance test is not routing metadata. It is exact-byte recovery through every actual role:

```text
original standard I2NP bytes
 -> OutboundGatewayRole
 -> outbound participant(s)
 -> OutboundEndpointRole
 -> RouterDeliveryAction::TunnelGateway
 -> canonical TunnelGatewayMessage
 -> InboundGatewayRole
 -> inbound participant(s)
 -> LocalInboundEndpointRole
 -> exact original standard I2NP bytes
```

Required assertion:

```rust
assert_eq!(recovered, original);
```

Then repeat with a message large enough to require fragmentation and reorder valid follow-on cells before the local endpoint to exercise bounded reassembly.

The existing test comment that leaves the full round trip “to runtime” must be removed; Plan 116 is specifically responsible for proving the runtime-neutral local data-plane trajectory before external integration.

---

## Fixed scope boundary

Forbidden during this pass:

```text
Emissary/i2pd/Java runtime validation
NTCP2 correction/activation
SSU2
Q1/Q2
rootless namespaces
Docker / Multipass / VM work
Python interoperability harnesses
public I2P network
new generic router dispatcher
garlic / LeaseSet / streaming / SAM / I2CP
Plan 117 execution
```

This pass has no environment blocker: all acceptance tests are local and deterministic.

---

## Required source-policy checks

At closure run:

```bash
rg -n 'Plan 116 provisional scaffolding' crates/i2pr-tunnel/src
rg -n 'build_placeholder_established' crates/i2pr-tunnel/src
rg -n 'action_from_unspecified' crates/i2pr-tunnel/src
rg -n 'full round-trip is left to the runtime|left to the runtime' crates/i2pr-tunnel/src/roles.rs
```

Expected production-state result:

```text
provisional ignored tests        = 0
production placeholder material  = 0
unspecified delivery fallback    = 0
cross-tunnel runtime exemption   = 0
```

A test-only placeholder helper is acceptable only when explicitly `#[cfg(test)]` and absent from production compilation.

---

## Validation bar

Before closure:

```bash
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-tunnel --lib
cargo test --locked -p i2pr-tunnel --all-targets
cargo test --locked -p i2pr-proto --all-targets
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-ntcp2-vectors.sh
bash scripts/check-ntcp2-interoperability.sh
git diff --check
```

Do not modify historical interoperability harnesses to make Plan 116 pass.

---

## Terminal acceptance state

Do not close until all of these are true:

```text
state_machine_material_take_once          = passed
state_machine_material_second_take        = rejected
real_registrar_pool_insertion             = passed
legacy_materialless_registrar_success     = impossible
production_placeholder_pool_entries       = impossible
inbound_reply_path_real_ibgw              = passed
fragment_boundary_local                   = passed
fragment_boundary_router                  = passed
fragment_boundary_tunnel                  = passed
fragment_generated_cells_roundtrip        = passed
fragmented_delivery_metadata_retained     = passed
fragmented_router_action                  = passed
fragmented_tunnel_action                  = passed
outbound_router_trajectory                = passed
outbound_to_inbound_exact_bytes           = passed
outbound_to_inbound_fragmented_exact      = passed
out_of_order_fragmented_cross_tunnel      = passed
workspace_validation                      = passed
plan_116                                  = passed-local-tunnel-data-plane
```

Only then may Plan 117 become the next executable line of work.
