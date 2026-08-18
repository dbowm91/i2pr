# Plans 115-117 roadmap: independent short-build -> local data plane -> live exploratory NetDB

## Status

- Date: 2026-08-18.
- Parent roadmap: [`plans/000-mvp-roadmap.md`](000-mvp-roadmap.md).
- Plan 115 independent native short-build Q0: **passed**.
- Plan 116 local TunnelData data plane: **unblocked and next**.
- Plan 117 live exploratory/NetDB integration: **blocked until Plan 116 passes**.

This sequence deliberately separates protocol construction, local router
functionality, and live integration so environment constraints cannot repeatedly
block unrelated implementation work.

## Current baseline

```text
M0-M2 foundation                         = closed
M3 NTCP2 local implementation             = present-but-exit-not-met
M3 development interop                    = protocol-defect-localized-at-noise_authenticated
normal daemon NTCP2                       = disabled-and-unenableable
M4 local NetDB/reseed/bootstrap            = substantially-implemented
M4 live lookup/publication                 = blocked-on-live-exploratory-path
M5 short-build local outbound              = strict-established
M5 short-build local inbound               = strict-established
M5 canonical production I2NP bridge        = locally-conformant-no-double-prefix
M5 independent short-build evidence        = passed-emissary-q0-construction-and-obep-reply-only
M5 TunnelData forwarding/data plane        = next implementation target
M5 mixed-router exploratory pair           = not-yet-proven
M6 destination/garlic/LeaseSet/streaming   = not-authorized-yet
```

---

# Gate 115 — independent short-build evidence

Status: **closed for local progression**.

Authority:

- [`plans/115-status.md`](115-status.md)
- [`plans/115-handoff.md`](115-handoff.md)

Pinned upstream Emissary independently consumed the production-generated i2pr
ShortTunnelBuild and reached native OBEP reply construction.

```text
Q0 independent native consumption = passed
Q1 authenticated transport        = deferred
Q2 live reply -> Established       = deferred
```

This result is sufficient to implement the local data plane. Q1/Q2 are not
requirements for Gate 116.

Do not add another Plan 115 validation pass without new concrete defect
evidence.

---

# Gate 116 — local tunnel data plane

Status: **executable now**.

Plan-of-record:
[`plans/116-local-tunnel-data-plane.md`](116-local-tunnel-data-plane.md)

Handoff:
[`plans/116-handoff.md`](116-handoff.md)

## Objective

Turn the existing short-build control plane into a functioning,
transport-neutral unidirectional tunnel data plane capable of carrying bounded
I2NP messages through deterministic exploratory outbound and inbound paths.

## Required product chain

```text
ShortBuild Established
 -> real established secret/key ownership
 -> real exploratory-pool registration
 -> TunnelData preprocessing
 -> fragmentation/reassembly
 -> AES layer/IV transformations
 -> outbound gateway
 -> participant(s)
 -> outbound endpoint
 -> ROUTER or TUNNEL delivery
 -> inbound gateway
 -> inbound participant(s)
 -> local inbound endpoint
 -> exact reconstructed I2NP message
```

## Important repository prerequisite

The current `ShortBuildRegistrar` is still a placeholder that returns a
fabricated insertion without storing usable tunnel key material. Gate 116 begins
by fixing that ownership boundary. Do not build the data plane on top of fake
pool state.

## Local acceptance

A Plan 116 pass requires:

```text
plan_116 = passed-local-tunnel-data-plane
```

with real established material, canonical TunnelData framing, bounded
fragmentation/reassembly, correct AES transforms, deterministic participant and
endpoint processing, and a complete local outbound-to-inbound tunnel trajectory.

A live independent router is **not** required for Gate 116 acceptance.

## Scope boundary

Gate 116 must not include:

```text
NTCP2 correction/activation
SSU2
Q1/Q2
rootless/VM/container work
public-network testing
another short-build reference probe
Milestone 6 destinations/garlic/LeaseSets/streaming
```

---

# Gate 117 — live exploratory pair and Milestone 4 live NetDB

Status: **not executable until Plan 116 closes**.

## Objective

Integrate the already-working tunnel build + data-plane components with the
smallest available real router-delivery lane and close the product dependency:

```text
reseed / validated RouterInfo
 -> peer/path selection
 -> real outbound + inbound exploratory builds
 -> working TunnelData forwarding
 -> outbound DatabaseLookup
 -> response through configured inbound tunnel
 -> NetDB validation/persistence
 -> local RouterInfo publication
 -> independent publication observation
```

## External-delivery policy

At Gate 117, use the smallest practical lane available at that time. It may be a
test-only development transport and does not need to imply normal-daemon
activation.

If the current environment still prevents authenticated router delivery, record:

```text
plan_117 = blocked-on-live-integration-environment
```

with the exact transport stage. Do not reinterpret that as a TunnelData or
short-build protocol failure without evidence.

Do not reconstruct a broad test matrix. One independent implementation is
sufficient for the first product integration checkpoint unless an ambiguity
requires a second reference.

## Gate 117 Milestone 5 acceptance

At minimum:

1. a real outbound exploratory tunnel is built through independent-router code;
2. a real inbound exploratory tunnel is built through independent-router code;
3. both become usable paths;
4. TunnelData crosses the independent hop(s);
5. one complete I2NP payload survives outbound and inbound processing;
6. expiry/cancellation cleanup is observed;
7. evidence is pinned, sanitized, and accurately classified.

## Gate 117 Milestone 4B acceptance

At minimum:

1. `DatabaseLookup` is sent through the outbound exploratory tunnel;
2. reply routing names the inbound exploratory gateway/tunnel correctly;
3. matching response returns through that inbound tunnel;
4. existing RouterInfo validation accepts the response;
5. accepted RouterInfo enters the bounded NetDB and persistence path;
6. local RouterInfo publication is sent through the correct live routing path;
7. a separate independent observation confirms publication;
8. direct-NTCP2 DatabaseLookup substitution remains forbidden.

---

# After Gate 117

Only after local Plan 116 functionality and sufficient Gate 117 live evidence
should Milestone 6 become the dominant line of work:

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

---

# Anti-loop and artifact rules

1. Environment blockers defer interoperability claims; they do not automatically
   halt transport-neutral implementation.
2. A demonstrated independent protocol rejection is different: localize the
   exact defect and correct that defect only.
3. Prefer production Rust state-machine/data-plane tests over new orchestration.
4. Do not rebuild the historical Python/NTCP2 harness to answer TunnelData
   implementation questions.
5. Plan 116 must produce router functionality rather than external evidence.
6. Gate 117 is the next place where real external delivery becomes mandatory.
7. Keep evidence documents small; retain hashes/stages/results, not raw secret
   traffic.
8. Do not create a new plan between successful Plan 116 closure and Gate 117
   merely to revalidate short-build construction.

## Current authority

```text
plan_115                         = passed-emissary-q0-construction-and-obep-reply-only
Q0_native_emissary               = passed
Q1_authenticated_transport       = deferred
Q2_external_return_established   = deferred
plan_116_local_data_plane        = unblocked-and-next
plan_117_live_integration        = blocked-until-plan116-passes
normal_daemon_ntcp2              = disabled-and-unenableable
ntcp2                             = experimental-non-advertised
```
