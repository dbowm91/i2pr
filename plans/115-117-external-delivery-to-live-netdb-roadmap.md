# Plans 115-117 roadmap: independent short-build evidence -> tunnel data plane -> live exploratory NetDB

## Status

- Date: 2026-08-17.
- This is a **gated roadmap**, not one monolithic implementation plan.
- Plan 115 Q0 construction + native OBEP reply has **passed** locally
  against pinned Emissary. The original Branch E closure
  (`blocked-no-bounded-independent-consumer-seam`) is superseded
  by this Q0 completion but is preserved as historical context in
  [`plans/115-status.md`](115-status.md). The full Q0 acceptance
  for the 115-117 amendment requires both Q0 native Emissary
  acceptance AND Q1/Q2 transport evidence; Q0 has now closed for
  the construction+OBEP-reply stage only.
- Plan 116 remains **gated** on a future Plan 115-style Q1/Q2
  pass on a host where the Plan 046 rootless sealed-namespace
  lane or the Plan 048/049 Multipass recovery lane is runnable.
- Plan 117 remains **gated** on the future Plan 116 tunnel
  data-plane boundary.
- Parent roadmap: [`plans/000-mvp-roadmap.md`](000-mvp-roadmap.md).

## Why this roadmap exists

The repository has accumulated substantial local tunnel-build correctness work while live transport evidence remains constrained by the development environment. Continuing to add short-build validation machinery would have diminishing product value.

The next three gates therefore separate three different questions:

```text
Plan 115: Does an independent implementation consume our short-build protocol bytes?
Plan 116: Can i2pr actually move I2NP traffic through constructed tunnels?
Plan 117: Can a real exploratory pair carry NetDB work and close the live Milestone 4 dependency?
```

This sequencing keeps independent protocol evidence, tunnel data-plane construction, and final live-router integration distinct. A transport blocker may remain visible without forcing all local router construction to stop.

## Current baseline after Plan 115 Q0 completion

```text
M0-M2 foundation                         = closed
M3 NTCP2 local implementation             = present-but-exit-not-met
M3 development interop                    = protocol-defect-localized-at-noise_authenticated
normal daemon NTCP2                       = disabled-and-unenableable
M4 local NetDB/reseed/bootstrap            = substantially-implemented
M4 live lookup/publication                 = blocked-on-live-exploratory-path
M5 exploratory substrate/build control     = substantially-implemented
M5 short-build local outbound              = strict-established
M5 short-build local inbound               = strict-established
M5 canonical production I2NP bridge        = locally-conformant-no-double-prefix
M5 independent short-build evidence        = passed-emissary-q0-construction-and-obep-reply-only
M5 TunnelData forwarding/data plane        = not-yet-closed
M5 mixed-router exploratory pair           = not-yet-proven
M6 destination/garlic/LeaseSet/streaming   = not-authorized-yet
```

## Gate 115 — independent short-build consumption and external delivery

Plan-of-record: [`plans/115-qualified-independent-short-build-consumption-and-external-delivery.md`](115-qualified-independent-short-build-consumption-and-external-delivery.md).

### Required product result

At minimum, one pinned independent router implementation must execute its native short-build processing against the exact production-generated i2pr STBM and return a native acceptance/rejection result.

### Preferred stronger result

The same message is delivered over an authenticated router transport and the independent reply causes i2pr to reach `Established`.

### Gate outcomes

#### 115-A: external Established

```text
Q0 = passed
Q1 = passed
Q2 = passed
Plan 116 = unblocked
live transport dependency = available-for-one-reference-development-lane
```

#### 115-B: independent native consumer passes, transport remains blocked

```text
Q0 = passed (construction + OBEP reply only; Q1/Q2 deferred)
Q1 = blocked/not-exercised
Q2 = blocked/not-exercised
Plan 116 local data plane = unblocked
mixed-router M5 exit = still blocked on qualified live lane
```

This is a valid continuation branch. It prevents the old transport harness from blocking actual tunnel-router development. **The current status is this branch: Q0 has passed for construction + native OBEP reply against pinned Emissary.**

#### 115-C: protocol defect localized

```text
Q0 = reached native reference processor
reference rejection = reproducible
Plan 116 = blocked
next action = one narrow protocol corrective plan
```

Do not continue building a data plane on a tunnel-build format that an independent implementation demonstrably rejects.

#### 115-D: no bounded independent consumer available

Record the exact missing reference boundary. Do not automatically start another generic harness project. Reassess whether a tiny Emissary/i2pd native test seam can be contributed upstream or maintained as a one-shot external fixture.

## Gate 116 — Milestone 5 tunnel data plane and exploratory-pool completion

Plan 116 is **not executable until Plan 115 closes with branch 115-A or 115-B**. Its exact plan file should be generated from the actual Plan 115 implementation surface rather than guessed in advance. The current Q0 status is `passed-construction-and-obep-reply-only` (branch 115-B); the implementation-floor line for Plan 116 must reference this partial Q0 status.

The scope below is the required boundary for that future plan.

### 116 objective

Turn the existing build-control plane into a functioning unidirectional tunnel data plane capable of carrying bounded I2NP messages through local exploratory inbound/outbound tunnels.

This is the point where development returns to router functionality rather than build-message validation.

### 116 mandatory scope

#### A. Tunnel role runtime model

Implement or complete explicit runtime ownership for:

```text
outbound gateway (local creator)
participant
outbound endpoint
inbound gateway
inbound participant
inbound endpoint (local creator)
```

Role behavior must use the existing `TunnelId`, path, derived layer keys, and success-only registration results from the build subsystem. Do not create a parallel tunnel identity model.

#### B. TunnelData encryption/decryption path

Implement the specification-required tunnel layer transformation using the layer/IV keys already derived by the short-build machinery.

Requirements:

- no locally invented cryptographic primitive;
- no secret-bearing `Debug` output;
- strict fixed 1024-byte TunnelData payload handling;
- deterministic known-vector or independently derived fixture coverage where available;
- malformed/incorrect tunnel IDs fail without unbounded work;
- layer processing is separated from runtime forwarding.

#### C. TunnelGateway injection

Use the existing `i2pr-proto::TunnelGatewayMessage`/I2NP model rather than adding a custom gateway envelope.

Required behavior:

```text
local outbound gateway receives an I2NP message
 -> canonical TunnelGateway/tunnel-fragment representation
 -> tunnel-data fragmentation
 -> layered encryption for the outbound path
 -> first-hop delivery action
```

The gateway must have bounded batching/queue behavior. Do not optimize batching before correct single-message forwarding exists.

#### D. Fragmentation and reassembly

Implement the current I2P tunnel-message fragmentation format with explicit bounds for:

- fragment count;
- message size;
- concurrent partial messages;
- per-tunnel partial-state memory;
- duplicate fragments;
- out-of-order fragments;
- expiry/timeouts.

Required tests include:

1. unfragmented small I2NP message;
2. multi-fragment message;
3. out-of-order fragments;
4. duplicate fragment;
5. missing fragment timeout;
6. over-limit message rejection;
7. cross-tunnel fragment isolation.

No wall-clock sleeps in deterministic tests.

#### E. Participant forwarding

For a participant:

```text
receive TunnelData for receive_tunnel
 -> apply one tunnel layer transform
 -> rewrite/associate next_tunnel as required by protocol representation
 -> forward to configured next router
```

The forwarding path must consume immutable registered build metadata. It must not re-query NetDB for routing fields already fixed by the established path.

#### F. Endpoint handling

Outbound endpoint:

- remove final tunnel layer;
- reassemble messages;
- dispatch according to encoded tunnel-delivery instructions;
- initially support only the minimum delivery modes needed for exploratory-router traffic.

Inbound local endpoint:

- remove creator-side accumulated layer processing as required;
- reassemble and hand the resulting I2NP message to the local router-facing boundary.

Do not implement destination/LeaseSet/streaming dispatch in Plan 116.

#### G. Exploratory pool lifecycle

Complete the pool behavior necessary for real use:

- desired inbound/outbound counts;
- pending-build accounting;
- success registration;
- expiration;
- pre-expiry replacement;
- failed-build backoff;
- cancellation cleanup;
- no reuse after terminal failure;
- bounded per-pool state.

Keep peer-selection policy inputs separate from wire codecs.

#### H. Delivery abstraction

Plan 116 should emit transport-neutral router-delivery requests. It must not require NTCP2 specifically.

A deterministic in-memory delivery adapter is sufficient for local Plan 116 acceptance when Plan 115 closed under 115-B (Q0 passed for construction + OBEP reply; Q1/Q2 deferred). The same messages must be compatible with the `EncodedI2npMessage`/`DeliveryRequest` boundary used by a later live lane.

### 116 acceptance criteria

Plan 116 local closure requires all of the following:

1. deterministic outbound exploratory tunnel constructed and registered;
2. deterministic inbound exploratory tunnel constructed and registered;
3. a small I2NP message injected at the outbound gateway emerges correctly at the remote endpoint simulator after all hop transforms;
4. a small I2NP message injected at the remote inbound gateway emerges correctly at the local inbound endpoint;
5. multi-fragment and out-of-order messages reassemble correctly;
6. malformed/expired/unknown-tunnel traffic fails within explicit bounds;
7. tunnel expiry/replacement releases state and keys;
8. transport remains an injected boundary, not hard-coded NTCP2;
9. full workspace CI/boundary checks pass;
10. no claim of live mixed-router interoperability is made unless a real independent lane was actually used.

### 116 closure states

```text
plan_116 = passed-local-tunnel-data-plane
```

is enough to proceed to Plan 117 integration even if Plan 115 Q1 remained blocked (the current state: Q0 passed for construction + OBEP reply only), but Plan 117 cannot fully pass without a qualified live transport lane.

If Plan 115 produced Q2 and the same reference lane remains usable, Plan 116 should include one small live forwarding smoke only after deterministic local data-plane correctness is established. Do not make live execution the development loop for data-plane implementation.

## Gate 117 — real exploratory pair and Milestone 4 live NetDB acceptance

Plan 117 is **not executable until Plan 116 local data-plane closure**.

### 117 objective

Use the smallest qualified external router lane to prove the actual product dependency chain:

```text
reseed/validated RouterInfo
 -> peer/path selection
 -> outbound + inbound exploratory tunnel construction
 -> TunnelGateway/TunnelData forwarding
 -> DatabaseLookup through outbound exploratory tunnel
 -> reply through inbound exploratory tunnel
 -> DatabaseStore/SearchReply processing
 -> local NetDB validation/persistence
```

Then independently verify local RouterInfo publication.

### 117 prerequisite transport rule

Plan 117 requires an actual router-to-router delivery mechanism. The source may be:

1. the Plan 115 authenticated lane if Q1/Q2 passed;
2. a later narrow correction of the same NTCP2 development lane;
3. another transport only if independently implemented and smaller by that time.

Normal-daemon advertisement remains disabled until its own activation checkpoint. A test-only development transport lane is sufficient for Plan 117 evidence if it reaches a real independent router and the status file does not mislabel it as production-qualified.

### 117 minimum topology

Use the smallest topology capable of proving one real exploratory round trip. Do not recreate a broad matrix.

Preferred target:

```text
one i2pr creator
one or more independent-router tunnel hops as required by valid topology
one reachable NetDB target/floodfill in the controlled development topology
```

If a fully isolated multi-router private topology is impossible in the environment, the future Plan 117 must explicitly decide between:

- a bounded ordinary-network development test with no anonymity claim; or
- a controlled reference-router topology available through existing process interfaces.

It must not silently turn absence of namespace isolation into protocol failure.

### 117 acceptance criteria: Milestone 5 mixed-router evidence

1. at least one outbound exploratory tunnel is built through independent-router code;
2. at least one inbound exploratory tunnel is built through independent-router code;
3. the paths reach registered/usable state;
4. TunnelData crosses the independent hop(s);
5. an I2NP payload survives the full outbound and inbound data paths;
6. expiry/cancellation cleanup is observed;
7. evidence is pinned and sanitized.

### 117 acceptance criteria: Milestone 4B live NetDB

1. a real `DatabaseLookup` is sent through the outbound exploratory path;
2. a matching valid response returns through the configured inbound exploratory path;
3. the response passes existing Plan 103 validation;
4. accepted RouterInfo is inserted into the bounded local NetDB;
5. persistence/reload uses the existing Plan 104 path;
6. local RouterInfo publication is sent using the correct live routing path;
7. a separate independent observation confirms the published RouterInfo matches the expected identity/content policy;
8. direct-NTCP2 `DatabaseLookup` substitution remains forbidden.

### 117 failure classification

Separate at least:

```text
transport_unavailable
tunnel_build_rejected
tunnel_reply_unreachable
tunnel_data_forwarding_failed
fragment_reassembly_failed
netdb_query_timeout
netdb_response_invalid
netdb_response_not_routed_to_reply_tunnel
routerinfo_publication_not_observed
```

Do not collapse them into `milestone4_failed`.

## What happens after Plan 117

Only after Plan 116 is locally complete and Plan 117 provides sufficient real exploratory/NetDB evidence should the roadmap authorize Milestone 6 implementation as the dominant line of work:

```text
Destination lifecycle
 -> destination-specific tunnel pools
 -> garlic
 -> LeaseSet creation/publication/lookup
 -> local destination routing
 -> minimal streaming
 -> independent destination interoperability
```

SAM and I2CP should remain downstream of a functioning destination/streaming core rather than being used as substitutes for it.

## Roadmap invariants through Plans 115-117

These remain true unless a later explicit activation plan changes them:

```text
normal_daemon_ntcp2       = disabled-and-unenableable
ntcp2                     = experimental-non-advertised
plan_079                  = deferred-to-pre-normal-activation-checkpoint
public_anonymity_claim    = forbidden
plans_109_to_114_crypto   = retained-unless-independent-defect-is-localized
transport_abstraction     = transport-neutral
runtime_ownership         = i2pr-runtime
netdb_direct_ntcp2_lookup = forbidden
```

## Artifact discipline

To prevent another planning/test artifact buildup:

- Plan 115 gets one plan, one handoff, one status at closure.
- Do not create Plan 116 implementation files until Plan 115's actual result is known.
- Do not create Plan 117 implementation files until Plan 116's actual delivery/data-plane boundary is known.
- Prefer Rust tests/helpers over Python.
- Prefer existing crates/tools over new harness packages.
- Delete temporary reference adapters if they only served one local experiment and are not required to reproduce durable evidence; retain only a small reproducible adapter when it materially protects interoperability.
- Evidence documents store hashes/stages/results, not secret-bearing raw traffic.

The success criterion for this roadmap is forward router capability, not the number of validation artifacts produced.
