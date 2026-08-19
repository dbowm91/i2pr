# Plan 122 — Milestone 6 destination routing and NetDB composition

## Status

- **Ready after Plan 121 closes**.
- Date: **2026-08-19**.
- Parent roadmap: [`118-123-milestone6-router-construction-roadmap.md`](118-123-milestone6-router-construction-roadmap.md).
- Preconditions:
  - Plan 119 typed Standard LeaseSet2/NetDB support;
  - Plan 120 local destinations and dedicated tunnel pools;
  - Plan 121 ECIES-X25519 Garlic/session layer.
- Primary output: first complete local end-to-end I2P destination message path.

## Objective

Compose the already-built NetDB, destination tunnel pools, ECIES Garlic layer,
and tunnel data plane into an actual destination routing pipeline.

At closure, two local i2pr destinations must exchange authenticated encrypted
application messages through **dedicated outbound and inbound tunnel paths**, not
through a direct client shortcut.

Required conceptual path:

```text
Destination A application payload
 -> resolve B Standard LeaseSet2 through production NetDB interfaces
 -> select valid B Lease2
 -> encode payload as nested I2NP Data message
 -> ECIES Garlic Clove / Garlic message for B
 -> A outbound destination tunnel
 -> OBEP TUNNEL delivery to B lease gateway + tunnel id
 -> deterministic router-delivery seam
 -> B inbound gateway / participants
 -> B local endpoint
 -> destination-owner dispatch
 -> B ECIES session manager
 -> authenticated Garlic clove
 -> exact Data payload at Destination B
```

Then exercise the reverse path, including New Session Reply / Existing Session
state where Plan 121 requires it.

The only omitted layer may be the unavailable authenticated router transport
between an outbound endpoint and the remote inbound gateway. That boundary must
remain explicit and must use the same typed delivery request the real transport
adapter will eventually consume.

---

# 1. Normative routing model

Primary references:

```text
https://i2p.net/en/docs/specs/common-structures/
https://i2p.net/en/docs/specs/i2np/
https://i2p.net/en/docs/specs/tunnel-message/
https://i2p.net/en/docs/specs/ecies/
```

For ordinary destination traffic:

1. the sender resolves the recipient Destination hash to a validated LeaseSet2;
2. the LS2 advertises one or more inbound tunnel gateways and tunnel IDs;
3. the sender encrypts the destination message to the LS2's X25519 key;
4. the sender places the resulting Garlic I2NP message into one of its own
   outbound destination tunnels;
5. the outbound endpoint forwards that I2NP message to the selected remote
   inbound gateway/tunnel according to `TUNNEL` delivery instructions;
6. the recipient's inbound tunnel ends at the recipient's local router/client
   context, where the Garlic message is decrypted by the owning destination.

Do not route the plaintext application message to the remote lease gateway.
Do not bypass the sender's outbound destination tunnel.

---

# 2. Phase A — typed LeaseSet2 lookup requests

Plan 119 should leave a typed lookup contract. Complete its production use for
remote destinations.

Introduce or finalize a request class such as:

```text
NetDbLookupKind::RouterInfo
NetDbLookupKind::LeaseSet2
```

or equivalent.

Destination lookup semantics:

```text
lookup key = SHA256(remote Destination)
required result = validated Standard LeaseSet2
```

Use the existing NetDB lookup state machine and exploratory-tunnel composition.
Do not create a second client-specific DHT implementation.

A client destination requests resolution; the router-wide NetDB subsystem owns
lookup/retry/floodfill selection. The resulting LS2 is returned to the client
layer as validated public data.

Bounds:

```text
max concurrent remote LS2 lookups per local destination
max router-wide concurrent LS2 lookups
max pending outbound messages waiting for a lookup
max pending bytes waiting for a lookup
lookup timeout/retry budget
```

Deduplicate simultaneous lookups for the same remote destination where practical.

Required tests:

```text
remote_ls2_cache_hit_avoids_new_lookup
concurrent_same_key_requests_are_bounded/deduplicated
lookup_success_unblocks_pending_message
lookup_failure_releases_pending_state
wrong_entry_type_does_not_satisfy_ls2_lookup
```

---

# 3. Phase B — local LeaseSet2 publication composition

Take Plan 120's signed publication-ready local LS2 and connect it to the existing
NetDB publication machinery.

Required flow:

```text
Destination LS2 lifecycle generates newer LS2
 -> typed client publication request
 -> router NetDB publication coordinator
 -> existing exploratory outbound tunnel composition
 -> DatabaseStore type 3
```

Local publication must use exploratory/router NetDB paths rather than a
destination's own application tunnel merely because both are available.

The client layer should receive only bounded status events:

```text
queued
attempted
accepted/local-composition-success
retry-scheduled
failed
```

Do not expose floodfill/router internals directly to the destination.

In the local deterministic environment, publication acceptance may terminate at
the existing all-i2pr NetDB seam. Do not claim external floodfill publication.

---

# 4. Phase C — Lease2 selection policy

Given a validated remote LS2, select one usable lease.

Minimum policy:

```text
exclude expired leases
exclude leases expiring inside a configurable safety margin
require nonzero/valid tunnel id according to routing policy
require usable RouterInfo for gateway when the downstream delivery path needs it
select among remaining leases using caller-supplied CSPRNG/deterministic RNG seam
```

Do not always choose index zero; that creates unnecessary fingerprintability and
load concentration.

Avoid building a large profiling subsystem here. Existing peer observations may
be used only if already available through clean contracts.

Selection output must be typed:

```text
SelectedLease {
    destination_hash,
    gateway_router_hash,
    tunnel_id,
    lease_expiration,
    leaseset_version/published marker,
}
```

or equivalent.

This allows the send path to detect that a queued selection became stale before
actual delivery.

---

# 5. Phase D — application payload -> I2NP Data

Define the ordinary local destination payload carrier from Plan 120 as a strict
I2NP `Data` message body.

If `Data` is currently represented only as an opaque/deferred body, implement the
minimal typed body required by the I2NP specification while preserving bounds.

The application bytes must remain distinguishable from the ECIES encrypted
outer bytes.

Target layering:

```text
application bytes
 -> I2NP Data message
 -> ECIES Garlic Clove with LOCAL destination delivery semantics
 -> ECIES encrypted Garlic message
```

Do not put raw streaming/application bytes directly into `TunnelData`.

---

# 6. Phase E — first-message LeaseSet2 bundling and binding

For the ordinary bound ECIES New Session path, include the sender's current
LeaseSet2 when needed so the recipient can bind the sender's static X25519 key to
its full Destination and route replies.

The ECIES specification recommends capturing a sender Destination from a
DatabaseStore/LeaseSet found while processing the New Session payload.

Implement a narrow, typed bundling policy:

```text
New Session payload
  clove 1: application Data message
  clove 2: sender current Standard LeaseSet2 DatabaseStore when required by
           session-binding/reply routing policy
```

Do not bundle stale or unverified local LS2.

Recipient behavior:

```text
decrypt/authenticate NS
 -> process DatabaseStore LS2 clove through local NetDB validation/store path
 -> verify LS2 X25519 key matches the static key authenticated in the NS
 -> bind pending ECIES session to sender Destination
 -> only then treat the full sender identity as known for reply routing
```

A mismatched LS2/static key must fail the binding and must not poison NetDB or
session state.

---

# 7. Phase F — compose outbound destination tunnel delivery

Reuse `OutboundGatewayRole`/TunnelData preprocessing from Plan 116, but add a
client-destination composition seam that chooses a destination-owned outbound
role rather than the exploratory pool.

Required outer delivery instruction at the outbound endpoint:

```text
TUNNEL delivery
  router = selected Lease2.gateway_router_hash
  tunnel = selected Lease2.tunnel_id
  message = encrypted Garlic I2NP message
```

The sender-side `DeliveryRequest.target` to the first hop remains the first hop
of **A's outbound tunnel**. The TUNNEL destination above is nested delivery
metadata recovered at A's OBEP.

Keep these identities distinct in tests:

```text
A first outbound hop        = P
A outbound endpoint         = E
B lease inbound gateway     = G
B advertised receive tunnel = T
B destination hash          = H_B
```

Assertions must prove they are not accidentally conflated.

---

# 8. Phase G — router-delivery boundary

In a fully networked router the outbound endpoint would send the recovered
I2NP message to `G` over an authenticated router transport, addressed to tunnel
`T`.

That execution lane is unavailable today. Plan 122 may use one explicit,
runtime-neutral seam:

```text
OutboundEndpointAction::DeliverTunnel {
    router: G,
    tunnel_id: T,
    message: Garlic(...),
}
```

(or the repository's existing equivalent)

fed directly to the remote `InboundGatewayRole` fixture.

Requirements:

- the boundary occurs **after** real outbound tunnel processing;
- no code decrypts or rewrites Garlic at the boundary;
- no code bypasses `T` or substitutes a local endpoint call;
- the same typed output is suitable for the future transport adapter;
- the test labels this as `authenticated-router-link-bypassed-local-seam`, not
  external interoperability.

Do not create socket/namespace infrastructure for this plan.

---

# 9. Phase H — destination-owned inbound dispatch

The existing local inbound endpoint dispatch is router/NetDB oriented. Extend
runtime ownership so a recovered inbound message can be dispatched to the
correct local destination without putting client policy into `i2pr-tunnel`.

Each activated destination inbound tunnel must retain public owner metadata at
the composition layer:

```text
inbound tunnel receive id / slot
 -> DestinationId
```

Secret layer keys remain owned by the tunnel role.

When the local endpoint recovers the Garlic I2NP message:

```text
DataPlaneRegistry/local composition
 -> identify owning DestinationId
 -> bounded i2pr-client inbound queue
 -> that destination's EciesSessionManager
```

Do not attempt all local destination private keys until one decrypts. The owning
inbound tunnel already supplies the correct local destination context; use it as
an isolation boundary.

Required tests:

```text
inbound_tunnel_dispatches_only_to_owner_destination
wrong_destination_cannot_observe_ciphertext_as_decrypt_candidate
expired_removed_tunnel_has_no_destination_owner_mapping
owner_mapping_removed_atomically_with_role
```

---

# 10. Phase I — authenticated Garlic processing and local delivery

At the destination:

```text
receive encrypted Garlic
 -> Plan 121 session classify/decrypt
 -> process payload blocks
 -> route DatabaseStore clove to bounded local NetDB interface when present
 -> route LOCAL Data clove to owning destination application queue
```

Never deliver application bytes before AEAD/session authentication completes.

Application queue behavior:

```text
bounded count
bounded bytes
explicit backpressure/drop policy
no secret/session-state logging
```

Unknown forward-compatible ECIES blocks should follow Plan 121/spec behavior;
unknown Garlic delivery instructions should not be guessed.

---

# 11. Phase J — reply routing

After receiving A's initial message and bundled LS2, B should know:

```text
A full Destination
A validated current LeaseSet2
A static X25519 key matching NS binding
```

Therefore B can select one of A's inbound leases and send NSR/application reply
through B's own outbound destination tunnel.

Required reverse path:

```text
B response
 -> choose A Lease2
 -> NSR or Existing Session Garlic
 -> B outbound destination tunnel
 -> typed TUNNEL delivery to A inbound gateway/id
 -> local router-delivery seam
 -> A inbound destination tunnel
 -> A owner dispatch
 -> A ECIES session processing
 -> application bytes
```

This is the first required round trip.

---

# 12. LeaseSet expiry/refresh during sends

Before composing a send, verify that the selected lease and LS2 are still valid
at caller-supplied `now`.

If stale:

```text
invalidate selection
 -> request/attach to bounded refresh lookup
 -> retain/drop pending payload according to queue policy
```

Do not send to an expired lease simply because it was valid when the application
enqueued the message.

A successful newer LS2 replacement must not destroy an active ECIES session;
crypto session binding is to the Destination/static key. If the new LS2 rotates
the X25519 key, define a safe transition: old session may continue only while
policy/spec permits; new sessions must use the current key. Do not silently bind
one static key to a different Destination.

---

# 13. Required deterministic end-to-end trajectories

## 13.1 New Session A -> B

Use real established destination tunnels for both sides.

```text
A and B create destination runtimes
A/B have dedicated inbound + outbound pools
A/B signed LS2 are validated/stored

A queues payload "hello"
 -> A resolves B LS2
 -> selects B lease G_B/T_B
 -> creates Data I2NP
 -> creates bound ECIES New Session Garlic
 -> bundles A LS2 as required
 -> A outbound TunnelData path
 -> A OBEP emits TUNNEL(G_B,T_B,Garlic)
 -> explicit local router-delivery seam
 -> B IBGW/participant/local endpoint
 -> B owner dispatch
 -> B authenticates NS
 -> B stores/validates A bundled LS2 and binds static key
 -> B application receives exactly "hello"
```

## 13.2 Reply B -> A

```text
B queues "world"
 -> selects A lease learned from validated bundled LS2
 -> emits NSR/appropriate Plan 121 reply Garlic
 -> B outbound tunnel
 -> A inbound tunnel
 -> A authenticates reply
 -> A application receives exactly "world"
```

## 13.3 Existing Session

After session establishment:

```text
A -> B payload 2 using Existing Session
B -> A payload 2 using Existing Session
```

Require exact-once delivery.

## 13.4 Faults

Inject at least:

```text
expired remote lease
wrong tunnel id
removed inbound destination role
tampered Garlic ciphertext
bundled LS2/static-key mismatch
unknown local Destination owner
full application queue
lookup timeout
```

All fail boundedly without cross-destination leakage.

---

# 14. Optional independent reference checkpoint

If the current environment can run an in-process native destination/LeaseSet2
consumer without new infrastructure, one narrow reference test is useful after
the all-i2pr path is green.

This is **optional for Plan 122 local closure**.

It must not reopen the Plan 117 validation loop.

Allowed evidence labels remain distinct:

```text
parser-compatible
native-in-process-destination-consumer
authenticated-router-link
mixed-router-live
```

Do not promote one to another.

---

# 15. Documentation updates

On closure update at least:

```text
README.md
AGENTS.md
docs/architecture/i2pr-client.md
docs/architecture/i2pr-tunnel.md for destination owner dispatch boundary
docs/protocol-support.md / support authority
specs/support.toml
```

Document the explicit authenticated-router-link bypass used in deterministic
Plan 122 tests.

---

# 16. Validation commands

At minimum:

```bash
cargo fmt --all --check
cargo test --locked -p i2pr-client --all-targets
cargo test --locked -p i2pr-netdb --all-targets
cargo test --locked -p i2pr-tunnel --all-targets
cargo test --locked -p i2pr-daemon --all-targets
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
```

No Python harness or live NTCP2 activation is required.

---

# 17. Explicit acceptance criteria

Plan 122 is complete only when:

- [ ] Remote Destination hashes resolve to typed validated Standard LeaseSet2
      through the existing NetDB lookup subsystem.
- [ ] LS2 lookup/pending-send state is bounded and deduplicated where practical.
- [ ] Local signed LS2 publication is connected to existing NetDB publication
      contracts.
- [ ] Lease selection excludes expired/near-expiry entries and is not fixed to
      index zero.
- [ ] Application bytes are encoded as I2NP Data and then protected inside an
      ECIES Garlic Clove/Message.
- [ ] Initial bound New Session can bundle the sender's current validated LS2.
- [ ] Recipient validates bundled LS2 and requires its X25519 key to match the
      static key authenticated in the New Session before binding full sender
      Destination identity.
- [ ] Outbound destination traffic uses a destination-owned outbound tunnel.
- [ ] OBEP emits TUNNEL delivery to the selected remote Lease2 gateway/id.
- [ ] The only transport omission is an explicit typed local router-delivery
      boundary after OBEP processing.
- [ ] Remote inbound gateway/participant/local-endpoint processing is real
      production tunnel-data-plane code.
- [ ] Inbound destination tunnel ownership dispatches ciphertext to exactly one
      local destination context.
- [ ] Application plaintext is delivered only after ECIES authentication.
- [ ] A -> B New Session payload is delivered exactly once.
- [ ] B -> A NSR/reply path works through B outbound and A inbound destination
      tunnels.
- [ ] Existing Session payloads work both directions over the same routing
      architecture.
- [ ] LS2 expiry/refresh and stale-selection behavior are deterministic.
- [ ] Tamper, wrong owner, wrong tunnel id, lookup timeout, and queue saturation
      tests fail boundedly.
- [ ] No direct client-to-client shortcut, SAM, I2CP, streaming, HTTP/SOCKS,
      normal-daemon NTCP2 activation, or new external harness is introduced.
- [ ] Workspace validation is green.

## Handoff

On closure:

```text
plan_122 = passed-local-destination-routing
milestone6_destination_message_path = passed-local-product
next = plans/123-m6-minimal-streaming-core.md
```
