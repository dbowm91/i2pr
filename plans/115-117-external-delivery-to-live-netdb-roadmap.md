# Plans 115-117 roadmap: independent short-build -> local data plane -> exploratory NetDB composition

## Status

- Date: 2026-08-18.
- Parent roadmap: [`plans/000-mvp-roadmap.md`](000-mvp-roadmap.md).
- Plan 115 independent native short-build Q0: **passed**.
- Plan 116 local TunnelData data plane: **passed-final-local-closure**.
- Plan 117 exploratory/NetDB composition: **ready for execution**.
- Plan 117 plan of record: [`117-live-exploratory-netdb-integration.md`](117-live-exploratory-netdb-integration.md).
- Plan 117 handoff: [`117-handoff.md`](117-handoff.md).

The purpose of this sequence is to separate protocol construction, local router functionality, independent native evidence, and authenticated external transport so environment limitations cannot repeatedly block unrelated implementation work.

## Current authority

```text
plan_115                              = passed-emissary-q0-construction-and-obep-reply-only
Q0_native_emissary                    = passed
plan_116                              = passed-final-local-closure
plan_117                              = ready-for-execution
plan_117_local_composition            = pending
plan_117_native_reference             = pending
plan_117_authenticated_transport      = deferred-until-attempted
Q1_authenticated_transport            = deferred
Q2_external_return_established        = deferred
normal_daemon_ntcp2                   = disabled-and-unenableable
ntcp2                                 = experimental-non-advertised
router_construction                   = active
```

---

# Gate 115 — independent short-build evidence

Status: **closed for progression**.

Authority:

- [`115-status.md`](115-status.md)
- [`115-handoff.md`](115-handoff.md)
- [`115-completion-emissary-native-q0.md`](115-completion-emissary-native-q0.md)

Pinned upstream Emissary independently consumed the production-generated i2pr ShortTunnelBuild and reached native OBEP reply construction.

```text
Q0 independent native consumption = passed
Q1 authenticated transport        = deferred
Q2 live reply -> Established       = deferred
```

Do not add another Plan 115 validation pass without new concrete protocol-defect evidence.

---

# Gate 116 — local tunnel data plane

Status: **closed**.

Authority:

- [`116-status.md`](116-status.md)
- [`116-handoff.md`](116-handoff.md)
- [`116-local-tunnel-data-plane.md`](116-local-tunnel-data-plane.md)
- [`116-completion-correction.md`](116-completion-correction.md)
- [`116-final-closure.md`](116-final-closure.md)
- [`116-terminal-cleanup.md`](116-terminal-cleanup.md)

Plan 116 now provides the runtime-neutral local tunnel data plane:

```text
ShortBuild Established
 -> real established secret/key ownership
 -> real exploratory-pool registration
 -> one-shot EstablishedMaterial transfer
 -> TunnelData preprocessing
 -> bounded fragmentation/reassembly
 -> AES tunnel layer/IV transformations
 -> outbound gateway
 -> participant(s)
 -> outbound endpoint
 -> ROUTER or TUNNEL delivery
 -> inbound gateway
 -> inbound participant(s)
 -> local inbound endpoint
 -> exact reconstructed I2NP message
```

The terminal closure includes exact-byte ordered and out-of-order outbound-to-inbound trajectories, bounded duplicate accounting, and delivery-metadata integrity.

Do not reopen Plan 116 for environment validation.

A non-blocking hardening item remains available for future opportunistic cleanup: reject a new fragment sequence greater than an already-declared terminal `is_last=true` sequence. This does not block Plan 117.

---

# Gate 117 — exploratory tunnel / NetDB composition

Status: **execute now**.

Plan of record:
[`117-live-exploratory-netdb-integration.md`](117-live-exploratory-netdb-integration.md)

Handoff:
[`117-handoff.md`](117-handoff.md)

## Objective

Join the already-built short-build, exploratory-pool, TunnelData, NetDB, publication, and transport-neutral ownership surfaces into an actual router product path:

```text
validated/reseeded RouterInfo store
 -> exploratory build/path scheduling
 -> real inbound + outbound established material
 -> activated local tunnel roles
 -> actual DatabaseLookup body
 -> outbound exploratory TunnelData
 -> independent floodfill handling
 -> reply through the configured inbound exploratory path
 -> local inbound endpoint
 -> RouterInfo validation/store
 -> local RouterInfo publication
 -> independent publication observation
```

Direct `DatabaseLookup` over NTCP2 remains forbidden as a substitute for this path.

## Evidence decomposition

Plan 117 explicitly distinguishes:

```text
117-L  local production composition
117-N  independent native reference composition
117-X  authenticated transport / live-process delivery
```

This is the anti-loop mechanism for the current environment.

### 117-L — mandatory local production composition

Must pass using i2pr production components.

Key required corrections already identified:

```text
LookupAction::SendDatabaselookup currently discards the actual DatabaseLookup body
PublicationAttemptRecord currently discards the actual DatabaseStore body
NetDbSeam::pending_action_after_path remains a placeholder
EstablishedMaterial has no composition-time activated-role owner
Daemon currently has no i2pr-tunnel composition dependency
```

The local pass must correct these seams before any reference or transport probe.

### 117-N — mandatory pinned native reference

Use one independent implementation only:

```text
upstream Emissary
revision 9b43484a21d5a1291c4881cdae62a36c527f8c0f
emissary-core 0.4.0
```

The pinned Emissary production implementation supports:

```text
short-build IBGW / Participant / OBEP roles
OBEP TunnelData processing
Tunnel Message parsing/reassembly
ROUTER/TUNNEL delivery
floodfill DatabaseLookup handling
DatabaseStore / DatabaseSearchReply generation
IBGW/participant TunnelData forwarding
```

The reference checkpoint should therefore prove the mixed-router path:

```text
i2pr outbound creator
 -> Emissary OBEP
 -> Emissary floodfill DatabaseLookup handler
 -> Emissary inbound gateway/participant
 -> i2pr local inbound endpoint
 -> i2pr RouterInfo validation/store
```

and independently accept i2pr RouterInfo publication.

The temporary Emissary patch must be test-only, small, deleted after use, and its final SHA-256 must be recorded **before** deletion.

The native lane does not prove Q1 or Q2. A test-only decapsulation seam for the garlic-wrapped OTBRM is acceptable if explicitly recorded as a Q2 bypass.

### 117-X — authenticated transport checkpoint

After 117-L and 117-N pass, inspect the existing qualified host lanes.

If an already-existing authenticated execution lane is runnable:

```text
run one bounded authenticated delivery probe
 -> record exact Q1/Q2 stage
```

If the current host still cannot provide the lane:

```text
117-X = deferred-host-lane-unavailable
```

Then stop external work.

Do not rebuild:

```text
rootless namespace harnesses
Docker / Multipass / VM orchestration
Python interop infrastructure
broad reference matrices
```

An environment failure is not evidence of a protocol defect.

---

# Gate 117 product order

Execute in this order:

```text
A  make lookup/publication actions own their actual typed I2NP bodies
B  add one-shot pool material activation + bounded local role registry
C  replace the NetDbSeam post-reply-path placeholder
D  route DatabaseLookup through outbound exploratory TunnelData
E  route inbound TunnelData to DatabaseStore / DatabaseSearchReply ingestion
F  route local RouterInfo publication through outbound exploratory TunnelData
G  pass one terminal all-i2pr production-seam lookup trajectory
H  pass the pinned Emissary native mixed-router lookup/publication checkpoint
I  attempt authenticated transport only if the existing host lane is runnable
J  update status / architecture / roadmap authority
```

Do not start the Emissary checkpoint first. It should validate working product composition, not become the implementation architecture.

---

# Gate 117 security invariants

1. DatabaseLookup must use an outbound exploratory tunnel.
2. The lookup reply gateway/tunnel must name the real inbound exploratory gateway receive endpoint.
3. A direct authenticated link to a floodfill is not a substitute for the exploratory path.
4. Inbound TunnelData is dispatched by the local creator endpoint receive tunnel ID.
5. Unknown TunnelData IDs fail closed.
6. Layer keys remain one-shot/single-owner secret state; do not persistently clone them across pool/runtime owners.
7. RouterInfo responses use the existing bounded decompression, validation, store, and persistence rules.
8. Publication queue acceptance is not equivalent to independent publication observation.
9. `ReplyEncryption::None` is limited to this first RouterInfo-only integration checkpoint; it is not blanket policy for future LeaseSet/client lookups.
10. New registries/queues inherit existing pool and lookup capacity bounds.
11. Normal-daemon NTCP2 remains disabled unless separately authorized later.
12. Raw tunnel plaintext, ciphertext, and secret keys must not enter status/evidence logs.

---

# Gate 117 completion states

## Local + native pass, authenticated host lane unavailable

This is a valid Plan 117 product-construction outcome:

```text
plan_117_local_composition            = passed
plan_117_native_reference             = passed-emissary-mixed-router-netdb
plan_117_authenticated_transport      = deferred-host-lane-unavailable
plan_117                              = local-native-complete-external-deferred
milestone4b_authenticated_external    = blocked
router_construction                   = may-continue
normal_daemon_ntcp2                   = disabled-and-unenableable
ntcp2                                 = experimental-non-advertised
```

This permits the roadmap to proceed into subsequent transport-neutral/local router construction while retaining an honest external-evidence gap.

## Local + native + authenticated transport pass

```text
plan_117_local_composition            = passed
plan_117_native_reference             = passed-emissary-mixed-router-netdb
plan_117_authenticated_transport      = passed
plan_117                              = passed-qualified-exploratory-netdb-integration
milestone4b_authenticated_external    = eligible-for-closure-review
```

Do not infer normal-daemon production transport support merely from the test lane.

## Native reference exposes a protocol defect

Record the exact highest stage, for example:

```text
failed-emissary-obep-tunneldata
failed-emissary-database-lookup-parse
failed-emissary-floodfill-response
failed-emissary-inbound-return
failed-i2pr-reference-routerinfo-validation
```

Localize that one defect and correct it once. Do not create another broad validation branch.

---

# After Gate 117

Once 117-L and 117-N are green, the next local router-construction line may move into the destination layer even if 117-X is deferred by the current host:

```text
Destination lifecycle
 -> destination tunnel pools
 -> garlic
 -> LeaseSet creation/publication/lookup
 -> local destination routing
 -> minimal streaming
 -> independent destination interoperability
```

SAM and I2CP remain downstream of a functioning destination/streaming core.

The authenticated external transport gap remains tracked independently until a suitable execution lane exists.

---

# Anti-loop and artifact rules

1. Environment blockers defer interoperability claims; they do not erase successful local/native product construction.
2. An affirmative independent protocol rejection is different: localize and correct that specific defect.
3. Prefer production Rust composition tests over orchestration.
4. Do not rebuild the historical Python/NTCP2 harness for Plan 117.
5. Use one pinned independent implementation unless a concrete ambiguity requires otherwise.
6. Keep reference patches temporary and record their digest before deletion.
7. Keep evidence small and sanitized; retain stages, types, lengths, hashes, and outcomes rather than raw traffic/secrets.
8. Do not create another Plan 116 validation pass.
9. Do not activate normal-daemon NTCP2 merely to make Gate 117 convenient.
10. Progress should now be measured by router functionality first, external transport evidence second.
