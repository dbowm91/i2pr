# Plan 117 handoff

- Status: **ready-for-execution**
- Date: 2026-08-18
- Plan of record: [`117-live-exploratory-netdb-integration.md`](117-live-exploratory-netdb-integration.md)
- Predecessor: [`116-status.md`](116-status.md) — **passed-final-local-closure**
- Roadmap: [`115-117-external-delivery-to-live-netdb-roadmap.md`](115-117-external-delivery-to-live-netdb-roadmap.md)
- Planning baseline: `e7d6b23761c0d84f98eab2fef2b98ecfde6c4606`
- Pinned independent reference: upstream Emissary `9b43484a21d5a1291c4881cdae62a36c527f8c0f` (`emissary-core 0.4.0`)

## Start here

Plan 116 is closed. Do not restart TunnelData/AES/fragment/short-build validation work without new affirmative protocol-defect evidence.

Execute [`117-live-exploratory-netdb-integration.md`](117-live-exploratory-netdb-integration.md) in its specified phase order.

The goal is to compose the router pieces already built:

```text
real exploratory short builds
 -> registered established material
 -> activated local tunnel roles
 -> real DatabaseLookup message ownership
 -> outbound exploratory TunnelData
 -> independent floodfill handling
 -> reply through the configured inbound tunnel
 -> local endpoint recovery
 -> RouterInfo validation/store
 -> RouterInfo publication + independent observation
```

This is a router-construction pass with one bounded independent native checkpoint. It is not another Milestone 3 interoperability-harness pass.

---

## Evidence levels

Keep these three claims separate throughout execution:

```text
117-L  local production composition
117-N  pinned Emissary native mixed-router composition
117-X  authenticated transport / live-process delivery
```

Required for this implementation pass:

```text
117-L = pass
117-N = pass or one localized reproducible protocol defect corrected once
```

117-X is attempted only if an already-qualified host lane is runnable without reconstructing the old namespace/VM/Python harness.

If not runnable:

```text
117-X = deferred-host-lane-unavailable
```

Stop external work at that point. Do not treat the host limitation as an I2P protocol failure.

A successful 117-N in-process reference run does **not** prove Q1 authenticated transport or Q2 live reply-to-Established.

---

## Current concrete implementation gaps

### 1. Lookup action discards the request

Current `LookupAction::SendDatabaselookup` contains only:

```text
lookup_id
peer
encoded_len
```

`lookup_engine.rs` constructs the actual `DatabaseLookupMessage`, validates/encodes it, and then discards it.

First correction:

```text
LookupAction::SendDatabaselookup
 -> own the actual typed DatabaseLookup request/body
```

Do not reconstruct protocol semantics later from `encoded_len`.

### 2. Publication attempt discards the DatabaseStore body

`PublicationCoordinator::begin_attempt()` constructs/validates a `DatabaseStoreMessage` but `PublicationAttemptRecord` returns only the local RouterInfo byte snapshot.

Correct it so the attempt owns the exact typed publication message the runtime must send.

Also verify that the `RouterInfoCompressed` payload is genuinely canonical gzip and independently decompresses to the exact local RouterInfo bytes.

### 3. `NetDbSeam` still contains the post-path stub

`pending_action_after_path()` accepts a valid reply path and then returns `NeedExploratoryReplyPath` again.

Plan 117 replaces this with real state-machine advancement to the next `SendDatabaselookup`/terminal action.

Do not preserve the placeholder behavior for compatibility.

### 4. Established material has no composition-time role owner

The pool stores `EstablishedMaterial`; production data-plane roles consume the one-shot `EstablishedTunnel` extracted from it.

Create one bounded local role registry and one narrow activation seam.

Recommended ownership:

```text
TunnelSlot -> OutboundGatewayRole
local inbound receive TunnelId -> (TunnelSlot, LocalInboundEndpointRole)
```

Do not persistently clone `LayerKeys` between pool and runtime role state.

### 5. Daemon is still intentionally non-networked

`i2pr-daemon` does not currently depend on `i2pr-tunnel`, and its service graph intentionally has no NTCP2 service.

Add the tunnel dependency/composition code Plan 117 needs, but **do not enable normal-daemon NTCP2**.

---

## Required execution order

Do not reorder the pass around the independent reference test.

```text
117-A  LookupAction + PublicationAttemptRecord own real typed I2NP bodies
  -> NetDB ownership/codec tests green

117-B  one-shot pool material activation + bounded local role registry
  -> ownership/expiry tests green

117-C  replace NetDbSeam post-reply-path stub + bounded build scheduling
  -> inbound/outbound path readiness tests green

117-D  DatabaseLookup standard-I2NP -> OBGW -> TunnelData -> first-hop DeliveryRequest
  -> prove no direct floodfill transport fallback

117-E  inbound TunnelData -> LocalInboundEndpointRole -> DatabaseStore/DatabaseSearchReply
  -> matching RouterInfo lookup reaches Success

117-F  local RouterInfo DatabaseStore -> outbound exploratory data plane
  -> canonical compressed publication body

117-G  all-i2pr deterministic production-seam terminal integration test
  -> lookup body -> outbound tunnel -> inbound response -> validated/store success

117-H  pinned Emissary native mixed-router lookup + publication checkpoint
  -> independent native evidence

117-I  authenticated transport checkpoint only if an existing qualified lane is runnable
  -> pass or explicit host-lane defer

117-J  status / roadmap / architecture closure authority
```

Do not start 117-H before 117-G passes.

---

## Product boundaries to preserve

Use these existing production surfaces:

```text
ShortBuildStateMachine
ShortBuildI2npBridge
ShortBuildRegistrar::admit_established_machine
ExploratoryPool
EstablishedMaterial::into_established_tunnel
OutboundGatewayRole::forward_cells
LocalInboundEndpointRole::process
RouterInfoLookup
PublicationCoordinator
i2pr-proto standard I2NP codec
i2pr-transport::EncodedI2npMessage
i2pr-transport::DeliveryRequest
```

Do not introduce parallel codecs, transports, tunnel registries, or lookup state machines.

The daemon/runtime standard-I2NP boundary owns message ID + expiration and encodes the typed body once.

The `DatabaseLookup` itself is **not** sent directly to the floodfill transport. It is ROUTER delivery inside outbound exploratory TunnelData. Only the resulting TunnelData standard I2NP messages are handed to first-hop `DeliveryRequest`s.

Inbound TunnelData dispatch is keyed by the explicit **local inbound endpoint receive tunnel ID**, not creator ID or external IBGW receive ID.

---

## Pinned Emissary native checkpoint

Use exactly:

```text
repository = https://github.com/eepnet/emissary.git
revision   = 9b43484a21d5a1291c4881cdae62a36c527f8c0f
package    = emissary-core 0.4.0
```

The minimum native topology is:

```text
outbound: i2pr creator -> Emissary OBEP
inbound:  Emissary IBGW -> Emissary Participant -> i2pr local endpoint
```

The pinned Emissary production code provides:

```text
short-build IBGW / Participant / OBEP admission
OBEP TunnelData decrypt + Tunnel Message parse/reassembly
ROUTER/TUNNEL delivery
floodfill DatabaseLookup handling
DatabaseStore / DatabaseSearchReply response routing
IBGW / participant TunnelData forwarding
```

Use a temporary checkout and test-only reference patch only. Do not vendor Emissary.

Record the **final temporary patch SHA-256 before deleting the checkout**.

The reference test may use a test-only Emissary Q2 decapsulation seam for the garlic-wrapped outbound build reply, provided it delegates to Emissary's own production/test crypto and the result is explicitly classified:

```text
Q2_live_return_established = not-proven
```

Do not implement a general garlic subsystem merely to remove this test-only boundary.

Native lookup acceptance must prove:

```text
Emissary OBEP accepts i2pr TunnelData
 -> exposes standard DatabaseLookup
 -> key matches requested target
 -> from matches real i2pr inbound gateway
 -> reply tunnel matches real i2pr inbound gateway receive id
 -> Emissary floodfill handles lookup
 -> response enters Emissary inbound tunnel
 -> i2pr endpoint recovers DatabaseStore
 -> i2pr validates/stores RouterInfo
 -> lookup reaches Success
```

Publication must also reach Emissary floodfill acceptance and an independent native observation, preferably read-after-write lookup.

---

## Strict reference budget

```text
1 baseline reference compile/test attempt
2 narrow correction attempts for test-patch/API mistakes
1 confirmation run
STOP
```

If Emissary production processing exposes a reproducible i2pr protocol defect, authorize exactly one localized product fix and one focused confirmation.

Do not add i2pd/Java/reference matrices during Plan 117.

---

## Fixed forbidden scope

```text
normal-daemon NTCP2 activation/correction
SSU2
new Python interoperability harnesses
rootless namespace engineering
Docker / Multipass / VM orchestration
public I2P participation
new Java/i2pd adapters
LeaseSet/client tunnel implementation
general garlic subsystem
streaming
SAM
I2CP
SOCKS/HTTP proxying
new generic router dispatcher
rewrite of Plan 116 TunnelData/AES/fragment code
```

One non-blocking Plan 116 hardening item may be fixed opportunistically if `fragment.rs` is already touched: reject a new fragment sequence greater than an already-declared `is_last=true` terminal sequence. Do not reopen Plan 116 or expand Plan 117 to chase it.

---

## Validation bar

At minimum:

```bash
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-netdb --all-targets
cargo test --locked -p i2pr-tunnel --all-targets
cargo test --locked -p i2pr-daemon --all-targets
cargo test --locked -p i2pr-testkit --all-targets
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
git diff --check
```

Run old NTCP2 validation scripts only if they remain mandatory repository-wide CI. Do not edit them to make Plan 117 green.

---

## Terminal acceptance

Before marking local/native Plan 117 complete, require at least:

```text
lookup_action_real_body                       = passed
publication_action_real_body                  = passed
publication_routerinfo_gzip                   = passed
pool_material_activation                      = passed-once-only
runtime_role_expiry_cleanup                   = passed
netdb_seam_post_path_dispatch                 = passed
no_direct_floodfill_lookup_transport          = passed
outbound_lookup_tunneldata                    = passed
inbound_response_tunneldata                   = passed
matching_routerinfo_validation_store          = passed
wrong_target_does_not_complete                = passed
database_search_reply_state_transition        = passed
publication_through_exploratory               = passed
all_i2pr_terminal_lookup_trajectory            = passed
emissary_native_short_build_roles             = passed
emissary_native_obep_tunneldata               = passed
emissary_native_floodfill_lookup              = passed
emissary_native_inbound_return                = passed
i2pr_reference_routerinfo_lookup_success      = passed
emissary_native_publication_acceptance        = passed
reference_patch_sha256_recorded               = passed
```

Then classify 117-X honestly.

If the current host still lacks an authenticated transport lane:

```text
plan_117_local_composition       = passed
plan_117_native_reference        = passed-emissary-mixed-router-netdb
plan_117_authenticated_transport = deferred-host-lane-unavailable
plan_117                         = local-native-complete-external-deferred
milestone4b_authenticated_external = blocked
router_construction              = may-continue
```

If an authenticated lane also succeeds:

```text
plan_117_authenticated_transport = passed
plan_117 = passed-qualified-exploratory-netdb-integration
milestone4b_authenticated_external = eligible-for-closure-review
```

Do not state that normal-daemon transport is production-ready in either case.

---

## After Plan 117

Once 117-L and 117-N are green, the next router-construction decision may move into the local Milestone 6 product layer even when 117-X is deferred by the host environment:

```text
Destination lifecycle
 -> destination tunnel pools
 -> garlic
 -> LeaseSet creation/publication/lookup
 -> local destination routing
 -> minimal streaming
```

SAM/I2CP remain downstream of a functioning destination/streaming core.
