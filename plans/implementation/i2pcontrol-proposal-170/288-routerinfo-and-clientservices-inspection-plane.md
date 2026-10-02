# Plan 288 — Proposal 170 RouterInfo and ClientServicesInfo inspection plane

Status: registered-prop170-inspection-plane-blocked-on-plan287

Classification: capability. Read-only; no router mutation.

Hard dependency: Plan 287 closed.

## Objective

Implement truthful bounded Proposal 170 RouterInfo additions and ClientServicesInfo over narrow inspection handles supplied by existing i2pr owners. Establish a source matrix that makes unsupported/missing state explicit and prevents fabricated administrative telemetry.

## Source contract first

Create a machine-readable source matrix with one row per exact Proposal 170 RouterInfo selector. Each row records:
- wire key and return type;
- owner subsystem;
- snapshot method;
- cardinality/encoded-byte ceiling;
- sensitivity/redaction rule;
- freshness semantics;
- available / permitted-neutral / unavailable state;
- fixture/test identifier.

Do not derive availability from whether a serializer exists.

A request containing an unavailable requested field fails explicitly rather than returning a fabricated zero, false, empty list/map, stale persisted intent, or semantically adjacent metric. Empty/zero is valid only after the authoritative owner was queried and reported an actual empty/zero state.

## Narrow inspection handles

Add daemon-facing snapshot capabilities, not global context exposure, for the minimum required domains:

### Router identity / publication

Expose current router id/hash, serialized current local RouterInfo where Proposal 170 requires it, router version/API declaration, uptime, and any current publication metadata from the actual identity/publication owner.

### Transport / network state

Expose bounded active NTCP2/SSU2 peer/session snapshots, configured connection ceilings, reachability/testing/error state and family distinctions from existing runtime owners. No raw mutable session maps.

### NetDB

Expose bounded known/active peer identifiers and serialized RouterInfo views from i2pr-netdb through dedicated read-only snapshots. Banned-peer data must come from an explicit ban owner if one exists; if i2pr has no ban facility, model that fact explicitly before deciding whether the protocol permits an authoritative empty result.

### Tunnel state

Expose exploratory, client/destination, participating/transit, build queue, TBM queue, and success-rate data from the real pool/build/transit owners. Persisted intent is not runtime state. Any rolling-rate selector needs a request-independent bounded metric owner rather than on-demand expensive scans.

### Logs/news/address-book

Do not fabricate these. Address-book selectors remain unavailable until Plan 294. If safe bounded logs or authenticated router news require new owners, record them as residual Plan 295 rows.

## ClientServicesInfo

Implement exact six selectors:
- I2PTunnel: bounded map of configured service names to public addresses/ports, split client/server, derived from the ServiceTunnelManager and control-owned inventory once present;
- HTTPProxy: actual HTTP service enabled/bind state;
- SOCKS: actual SOCKS service enabled/bind state;
- SAM: actual enabled state plus bounded active session information from SAM owner;
- BOB: explicit false because i2pr has no BOB service and the proposal describes Java BOB as deprecated;
- I2CP: actual enabled/bind state.

Never expose private destination material, SAM private destinations, I2CP secret material, remote peer IP addresses unless the Proposal exact field requires them and the privacy review approves the representation.

## Request semantics

Presence/select semantics must match the pinned Proposal/base behavior exactly. Direct Proposal selector form and any base compatibility form must be separated explicitly so overlapping keys cannot select different serializers accidentally.

Bound response collections before allocation/serialization. Apply both row-count and encoded-byte ceilings.

## Evidence

Required tests:
- exact selector inventory has one source row each;
- every available selector is backed by a live/current owner in composition tests;
- unavailable selectors fail whole-request without partial response;
- neutral values are emitted only for explicitly permitted neutral rows;
- empty/zero behavior is distinguished from source unavailable;
- cardinality and encoded-size +1 rejection;
- active peer/session/tunnel state changes appear after deterministic state transitions;
- no secret/high-cardinality diagnostic leakage;
- ClientServicesInfo disabled/enabled transitions for all implemented services;
- BOB is a deliberate constant capability result, not a missing-handler fallback;
- no mutation occurs from any Plan 288 request.

## Acceptance criteria

Plan 288 may close as partial read-only Proposal 170 support if residual selectors are precisely enumerated for Plan 295. It may not call the whole RouterInfo surface complete until every requested selector has a truthful source/permitted-neutral result and final conformance passes.

Closure must publish the source matrix and its exact counts, and must not change M12/mainline behavior.
