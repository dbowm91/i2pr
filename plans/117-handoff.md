# Plan 117 handoff — corrective closure

- Status: **local-native-complete-external-deferred**
- Date: 2026-08-18
- Corrective plan of record: [`117-corrective-closure.md`](117-corrective-closure.md)
- Original Plan 117: [`117-live-exploratory-netdb-integration.md`](117-live-exploratory-netdb-integration.md)
- Status authority: [`117-status.md`](117-status.md)
- Predecessor: [`116-status.md`](116-status.md) — **passed-final-local-closure**
- Roadmap: [`115-117-external-delivery-to-live-netdb-roadmap.md`](115-117-external-delivery-to-live-netdb-roadmap.md)
- Current implementation floor: `1608f5e5be3d2003b82340fb0293776087c3672c`
- Corrective closure commit: `9fdfc1038f5cd018ad7a69d06fcc10400406f604`
- Pinned independent reference: upstream Emissary `9b43484a21d5a1291c4881cdae62a36c527f8c0f` (`emissary-core 0.4.0`)

## Start here

Do **not** restart Plan 117 A–F and do **not** reopen Plan 116.

Execute [`117-corrective-closure.md`](117-corrective-closure.md).

The initial A–F implementation is useful and should be corrected in place. The remaining work is now explicitly bounded to:

```text
wrong lookup ROUTER destination
wrong outer TunnelData authenticated-link framing
pool activation destroys reply-path metadata
manual readiness bit can diverge from real registry
missing Phase G terminal production-seam trajectory
missing Phase H pinned in-process Emissary native checkpoint
honest Phase I authenticated-transport classification
closure documentation synchronization
```

---

## Current product state

Retain:

```text
LookupAction owns DatabaseLookupMessage
PublicationAttemptRecord owns DatabaseStoreMessage
RouterInfo publication gzip exists
RouterInfoLookup post-path advance exists
DataPlaneRegistry exists
outbound lookup/publication composition modules exist
inbound TunnelData dispatch exists
normal daemon NTCP2 remains disabled
```

Correct before closure:

```text
Plan 117-D routing identity
Plan 117-D/F outer I2NP transport framing
Plan 117-B activation lifetime ownership
Plan 117-C readiness authority
Plan 117-G terminal local proof
Plan 117-H native reference proof
```

---

## Three identities that must remain distinct

Every lookup correction/test should visibly use different values for:

```text
K = requested NetDB key
F = selected floodfill peer
P = outbound first-hop router
```

Required mapping:

```text
DatabaseLookup.key      = K
Tunnel ROUTER target    = F
DeliveryRequest.target  = P
```

Do not use `DatabaseLookup.key` as the tunnel ROUTER destination.

Official NetDB behavior is that RouterInfo lookups are sent through outbound exploratory tunnels to selected floodfill routers, with replies returning through inbound exploratory tunnels.

---

## I2NP framing boundary

The first implementation currently puts only the raw 1028-byte `TunnelData` body into `EncodedI2npMessage`.

That is not the repository's authenticated-link contract.

Correct hierarchy:

```text
DatabaseLookup / DatabaseStore
 -> standard 16-byte I2NP envelope               # nested inside tunnel
 -> OutboundGatewayRole preprocessing
 -> TunnelData body (type 18, 1028 bytes)
 -> NTCP2/SSU2 short-transport I2NP envelope     # authenticated-link message
 -> EncodedI2npMessage
 -> DeliveryRequest(target=P)
```

Use the existing `i2pr-proto` APIs:

```text
I2npMessage::new_short_transport
I2npMessage::encode_short_transport_to_vec
I2npMessage::decode_short_transport
```

Do not create a manual short-header codec.

Each TunnelData cell gets a fresh outer message ID from the injected CSPRNG. Do not reuse the nested NetDB message ID as the outer cell ID.

---

## Activation ownership correction

The pool must remain the authority for non-secret registration/routing/lifetime state after activation.

Target ownership:

```text
ExploratoryPool
  owns registration + public routing metadata + activation state

DataPlaneRegistry
  owns activated secret-bearing local role
```

`ExploratoryPool::activate(slot)` must move secrets once while leaving the pool entry present.

Required behavior:

```text
first activation         -> EstablishedTunnel returned
reply-path selection     -> unchanged after activation
pool capacity count      -> unchanged after activation
second activation        -> AlreadyActivated
expiry/failure           -> registration removed + registry role removable by same slot
```

Do not duplicate `LayerKeys` to achieve this.

---

## Readiness correction

Remove the production authority of:

```text
set_outbound_role_available(true)
```

`LookupReadyForTunnelDispatch` must derive from a real, usable `DataPlaneRegistry` outbound role at a caller-supplied deterministic time.

A stale boolean must not authorize lookup dispatch.

---

## Mandatory execution order

```text
117-C1  fix selected-floodfill routing target
117-C2  fix TunnelData short-transport I2NP framing
117-C3  retain pool metadata across one-shot secret activation
117-C4  derive readiness from DataPlaneRegistry
117-C5  focused regression matrix green
117-G   all-i2pr production-seam terminal lookup/publication
117-H   pinned Emissary in-process native lookup/publication
117-I   authenticated transport only if an existing lane is already runnable
117-J   status/docs/roadmap synchronization
```

Do not execute H before G.

Do not execute I before G and H.

---

## Phase G minimum proof

The local creator-side tunnels in the terminal test must originate from successful production short-build state machines:

```text
ShortBuildStateMachine
 -> accepted replies
 -> Established
 -> ShortBuildRegistrar::admit_established_machine
 -> ExploratoryPool real EstablishedMaterial
 -> activate(slot)
 -> DataPlaneRegistry
```

No creator-side placeholder material.

No direct `EstablishedTunnel::new()` as a substitute for successful build-derived material in the terminal test.

Then prove:

```text
lookup K selects floodfill F where F != K
 -> outbound role produces short-transport TunnelData to first hop P
 -> simulated remote outbound path exposes standard DatabaseLookup
 -> DatabaseLookup key=K, destination=F, reply path=real inbound tunnel
 -> matching DatabaseStore returns through activated inbound path
 -> local endpoint recovers exact standard DatabaseStore
 -> existing validator accepts RouterInfo K
 -> store contains K
 -> RouterInfoLookup reaches Success
```

Also prove wrong-target DatabaseStore does not complete, DatabaseSearchReply remains iterative, and publication exposes the local RouterInfo DatabaseStore at the selected floodfill target.

---

## Phase H is in-process Emissary, not the old host harness

Use exactly:

```text
repository = https://github.com/eepnet/emissary.git
revision   = 9b43484a21d5a1291c4881cdae62a36c527f8c0f
package    = emissary-core 0.4.0
```

Execution model:

```text
temporary checkout
 -> test-only dev dependencies to i2pr-under-test
 -> test inside Emissary existing test surface
 -> production native short-build/transit/NetDB handlers
 -> record final patch SHA-256
 -> delete temporary checkout
```

No rootless namespace.

No Multipass.

No Docker.

No permanent Python harness.

Minimum native topology:

```text
outbound: i2pr creator -> Emissary OBEP
inbound:  Emissary IBGW -> Emissary Participant -> i2pr local endpoint
```

Require native lookup path:

```text
i2pr corrected TunnelData
 -> Emissary OBEP
 -> standard DatabaseLookup
 -> Emissary floodfill handler
 -> DatabaseStore response
 -> Emissary inbound tunnel roles
 -> i2pr local endpoint
 -> i2pr RouterInfo validation/store
 -> lookup Success
```

and native RouterInfo publication acceptance.

The final temporary patch SHA-256 must be recorded **before** deletion. Do not repeat the Plan 115 evidence omission.

A test-only Q2 decapsulation seam remains acceptable if explicitly classified as not proving live Q2.

---

## Phase I authenticated transport

117-X remains separate from the in-process native checkpoint.

Only use an already-existing qualified authenticated lane.

If none is runnable on this host:

```text
117-X = deferred-host-lane-unavailable
Q1_authenticated_transport = deferred
Q2_external_return_established = deferred-or-not-proven
```

Stop external work there.

Do not engineer a new host lane inside this pass.

---

## Fixed forbidden scope

```text
Plan 116 reopening
normal-daemon NTCP2 activation
unrelated NTCP2 protocol corrections
SSU2 implementation
rootless namespace engineering
Multipass / VM / Docker recovery
new Python interop harness
public I2P participation
Java/i2pd reference matrix
general garlic subsystem
LeaseSet/client tunnel implementation
streaming
SAM
I2CP
SOCKS/HTTP proxy
new generic router dispatcher
```

---

## Focused regression bar before Phase G

At minimum require tests equivalent to:

```text
outbound_lookup_routes_to_selected_floodfill_not_lookup_key
outbound_lookup_delivery_is_complete_short_transport_tunneldata
nested_database_lookup_remains_standard_header
publication_routes_to_selected_floodfill
publication_delivery_is_complete_short_transport_tunneldata
activate_preserves_inbound_reply_path
activate_preserves_registration_count
second_activation_returns_already_activated
pool_eviction_can_remove_matching_registry_role
registry_empty_never_reports_lookup_ready
activated_outbound_role_enables_lookup_ready
expired_outbound_role_does_not_enable_lookup_ready
```

Then Phase G and H tests.

---

## Validation bar

Run focused tests first, then at minimum:

```bash
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-proto --all-targets
cargo test --locked -p i2pr-transport --all-targets
cargo test --locked -p i2pr-transport-ntcp2 --all-targets
cargo test --locked -p i2pr-netdb --all-targets
cargo test --locked -p i2pr-tunnel --all-targets
cargo test --locked -p i2pr-daemon --all-targets
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
git diff --check
```

Do not modify historical interoperability scripts to manufacture a pass.

---

## Closure state

Do not advance the roadmap until:

```text
plan_117_corrective_routing       = passed
plan_117_transport_framing        = passed-short-transport-tunneldata
plan_117_activation_ownership     = passed-metadata-retained-secrets-once
plan_117_runtime_readiness        = passed-registry-derived
plan_117_local_composition        = passed-all-i2pr-production-seam-netdb
plan_117_native_reference         = passed-emissary-mixed-router-netdb
```

If the only remaining gap is authenticated transport on this host, close as:

```text
plan_117_authenticated_transport  = deferred-host-lane-unavailable
plan_117                          = local-native-complete-external-deferred
router_construction               = may-continue
milestone4b_authenticated_external = blocked
```

That is the intended anti-loop outcome.
