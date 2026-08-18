# Plan 117 — exploratory tunnel / NetDB composition and qualified native reference checkpoint

## Status

- **Ready for execution**.
- Date: **2026-08-18**.
- Planning baseline: `e7d6b23761c0d84f98eab2fef2b98ecfde6c4606`.
- Predecessor: [`116-status.md`](116-status.md) — `passed-final-local-closure`.
- Roadmap authority: [`115-117-external-delivery-to-live-netdb-roadmap.md`](115-117-external-delivery-to-live-netdb-roadmap.md).
- Handoff: [`117-handoff.md`](117-handoff.md).
- Pinned independent reference: upstream Emissary `9b43484a21d5a1291c4881cdae62a36c527f8c0f`, package `emissary-core 0.4.0`.

Plan 116 is closed. Do not reopen its TunnelData framing, AES layer, fragment machinery, or short-build cryptography unless this plan produces a reproducible protocol defect in those surfaces.

This plan is the first composition pass that joins the existing short-build control plane, established exploratory tunnels, TunnelData data plane, transport-neutral NetDB state machines, and a bounded independent-router reference path.

It is **not** a return to the historical Milestone 3 harness architecture.

---

# 1. Objective

Close the next product dependency:

```text
validated/reseeded RouterInfo store
 -> exploratory path selection/build request
 -> successful short-build material registration
 -> runtime activation of local OBGW / local inbound endpoint roles
 -> actual DatabaseLookup body retained by the lookup action
 -> DatabaseLookup standard-I2NP envelope
 -> outbound exploratory TunnelData
 -> independent OBEP ROUTER delivery
 -> floodfill DatabaseLookup handling
 -> reply addressed to the real inbound exploratory gateway/tunnel
 -> inbound TunnelData
 -> local inbound endpoint exact I2NP recovery
 -> DatabaseStore / DatabaseSearchReply ingestion
 -> bounded RouterInfo validation/store/persistence handoff
```

Then exercise the same outbound path for local RouterInfo publication and require an independent observation that the publication was accepted.

The primary success condition is not "a test emitted the right metadata". The primary success condition is that real production-owned message objects and role implementations compose across the boundaries already built in Plans 103–116.

---

# 2. Correct Gate 117 semantics for the current environment

The historical 115–117 roadmap called Gate 117 the point where live external delivery becomes mandatory. That remains the requirement for a full authenticated external interoperability claim, but it must not recreate an environment loop that blocks unrelated router construction.

This plan therefore records three separate evidence levels:

```text
117-L  local production composition
117-N  independent native reference composition (pinned Emissary, in-process)
117-X  authenticated transport / live process delivery
```

Mandatory for this implementation pass:

```text
117-L = pass
117-N = pass, or one localized reproducible protocol defect corrected once
```

117-X is attempted only if an already-qualified execution lane is available without rebuilding the old rootless/VM/Python harness. If the host cannot provide that lane, record:

```text
117-X = deferred-host-lane-unavailable
```

That result does **not** invalidate 117-L or 117-N and does not block subsequent transport-neutral router construction. It does mean Milestone 4B authenticated external acceptance remains open.

Do not claim Q1 authenticated transport or Q2 live return-to-Established unless those operations really occur across an authenticated transport.

---

# 3. Research findings that define this plan

## 3.1 Official I2P behavior

Current I2P documentation specifies that RouterInfo lookups are sent through an **outbound exploratory tunnel** and replies return through an **inbound exploratory tunnel**. A successful floodfill lookup returns `DatabaseStore`; a miss returns `DatabaseSearchReply`.

The relevant current specification references are:

- `https://i2p.net/en/docs/overview/network-database/`
- `https://i2p.net/en/docs/specs/i2np/`
- `https://i2p.net/en/docs/specs/tunnel-message/`
- `https://i2p.net/en/docs/specs/tunnel-implementation/`

Direct `DatabaseLookup` over NTCP2 to the floodfill remains forbidden as a substitute for the exploratory path.

The I2NP DatabaseLookup format permits an unencrypted reply mode. The existing i2pr `ReplyEncryption::None` path may therefore be retained for the first **RouterInfo-only** integration checkpoint. Do not expand this pass into LeaseSet lookup encryption. ECIES reply encryption is a later privacy/security hardening item unless the pinned independent reference refuses the standards-legal unencrypted RI mode.

## 3.2 Current i2pr lookup action loses the message body

`crates/i2pr-netdb/src/lookup_engine.rs` builds a real `DatabaseLookupMessage`, encodes it, then emits only:

```text
LookupAction::SendDatabaselookup {
    lookup_id,
    peer,
    encoded_len,
}
```

The actual request/body is discarded.

`encoded_len` is diagnostic data, not a dispatch payload. This must be corrected before any runtime or reference test can honestly claim to send the lookup generated by the state machine.

## 3.3 Current publication attempt also loses the real DatabaseStore body

`PublicationCoordinator::begin_attempt()` constructs a `DatabaseStoreMessage` / `I2npBody::DatabaseStore`, validates that it encodes, and then discards the body. `PublicationAttemptRecord` returns only the encoded local RouterInfo bytes.

The runtime therefore has insufficient information to reproduce the exact publication message the coordinator validated.

This must be corrected in the same data-ownership pass as the lookup action.

## 3.4 Current daemon seam stops after accepting a reply path

`crates/i2pr-daemon/src/netdb_seam.rs::pending_action_after_path()` still returns the conservative Plan 107/108-era `NeedExploratoryReplyPath` placeholder after `accept_reply_path()` succeeds. The test comments explicitly say a later plan should drive the next lookup action.

Plan 117 is that later plan.

Do not add another boolean/status shim. Drive the existing `RouterInfoLookup` state machine to the next real action.

## 3.5 Current daemon does not depend on `i2pr-tunnel`

`i2pr-daemon` currently depends on `i2pr-netdb`, `i2pr-runtime`, and `i2pr-transport`, but not `i2pr-tunnel`. The live daemon graph is intentionally non-networked and normal-daemon NTCP2 remains disabled.

Plan 117 may add the **composition dependency** on `i2pr-tunnel` and product code that creates transport-neutral delivery requests. It must **not** silently enable NTCP2 in `build_daemon_graph()`.

## 3.6 Established tunnel material needs a runtime activation owner

The exploratory pool stores `EstablishedMaterial`. The actual data-plane roles consume an `EstablishedTunnel` via the one-shot `EstablishedMaterial::into_established_tunnel()` transfer.

Therefore Plan 117 needs one explicit runtime-side owner for activated local roles:

```text
outbound slot -> OutboundGatewayRole
inbound local receive tunnel id -> LocalInboundEndpointRole
```

Remote participants/OBEP/IBGW remain remote roles. The local runtime must not clone per-hop layer keys merely to keep the pool looking secret-bearing.

The pool may remain the metadata/reply-path authority after activation. The activation operation consumes the one-shot secret-bearing role material and leaves registration metadata available for path selection and expiry.

## 3.7 Pinned Emissary can provide the independent native lane

At revision `9b43484a21d5a1291c4881cdae62a36c527f8c0f`:

- `TransitTunnelManager::handle_short_tunnel_build()` supports short-build `InboundGateway`, `Participant`, and `OutboundEndpoint` roles and starts the corresponding production transit role.
- Emissary's production OBEP decrypts TunnelData, verifies the tunnel checksum, parses/reassembles Tunnel Message records, parses standard I2NP, and returns ROUTER or TUNNEL delivery.
- Emissary's floodfill NetDB natively parses `DatabaseLookup` and responds with `DatabaseStore` or `DatabaseSearchReply`, including tunnel reply routing.

This permits a useful independent mixed-router checkpoint entirely in-process and without sockets, rootless namespaces, Docker, Multipass, or public-network participation.

The native lane does **not** prove authenticated transport. Keep Q1/Q2 classifications separate.

---

# 4. Hard scope lock

Mandatory implementation surfaces are expected to be limited to:

```text
crates/i2pr-netdb/src/lookup_action.rs
crates/i2pr-netdb/src/lookup_engine.rs
crates/i2pr-netdb/src/publication.rs
crates/i2pr-tunnel/src/pool.rs             # activation seam only if required
crates/i2pr-daemon/Cargo.toml
crates/i2pr-daemon/src/netdb_seam.rs
crates/i2pr-daemon/src/...                 # one narrow coordinator/registry module if needed
crates/i2pr-testkit/...                    # only if existing testkit is the cleanest integration owner
plans/117-status.md                        # create only at completion
plans/117-handoff.md
```

Permitted supporting changes:

```text
crates/i2pr-proto                         only for a demonstrated missing canonical codec
crates/i2pr-transport                     only for a demonstrated ownership/boundary mismatch
README / AGENTS / architecture docs       only after successful implementation
```

Forbidden scope:

```text
normal-daemon NTCP2 activation
NTCP2 crypto or handshake correction
SSU2
new Python harnesses
Docker / Multipass / VM orchestration
rootless namespace work
Java I2P adapter work
new i2pd adapter work
public I2P participation
LeaseSet/client tunnels
garlic routing as a general subsystem
streaming
SAM
I2CP
SOCKS/HTTP proxying
new generic router dispatcher
rewrite of Plan 116 TunnelData/AES/fragment code
new transport abstraction parallel to i2pr-transport
```

Use existing production abstractions. Add the minimum composition code needed to join them.

---

# 5. Phase A — make NetDB actions own the message they ask the runtime to send

This phase is mandatory before touching tunnel dispatch.

## 5.1 Lookup action

Replace the `encoded_len`-only payload with an owned typed request/body.

Preferred shape:

```rust
LookupAction::SendDatabaselookup {
    lookup_id: LookupId,
    peer: RouterHash,
    message: DatabaseLookupMessage,
}
```

An owned `I2npBody::DatabaseLookup` is also acceptable if that results in fewer conversions.

Do not make `Vec<u8>` the protocol authority inside the state machine unless a concrete API forces it. The state machine should own the typed I2NP body and the daemon/runtime envelope boundary should assign standard-header fields and encode it exactly once.

If diagnostic length is useful, expose a method that derives it by encoding or cache it as non-authoritative metadata. Do not keep a body-less action.

Required tests:

```text
send_action_carries_exact_lookup_target
send_action_carries_reply_gateway_and_tunnel
send_action_carries_excluded_peer_set
send_action_body_round_trips_through_i2np_codec
send_action_does_not_need_reconstruction_from_encoded_len
```

## 5.2 Publication action

Change `PublicationAttemptRecord` so it carries the exact `DatabaseStoreMessage` or canonical `I2npBody` that the coordinator validated.

The runtime must not reconstruct the publication body from `record.encoded` plus implicit assumptions.

The publication record may also retain the local RI bytes if useful for diagnostics/retry, but the actual outbound message must be explicit.

Required tests:

```text
publication_attempt_carries_database_store_body
publication_attempt_store_key_equals_local_router_hash
publication_attempt_body_round_trips_through_i2np_codec
publication_retry_reuses_same_database_store_semantics
```

## 5.3 RouterInfo compression audit

Before independent publication validation, verify that the `DatabaseStoreData::RouterInfoCompressed` payload is actually the canonical gzip representation required by the I2NP DatabaseStore format.

Do not trust the enum name.

If `PublicationCoordinator` currently inserts raw RouterInfo bytes into a field intended for gzip bytes, correct it here using one existing bounded gzip implementation. Do not add a second compression stack.

Acceptance:

```text
publication payload independently decompresses to exact local RouterInfo bytes
```

The gzip stream should match I2P compatibility requirements where the repository already has support for them; do not broaden this plan into general compression fingerprint hardening unless the independent reference rejects the current output.

---

# 6. Phase B — add one-shot tunnel activation and a bounded local role registry

## 6.1 Product ownership model

After successful registration:

```text
ExploratoryPool
  owns registration metadata
  owns EstablishedMaterial until activation

activation
  -> EstablishedMaterial::into_established_tunnel()
  -> exactly one local role owner
```

Create the smallest bounded runtime-side registry, for example:

```text
ExploratoryDataPlaneRegistry
  outbound: TunnelSlot -> OutboundGatewayRole
  inbound: local_receive_tunnel_id -> (TunnelSlot, LocalInboundEndpointRole)
```

The exact name is not important. The ownership rules are.

The registry must be bounded by the existing exploratory pool capacities. Do not introduce an independent unbounded tunnel count.

## 6.2 Pool activation seam

If direct mutable access is awkward, add a narrow pool method such as:

```rust
activate(slot) -> Result<EstablishedTunnel, PoolError>
```

or equivalent.

Requirements:

- succeeds exactly once per real established material entry;
- does not clone `LayerKeys` into a second persistent owner;
- leaves non-secret registration metadata available;
- a second activation fails with a typed error;
- test-only placeholder material cannot enter a production activation path;
- inbound reply-path selection remains valid after activation;
- outbound first-hop metadata remains available after activation.

Do not redesign `ExploratoryPool` into an async runtime service.

## 6.3 Expiry/failure coupling

`ExploratoryPool::advance_time()` already returns expired slots.

The owning coordinator must remove corresponding activated roles from the runtime registry whenever the pool evicts/fails/removes a slot.

Dropping a role must zeroize its key material through the existing role/established drop paths.

Required tests:

```text
activate_outbound_once
activate_inbound_once
second_activation_fails
reply_path_survives_inbound_activation
pool_expiry_removes_activated_role
pool_failure_removes_activated_role
unknown_local_receive_tunnel_fails_closed
```

---

# 7. Phase C — replace the daemon NetDB seam placeholder with a real composition state machine

The goal is not to make `run_daemon()` open sockets. The goal is to make the daemon composition root capable of producing and consuming the exact transport-neutral work items a transport service will own.

## 7.1 Add the tunnel dependency

Add `i2pr-tunnel` to `i2pr-daemon` only because Plan 117 is the composition boundary that now owns exploratory tunnels.

Do not add `i2pr-transport-ntcp2` to the daemon graph.

## 7.2 Replace `pending_action_after_path()`

After a provider supplies a reply path and `accept_reply_path()` succeeds, immediately drive the lookup machine until it produces one of:

```text
SendDatabaselookup
Complete
NeedExploratoryReplyPath only if the accepted path was invalidated before dispatch
```

Do not return `NeedExploratoryReplyPath` merely because that is what the old stub did.

The seam should not require callers to reach into `lookup_mut()` to obtain the next action.

## 7.3 Build scheduling when paths are absent

Add explicit composition outcomes for:

```text
NeedInboundExploratory
NeedOutboundExploratory
LookupReadyForTunnelDispatch
```

Do not encode these as free-form strings.

When no inbound reply path exists:

```text
lookup stays pending
 -> request bounded inbound exploratory build
```

When an inbound reply path exists but no usable outbound local role exists:

```text
lookup remains pending with reply path
 -> request bounded outbound exploratory build
```

Only when both are usable may the coordinator emit TunnelData for the lookup.

Do not fall back to direct transport delivery of the DatabaseLookup.

## 7.4 Short-build dispatch integration

Use existing production surfaces:

```text
ShortBuildStateMachine::prepare
 -> ShortBuildAction::Deliver
 -> ShortBuildI2npBridge
 -> standard I2NP type 25
 -> i2pr_transport::EncodedI2npMessage
 -> i2pr_transport::DeliveryRequest(target = first hop)
```

Do not hand-build STBM or the standard I2NP envelope.

The coordinator must retain the pending `ShortBuildStateMachine` until it receives the corresponding reply/outcome.

On successful local/native reply processing:

```text
state machine Established
 -> ShortBuildRegistrar::admit_established_machine
 -> pool slot
 -> activate slot into local role registry
```

Build failures update the existing pool failure accounting and remain bounded by the existing pool/build policies.

### Important Q2 boundary

The general inbound garlic reply path for an external OBEP is not implemented in this plan. Product code may expose a typed input for an already-decapsulated OTBRM payload, because that is what the existing build state machine consumes.

Do **not** implement a general garlic subsystem solely to make the temporary Emissary test look more live.

The independent native reference test may use a temporary test-only Emissary helper to expose the inner OTBRM for i2pr state-machine completion. That test must record this as a Q2 bypass and must not be reported as live external reply-to-Established evidence.

---

# 8. Phase D — outbound RouterInfo lookup through the real outbound exploratory role

## 8.1 Standard I2NP envelope

The daemon/runtime boundary owns standard I2NP header creation:

```text
message_id      = fresh nonzero runtime value
expiration      = bounded current-time + I2NP lifetime
body            = typed DatabaseLookup from LookupAction
```

Encode with the existing `i2pr-proto` standard encoder.

Do not add a second I2NP codec.

## 8.2 Tunnel delivery instruction

The outbound exploratory gateway must carry the `DatabaseLookup` with:

```text
DeliveryInstruction::Router {
    router = selected floodfill RouterHash
}
```

Then call:

```text
OutboundGatewayRole::forward_cells
```

with injected production CSPRNG.

Each returned TunnelData cell becomes a standard I2NP type-18 message targeted at the outbound tunnel's first remote hop through `i2pr-transport::DeliveryRequest`.

Do not send the DatabaseLookup itself to `DeliveryRequest`.

## 8.3 Transport-neutral result handling

The composition layer must distinguish:

```text
TunnelData prepared
transport queue accepted
no active first-hop link
queue/resource rejected
delivery deadline elapsed
```

A `NoActiveLink` may request/schedule transport dialing through the existing `i2pr-transport` ownership model, but this plan must not implement an NTCP2-specific dialer.

Required local tests:

```text
lookup_body_becomes_standard_i2np
lookup_standard_i2np_becomes_router_delivery_tunneldata
outbound_first_hop_delivery_targets_real_pool_peer
lookup_never_uses_direct_transport_to_floodfill
fragmented_lookup_if_ever_needed_uses_forward_cells
```

(DatabaseLookup should normally fit one cell; keep the generic multi-cell path because the production role already owns it.)

---

# 9. Phase E — inbound exploratory TunnelData dispatch and NetDB response ingestion

## 9.1 TunnelData routing

Add a narrow runtime-facing method that accepts a decoded standard I2NP `TunnelDataMessage` plus source-peer identity/time.

Routing rule:

```text
TunnelData.tunnel_id
 -> activated inbound local receive map
 -> LocalInboundEndpointRole::process
```

Unknown tunnel IDs fail closed and do not allocate a new reassembler/tunnel role.

Do not route by creator tunnel ID or inbound gateway receive ID. The local endpoint must be keyed by its explicit local receive tunnel ID.

## 9.2 Recovered I2NP dispatch

When `LocalInboundEndpointRole` completes a message, decode the recovered standard I2NP exactly once and support only the NetDB response types required here:

```text
DatabaseStore
DatabaseSearchReply
DeliveryStatus only if publication verification uses it
```

Unexpected message types are rejected/ignored with a typed bounded category. Do not add a generic router dispatcher in this plan.

## 9.3 DatabaseStore

For RouterInfo lookup success:

```text
DatabaseStore RouterInfoCompressed
 -> bounded gzip decompression
 -> RouterInfo decode
 -> existing RouterInfo validation
 -> RouterInfoLookup::handle_store
 -> bounded RouterInfoStore
```

The target key must match the active lookup target according to the existing lookup state machine.

Do not let a syntactically valid unrelated RouterInfo complete the active query.

## 9.4 DatabaseSearchReply

Route the existing parsed response into:

```text
RouterInfoLookup::handle_search_reply
```

Preserve existing candidate-count, suggested-hash, and deadline bounds.

The unauthenticated `from` field must not be treated as proof of source identity; use existing peer/session context only where the current lookup policy already requires it.

Required tests:

```text
inbound_tunneldata_unknown_receive_id_rejected
inbound_database_store_completes_matching_lookup
inbound_database_store_wrong_target_does_not_complete
inbound_database_search_reply_advances_lookup
inbound_expired_i2np_rejected
inbound_role_reassembly_state_survives_multiple_cells
```

---

# 10. Phase F — local RouterInfo publication through the exploratory path

Publication must use the same outbound exploratory data plane rather than inventing a direct floodfill send path.

## 10.1 Publication message

Use the typed `DatabaseStoreMessage` retained by the corrected `PublicationAttemptRecord`.

Create a standard I2NP envelope at the daemon/runtime boundary, then route it through:

```text
OutboundGatewayRole::forward_cells
 -> ROUTER delivery to selected floodfill
 -> TunnelData DeliveryRequest to first outbound hop
```

## 10.2 Correct RouterInfo payload

Require the publication `DatabaseStore` to carry the canonical compressed RouterInfo representation expected by an independent implementation.

The local state machine must not mark publication as independently observed merely because the local transport queue accepted the outbound TunnelData.

## 10.3 Verification

Preferred independent observation in 117-N:

```text
i2pr publishes local RouterInfo to Emissary floodfill
 -> Emissary accepts/stores it
 -> separate i2pr RouterInfo lookup asks for the local RouterHash
 -> Emissary returns DatabaseStore through i2pr inbound exploratory path
 -> i2pr validates the returned RouterInfo
```

This read-after-write check is stronger than introspecting a private reference map.

If the pinned Emissary test architecture makes a second lookup disproportionately complex, a test-only assertion against Emissary's production NetDB/profile storage is acceptable, but record that the publication observation was internal-native rather than round-trip lookup observation.

---

# 11. Phase G — mandatory all-i2pr deterministic production-seam integration test

Before touching the external reference checkout, add one deterministic integration test using only i2pr production components.

The purpose is to prove the **composition**, not protocol independence.

Required sequence:

```text
real ShortBuildStateMachine success trajectory(s)
 -> real ShortBuildRegistrar
 -> real ExploratoryPool entries
 -> activate outbound/inbound local roles
 -> NetDbSeam begin lookup
 -> real reply path supplied from pool
 -> LookupAction carries actual DatabaseLookup
 -> standard I2NP envelope
 -> OutboundGatewayRole -> TunnelData
 -> production i2pr remote-role simulator path exposes ROUTER delivery
 -> inject a canonical DatabaseStore through production inbound roles
 -> LocalInboundEndpointRole recovers exact standard I2NP
 -> RouterInfoLookup accepts target
 -> RouterInfoStore contains accepted RI
```

Do not manually call AES helpers inside this terminal integration test. Use the production role APIs.

Add a second path for `DatabaseSearchReply` only if the first implementation cannot demonstrate iterative lookup state with existing smaller unit tests.

The deterministic test must not use the test-only pool placeholder registration APIs. It must use real `EstablishedMaterial` from successful short-build state-machine paths or a production-equivalent constructor already used by Plan 116 acceptance.

---

# 12. Phase H — mandatory pinned Emissary native mixed-router checkpoint (117-N)

## 12.1 Why Emissary

Use upstream Emissary, pinned exactly to:

```text
repository = https://github.com/eepnet/emissary.git
revision   = 9b43484a21d5a1291c4881cdae62a36c527f8c0f
package    = emissary-core 0.4.0
```

Do not silently move the pin during execution.

If the pin no longer builds because of registry/toolchain drift, record the build failure distinctly. Do not switch to a moving reference in the same pass.

## 12.2 Temporary checkout only

Use the Plan 115 Q0 pattern:

```bash
set -euo pipefail
I2PR_ROOT="$(git rev-parse --show-toplevel)"
WORK="$(mktemp -d)"
git clone https://github.com/eepnet/emissary.git "$WORK/emissary"
git -C "$WORK/emissary" checkout --detach 9b43484a21d5a1291c4881cdae62a36c527f8c0f
ln -s "$I2PR_ROOT" "$WORK/emissary/i2pr-under-test"
```

Do not vendor or submodule Emissary.

Record the SHA-256 of the final temporary patch **before deleting the checkout**. The previous Q0 bookkeeping miss must not repeat.

## 12.3 Allowed temporary patch

The patch may:

- add path dev-dependencies on the minimum i2pr crates;
- add test-only helper methods under `#[cfg(test)]` inside Emissary modules where Rust privacy otherwise prevents calling the production handler;
- add one or a very small number of focused tests in existing Emissary test modules.

The helper must delegate directly to production parsing/crypto/NetDB logic. Do not duplicate Emissary algorithms in the test patch.

Do not commit any reference patch to i2pr.

## 12.4 Minimum reference topology

Use the smallest topology that executes real remote roles:

```text
outbound: i2pr creator -> Emissary OBEP
inbound:  Emissary IBGW -> Emissary Participant -> i2pr creator endpoint
```

A second inbound participant is unnecessary unless the pinned API requires it.

The floodfill responder may share a reference router context with one of the above roles if that keeps the test simple and does not bypass the OBEP ROUTER delivery decision. Otherwise instantiate one additional Emissary floodfill context.

## 12.5 Build establishment

For every remote Emissary hop:

1. use Emissary's generated router hash and X25519 static public key;
2. construct the production i2pr `ShortBuildPath`;
3. generate the build with i2pr `ShortBuildStateMachine`;
4. wrap the outgoing STBM through `ShortBuildI2npBridge`;
5. parse it with Emissary's standard I2NP parser;
6. invoke Emissary's production `handle_short_tunnel_build()`;
7. require native admission/role creation.

For the outbound OBEP response, the pinned implementation garlic-wraps the OTBRM. Since general garlic delivery is outside Plan 117, the temporary reference patch may expose or unwrap the inner OTBRM using Emissary's own test-side crypto/state and feed that exact payload to the i2pr state machine.

Record explicitly:

```text
outbound_build_reply_transport = native-emissary-production-reply-with-test-only-q2-decapsulation
Q2_live_return_established = not-proven
```

Do not claim Q2.

For the inbound build, chain the production Emissary short-build handlers in their natural forwarding order and give the resulting reply payload to i2pr's existing state machine.

## 12.6 Independent outbound lookup

After i2pr has activated the outbound local role:

```text
i2pr typed DatabaseLookup
 -> i2pr standard I2NP
 -> i2pr OBGW TunnelData preprocessing
 -> Emissary production OBEP TunnelData processing
 -> Emissary returns ROUTER delivery
```

Require:

```text
Emissary destination router == selected floodfill
Emissary delivered message type == DatabaseLookup
Emissary standard payload parses with its own DatabaseLookup parser
key == i2pr target
from == i2pr real inbound gateway
reply tunnel id == i2pr real inbound gateway receive tunnel
lookup type == RouterInfo
```

This proves the lookup actually survives the independent tunnel endpoint, not just i2pr's own inverse transform.

## 12.7 Independent floodfill response

Feed the Emissary-produced `DatabaseLookup` into Emissary's production floodfill NetDB handler.

Seed the reference NetDB with a deterministic valid RouterInfo for the requested key so the success path returns `DatabaseStore`.

Require the reference response to target exactly the reply gateway/tunnel from the i2pr request.

A separate miss-path `DatabaseSearchReply` test is useful but not mandatory if it substantially expands the temporary patch. Local i2pr tests must already cover the DSRM state transition.

## 12.8 Independent inbound return

Feed the Emissary floodfill's tunnel reply into the production Emissary IBGW and participant roles created by the inbound short build.

The final TunnelData is then handed to i2pr's production `LocalInboundEndpointRole`.

Require:

```text
i2pr endpoint recovers one standard I2NP message
message type == DatabaseStore
RouterInfo key == requested target
existing i2pr validation accepts it
RouterInfoLookup reaches Success
RouterInfoStore contains the target
```

This is the core 117-N mixed-router result.

## 12.9 Independent publication

Send i2pr's local RouterInfo `DatabaseStore` through the same outbound path and require Emissary floodfill acceptance.

Prefer read-after-write verification as described in Phase F.

At minimum record:

```text
publication_emissary_parse = passed
publication_emissary_validation = passed
publication_key_matches_i2pr_local = true
```

## 12.10 Native reference stage taxonomy

Record the highest stage reached separately for lookup and publication.

Lookup stages:

```text
117n_reference_checkout_pinned
117n_short_builds_native_accepted
117n_i2pr_tunnels_activated
117n_lookup_i2pr_tunneldata_generated
117n_emissary_obep_tunneldata_accepted
117n_emissary_obep_database_lookup_exposed
117n_emissary_floodfill_lookup_accepted
117n_emissary_database_store_reply_generated
117n_emissary_inbound_tunnel_processed
117n_i2pr_inbound_endpoint_recovered
117n_i2pr_routerinfo_validated
117n_i2pr_lookup_success
```

Publication stages:

```text
117n_publication_i2pr_database_store_generated
117n_publication_emissary_obep_accepted
117n_publication_emissary_floodfill_accepted
117n_publication_independently_observed
```

Failure categories must name the exact layer. Do not record generic `interop-failed`.

## 12.11 Attempt budget

Use a strict native-reference budget:

```text
1 baseline compile/test attempt
2 narrowly scoped correction attempts for API/test-patch mistakes
1 confirmation run
STOP
```

If the independent production handler exposes a reproducible i2pr protocol defect, authorize exactly one localized product correction and one focused confirmation.

Do not create a second reference matrix in this plan.

---

# 13. Phase I — authenticated transport checkpoint (117-X), only when already runnable

After 117-L and 117-N are green, inspect—not rebuild—the existing qualified host lanes.

Allowed outcome A:

```text
existing lane runnable
 -> execute one bounded authenticated delivery probe
 -> record Q1/Q2 stages
```

Allowed outcome B:

```text
existing lane unavailable due host privilege/namespace/VM constraint
 -> record 117-X = deferred-host-lane-unavailable
 -> stop external work
```

Do not:

- write a new Python orchestrator;
- add Docker-only infrastructure;
- spend another development cycle on namespace capability probing;
- activate normal-daemon NTCP2 to make the test convenient;
- reinterpret a host capability failure as an I2P protocol defect.

A test-only development transport is acceptable if it already exists and can be invoked without protocol changes.

Authenticated external success must only be claimed if an actual transport session carried the messages.

---

# 14. Local acceptance tests

Required test names may vary, but the following semantic coverage is mandatory.

## NetDB message ownership

```text
lookup_send_action_owns_database_lookup
lookup_send_action_round_trips_exact_body
publication_attempt_owns_database_store
publication_routerinfo_payload_is_valid_gzip
```

## Activation ownership

```text
outbound_pool_material_activates_once
inbound_pool_material_activates_once
inbound_reply_path_available_after_activation
activation_does_not_clone_persistent_layer_keys
expiry_removes_runtime_role
```

## Daemon seam

```text
accepted_reply_path_immediately_produces_send_action
missing_inbound_path_requests_inbound_build
missing_outbound_role_requests_outbound_build
direct_floodfill_transport_lookup_is_never_emitted
```

## Outbound lookup

```text
database_lookup_routes_as_router_delivery_through_outbound_gateway
first_hop_tunneldata_targets_outbound_first_hop
lookup_standard_i2np_survives_local_obep
```

## Inbound response

```text
matching_database_store_returns_through_inbound_endpoint
matching_database_store_completes_lookup
wrong_database_store_key_does_not_complete_lookup
database_search_reply_advances_existing_lookup
unknown_local_tunnel_id_rejected
```

## Publication

```text
publication_database_store_routes_through_outbound_exploratory
publication_message_reuses_exact_local_routerinfo_snapshot
```

## Terminal all-i2pr trajectory

One test must prove:

```text
real registered+activated exploratory pair
 -> lookup action with actual DatabaseLookup
 -> outbound TunnelData
 -> ROUTER delivery
 -> inbound DatabaseStore TunnelData
 -> local endpoint
 -> lookup Success
 -> target present in store
```

---

# 15. Security and anonymity invariants

1. DatabaseLookup must not bypass the exploratory tunnel just because a floodfill transport link already exists.
2. The reply `from` field must name the real inbound gateway when tunnel delivery is requested.
3. The reply tunnel ID must be the inbound gateway's receive tunnel ID, not the creator/local endpoint tunnel ID.
4. The local inbound TunnelData receive key is the explicit local endpoint receive ID.
5. Layer keys remain single-owner secret state; do not persistently clone them between pool and runtime registry.
6. `Debug`/tracing must not print raw layer keys or complete TunnelData plaintext/ciphertext.
7. Lookup response RouterInfo goes through the existing validation/bounded-store path.
8. Unknown TunnelData IDs and unknown lookup correlation state fail closed.
9. Publication delivery queue acceptance is not equivalent to independent publication observation.
10. `ReplyEncryption::None` is restricted here to the first RI lookup integration scope; do not reuse this decision for future LeaseSet/client lookup work.
11. All new maps/registries have explicit capacity inherited from existing pool/lookup limits.
12. Expired tunnels cannot be selected or used after runtime role eviction.

---

# 16. Dependency and architecture rules

Maintain this direction:

```text
i2pr-proto
   ^
i2pr-netdb        i2pr-tunnel        i2pr-transport
      \                |                  /
       \               |                 /
        -------- i2pr-daemon / composition --------
                         |
                    i2pr-runtime
```

Do not make `i2pr-tunnel` depend on daemon/runtime/transport.

Do not make `i2pr-netdb` depend on tunnel or transport.

The daemon composition root may depend on all three because Plan 117 is specifically where those pure state machines are joined.

If a transport-neutral coordinator belongs more cleanly in a small existing runtime module, that is acceptable, but do not create a new crate merely for Plan 117.

---

# 17. Implementation order

Execute in exactly this order:

```text
117-A  data-bearing LookupAction + PublicationAttemptRecord
  -> targeted netdb tests green

117-B  pool activation seam + bounded local role registry
  -> ownership/expiry tests green

117-C  replace NetDbSeam post-reply-path stub
  -> path/build scheduling tests green

117-D  outbound DatabaseLookup -> standard I2NP -> OBGW TunnelData
  -> no direct floodfill transport bypass

117-E  inbound TunnelData -> local endpoint -> DatabaseStore/DSRM state machine
  -> matching lookup success

117-F  publication -> outbound exploratory TunnelData
  -> canonical gzip DatabaseStore

117-G  all-i2pr deterministic terminal integration test
  -> exact product composition green

117-H  pinned Emissary native mixed-router lookup + publication checkpoint
  -> independent native evidence

117-I  optional existing authenticated transport checkpoint
  -> pass or explicit environment defer

117-J  status / roadmap / architecture authority update
```

Do not begin with the Emissary test. External/native evidence is only meaningful after the product composition code exists.

---

# 18. Validation commands

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

Run the historical NTCP2 vector/interoperability scripts only if they are still part of the repository-wide required CI command set. Do not modify them to make Plan 117 pass.

Before the independent reference probe, run the terminal all-i2pr integration test directly with `--nocapture` and record its exact test name/result.

For the Emissary temporary checkout, record:

```text
pinned commit
emissary-core version
final temporary patch SHA-256
exact cargo test command
highest lookup stage
highest publication stage
sanitized message lengths/types only
```

Do not retain secret keys or raw TunnelData payloads in the evidence file.

---

# 19. Explicit acceptance criteria

Plan 117 local/native construction may be considered complete only when **all** are true:

1. `LookupAction::SendDatabaselookup` owns the real typed lookup payload/body.
2. `encoded_len` is not the sole outbound lookup data.
3. `PublicationAttemptRecord` owns the real typed DatabaseStore publication message/body.
4. publication RouterInfo payload is independently valid gzip and decompresses to the exact local RouterInfo snapshot.
5. accepting a reply path in `NetDbSeam` no longer returns the old placeholder solely because of the stub.
6. the daemon composition root has a bounded path to request inbound/outbound exploratory construction.
7. short-build dispatch uses `ShortBuildStateMachine` + `ShortBuildI2npBridge` + `DeliveryRequest`; no hand-built STBM envelope.
8. a successful build is registered through `ShortBuildRegistrar::admit_established_machine`.
9. registered material activates into exactly one local role owner.
10. activation does not leave a persistent cloned LayerKeys copy.
11. inbound reply-path metadata remains usable after local role activation.
12. pool expiry/failure removes the corresponding activated role.
13. the lookup standard I2NP message is sent as ROUTER delivery through an outbound exploratory tunnel.
14. no direct transport-to-floodfill DatabaseLookup fallback exists.
15. TunnelData first-hop delivery targets the actual established outbound first hop.
16. inbound TunnelData is routed by the local inbound endpoint receive tunnel ID.
17. unknown inbound TunnelData IDs fail closed.
18. recovered DatabaseStore reaches the existing RouterInfo validation/store path.
19. a matching DatabaseStore completes the correct active lookup.
20. a wrong-key DatabaseStore cannot complete the active lookup.
21. DatabaseSearchReply can advance the existing lookup without bypassing existing bounds.
22. publication DatabaseStore traverses the outbound exploratory data plane.
23. publication is not marked independently observed from local queue acceptance alone.
24. one terminal all-i2pr integration test proves actual lookup body -> outbound TunnelData -> inbound response -> lookup success.
25. the terminal all-i2pr test uses production role APIs rather than manual AES operations.
26. pinned Emissary independently accepts i2pr short-build construction for every remote role used by the reference topology.
27. pinned Emissary independently accepts i2pr-produced outbound TunnelData at its production OBEP.
28. Emissary exposes a standard DatabaseLookup whose key/from/reply tunnel fields match i2pr intent.
29. Emissary floodfill production NetDB handles that DatabaseLookup.
30. the reference response is routed to the exact i2pr inbound gateway/tunnel.
31. Emissary production inbound role(s) process the reply into TunnelData for i2pr.
32. i2pr local inbound endpoint recovers the reference-produced DatabaseStore.
33. i2pr validates/stores the returned RouterInfo and the lookup reaches Success.
34. i2pr local RouterInfo publication is accepted by the pinned Emissary floodfill.
35. publication has an independent native observation (prefer read-after-write).
36. temporary Emissary patch hash is recorded before checkout deletion.
37. Q1/Q2 are not claimed from the in-process native lane.
38. if authenticated transport lane is unavailable, the exact environment limitation is recorded once and no new harness is built.
39. normal-daemon NTCP2 remains disabled/unenableable unless a separate future activation authority changes it.
40. workspace validation passes apart from explicitly documented pre-existing environment-only checks unrelated to Plan 117.

---

# 20. Failure taxonomy

Use typed/localized result labels.

Local composition:

```text
failed-lookup-action-body-ownership
failed-publication-body-ownership
failed-routerinfo-publication-compression
failed-tunnel-material-activation
failed-runtime-role-registration
failed-reply-path-after-activation
failed-netdb-seam-post-path-advance
failed-short-build-dispatch-composition
failed-outbound-tunneldata-composition
failed-inbound-tunneldata-routing
failed-netdb-response-ingestion
failed-publication-tunnel-dispatch
```

Native reference:

```text
failed-reference-checkout
failed-reference-build
failed-emissary-short-build-obep
failed-emissary-short-build-ibgw
failed-emissary-short-build-participant
failed-emissary-obep-tunneldata
failed-emissary-database-lookup-parse
failed-emissary-floodfill-response
failed-emissary-inbound-return
failed-i2pr-reference-response-decode
failed-i2pr-reference-routerinfo-validation
failed-emissary-publication-parse
failed-emissary-publication-validation
```

External transport:

```text
passed-authenticated-transport
failed-authenticated-session
failed-post-auth-i2np-delivery
deferred-host-lane-unavailable
```

Never collapse an environment limitation into a protocol defect.

---

# 21. Closure tokens

Successful local + native completion with no authenticated host lane:

```text
plan_116                              = passed-final-local-closure
plan_117_local_composition            = passed
plan_117_native_reference             = passed-emissary-mixed-router-netdb
plan_117_authenticated_transport      = deferred-host-lane-unavailable
plan_117                              = local-native-complete-external-deferred
milestone4b_authenticated_external    = blocked
router_construction                   = may-continue
normal_daemon_ntcp2                   = disabled-and-unenableable
ntcp2                                 = experimental-non-advertised
```

If an authenticated lane also passes:

```text
plan_117_authenticated_transport      = passed
plan_117                              = passed-qualified-exploratory-netdb-integration
milestone4b_authenticated_external    = eligible-for-closure-review
```

If 117-N exposes an affirmative protocol defect:

```text
plan_117_native_reference = failed-<exact-stage>
```

Localize that single defect, correct it once, and rerun the focused native checkpoint. Do not create a broad validation branch.

---

# 22. Post-Plan-117 direction

After 117-L and 117-N pass, the router has enough tunnel + NetDB composition to begin the next local product layer even if authenticated external transport remains deferred by the host environment.

The next roadmap decision may then evaluate Milestone 6 local construction:

```text
Destination lifecycle
 -> destination tunnel pools
 -> garlic
 -> LeaseSet creation/publication/lookup
 -> local destination routing
 -> minimal streaming
```

Do not start SAM or I2CP merely because Plan 117 finishes; those remain downstream of a functioning destination/streaming core.

Do not use Plan 117 completion as evidence that the normal daemon has production transport connectivity. That remains a separate transport activation decision.

---

# 23. Small non-blocking carry-forward from Plan 116

A post-closure audit noted one additional bounded reassembly hardening case: after a follow-on has already declared `is_last=true` at sequence `N`, a later new fragment with sequence `> N` should be rejected as contradictory terminal-sequence state.

This is **not** a Plan 117 prerequisite and must not reopen Plan 116.

If `fragment.rs` is touched for a directly related reason during Plan 117, add the narrow invariant/test opportunistically. Otherwise record it in the security/hardening backlog and continue.

---

# 24. Handoff rule

Execute the local phases first. The first acceptable stopping point is not an environment error; it is:

```text
117-L green
117-N attempted to its strict bounded budget
117-X either genuinely executed or explicitly deferred without new harness work
```

The goal is to convert the existing protocol pieces into a functioning router composition and obtain one independent native proof, not to spend another development cycle proving that the current host cannot run privileged networking infrastructure.
