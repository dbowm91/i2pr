# Plan 199 status — Milestone 10 unified final closure

Status: **`blocked-java-public-client-leaseset2-and-m10-remote-transport`**.

Plan of record:
[`plans/199-m10-unified-final-closure.md`](199-m10-unified-final-closure.md).

Plan 199 is the focal handoff authority for the remaining Milestone 10 work. It incorporates the still-valid closure requirements from Plan 198 (Java-family M6 final qualification) and Plan 195 (the two remaining remote M10 application rows) into one execution program. Earlier plans retain their evidence/provenance and are not rewritten as passed by registration of this plan.

## Execution result at current head

The Plan 199 Phase A corrective was exercised after `git pull --rebase` at
the current head. The Java lane now starts two disposable stock Java I2P
2.13.0 routers, verifies loopback SSU2 topology, and submits each router's
ordinary RouterInfo to the other over the real i2pr authenticated session.
That bootstrap row passes. The public Java destination and Streaming helpers
also connect through the public client APIs, but the client-owned LeaseSet2
still does not become visible to the other router's real DatabaseLookup path.
The mandatory rows stop fail-closed at
`client-ls2-local-but-not-network-visible`; no M6 Java-family or two-family
closure claim is made.

Phase B cannot honestly be promoted by bookkeeping: the current M10 daemon
manager delivers through the local co-owned peer bridge and has no production
remote-router transport/LeaseSet2 lookup path. The existing remote runner
therefore remains a bounded qualification probe, not a real curl/jaraco
remote application path. Plan 199 remains blocked until both boundaries are
implemented and rerun with command-derived evidence.

## Registration floor

```text
i2pr main = ed81560ce93652576d3b8141b4361490af463254
routine CI = 34808892812 (success)

plan_193 = passed-m6-i2pd-mixed-router-streaming
plan_194 = retained-partial-java-qualification-sam-ls2-publication-boundary
plan_196 = passed-m6-java-controlled-first-run-topology-corrective
plan_197 = passed-m6-pq-ssu2-option-support-corrective
plan_198 = blocked-public-java-client-leaseset2-publication
plan_195 = registered-blocked-by-plan198

milestone6_i2pd_streaming_interop = passed-via-plan193
milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed

milestone10_local_product = passed-via-plan180-and-plan182
milestone10_independent_application_clients = local-rows-passed-plan181-not-closed
milestone10_remote_service_interop = not-yet-passed
milestone10_final_acceptance = not-yet-closed
```

## Remaining sequence under Plan 199

```text
Phase A
  confirm the Java public-client LS2 publication boundary
  -> provide a legitimate network-visible Java floodfill/main-NetDB publication target
  -> complete Java destination + Streaming rows
  -> exact-head two-family M6 all-pass closure

Phase B
  evolve the Plan 181 M10 external lane from blocked-only remote qualification
  -> remote-independent-http-eepsite = command-derived passed
  -> remote-independent-irc-service = command-derived passed

Phase C
  exact-head workspace/static/dependency floor
  -> exact-head M6 external workflow
  -> exact-head M10 service-tunnel external workflow
  -> authority/support/documentation normalization
  -> close Milestone 10
```

The Plan 199 source diagnosis is that Java I2P 2.13.0 places public-client LeaseSets in client-specific segmented NetDBs and normally republishes them to a distinct floodfill; the current single-Java-router private topology has no eligible publication peer and the router excludes itself from floodfill peer selection. Phase A.0 must confirm the corresponding runtime lifecycle with sanitized evidence before the two-router correction is counted.

## Handoff

```text
next_executable_plan = 199 (resolve Java client LeaseSet2 publication and add the real M10 remote transport path)
next_product_layer = m10-unified-final-closure-blocked-at-phase-A-and-phase-B
```

Do not execute Plans 198 and 195 as separate roadmap programs. Their still-valid requirements are incorporated into Plan 199 and remain referenced as detailed provenance.

Plan 199 closes only when its explicit final acceptance criteria are all satisfied, including:

```text
mandatory i2pd M6 rows = all passed
mandatory Java M6 rows = all passed
mandatory M6 blocked/failed/missing rows = 0
Plan 181 retained local M10 rows = all passed
remote-independent-http-eepsite = passed
remote-independent-irc-service = passed
mandatory M10 blocked/failed/missing rows = 0
exact-head routine CI = success
exact-head M6 external workflow = success
exact-head M10 service-tunnel external workflow = success
support/conformance/architecture/status documents = consistent
```

On closure, authority becomes:

```text
plan_198 = passed-m6-java-public-client-complete-second-family-closure
milestone6_interoperable = passed-via-plan193-and-plan198
plan_195 = passed-m10-remote-independent-service-final-closure
plan_199 = passed-m10-unified-final-closure
milestone10_independent_application_clients = passed
milestone10_remote_service_interop = passed-via-plan195-under-plan199
milestone10_final_acceptance = closed-via-plan199
next_executable_plan = none-at-m10-layer
next_product_layer = milestone11-planning
```
