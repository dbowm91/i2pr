# Plan 199 status — Milestone 10 unified final closure

Status: **`superseded-execution-decomposed-and-closed-via-plans200-204`**.

Plan of record:
[`plans/closure/service-tunnels/199-m10-unified-final-closure.md`](199-m10-unified-final-closure.md).

Plan 199 remains the historical umbrella for the final M6/M10
requirements, but it is no longer the executable handoff. Its
attempted implementation at commit
`71942008d40f58507181f286449f979c92ee7c90` exposed two
independent blockers that were split into Plans 200–204 by
`plans/closure/service-tunnels/204-status.md`. The decomposed graph closes on the same
exact head once Plan 201 + Plan 203 are independently green.

## Retained execution result

The Plan 199 attempt established two disposable exact-pinned
Java I2P 2.13.0 routers, authenticated SSU2 sessions, ordinary
RouterInfo DatabaseStore submission in both directions, and
real i2pr one-hop tunnel construction. The public Java helpers
connected through public I2CP/Streaming APIs. A real tunneled
DatabaseLookup for the helper Destination still received no
Standard LeaseSet2 within the bounded window.

That run did **not** prove the intermediate publication
boundaries strongly enough to select a correct fix. In
particular, `RouterDeliveryOutcome::Accepted` proved i2pr
admitted the RouterInfo send, not that the Java main NetDB
installed and could serve the peer; helper `READY` after
`session.connect()` proved session establishment, not
network-visible LeaseSet2 publication.

The same pass also proved the M10 production manager still
routes queued Streaming requests through the Plan 182
local/co-owned bridge and has no production remote-router
LeaseSet2/tunnel/transport composition. The retained negative
remote qualification therefore reports `unknown_peer > 0`,
`delivered = 0`, and no establishment by design.

No M6 Java-family or M10 remote-service closure claim is made
from Plan 199.

## Decomposed executable graph (closed-by-plans-200-204)

```text
Java / M6 branch:
  Plan 200 — publication observability + verified A/B main-NetDB bootstrap
      -> Plan 201 — smallest evidence-selected corrective + Java second-family closure

M10 product branch:
  Plan 202 — production remote Destination/LeaseSet2/tunnel/Streaming composition
      -> Plan 203 — positive remote HTTP + IRC application interop

Convergence:
  Plan 201 + Plan 203
      -> Plan 204 — exact-head final evidence + authority/documentation normalization
```

Plans 200 and 202 are intentionally independent and executed in
parallel. Plans 200, 202, and 203 are passed. Plan 201 is
in-progress with its Branch G corrective framework landed.
Plan 204 landed the docs/CI/evidence-authority normalization
pass on top of the three passed plans and remains blocked on
Plan 201's exact-head external run.

## Current authority

```text
plan_193 = passed-m6-i2pd-mixed-router-streaming
plan_194 = retained-partial-java-qualification-sam-ls2-publication-boundary
plan_196 = passed-m6-java-controlled-first-run-topology-corrective
plan_197 = passed-m6-pq-ssu2-option-support-corrective
plan_195 = evidence-passed-m10-remote-independent-service-final-closure-pending-plan204-normalization
plan_198 = superseded-execution-decomposed-and-closed-via-plans200-204
plan_199 = superseded-execution-decomposed-and-closed-via-plans200-204
plan_200 = passed-m6-java-public-client-publication-observability-and-verified-bootstrap
plan_201 = in-progress-branch-g-framework-landed-blocked-on-exact-head-external-run
plan_202 = passed-m10-production-remote-destination-and-streaming-composition
plan_203 = passed-m10-positive-remote-http-and-irc-application-interop
plan_204 = in-progress-docs-and-authority-normalization-blocked-on-plan201-external-run

milestone6_i2pd_streaming_interop = passed-via-plan193
milestone6_java_mixed_router_interop = not-yet-passed (Plan 201 Branch-G framework landed; final closure blocked on Plan 200 exact-head external run)
milestone6_interoperable = not-yet-claimed

milestone10_local_product = passed-via-plan180-and-plan182
milestone10_independent_application_clients = evidence-passed-pending-plan204-normalization (Plan 181 local rows passed; Plan 203 promotes remote rows to passed-on-env)
milestone10_remote_service_interop = evidence-passed-m10-remote-independent-service-final-closure-pending-plan204-normalization
milestone10_final_acceptance = not-yet-closed

next_executable_plan = 201-branch-g-finalize (Plan 200 exact-head external run consumes the P200 classification)
```

## Handoff rules

Do not execute Plan 199 again as one monolithic program. Do not
independently restart Plan 195 as `registered-blocked-by-plan199`
— that status was reactivated by Plan 204 because Plan 203
supplies the positive application evidence Plan 195 was
originally registered to provide. Do not independently restart
Plan 198 — it is now `superseded-execution-decomposed-and-
closed-via-plans200-204`.

Plan 200 must diagnose the first real Java publication boundary
before Plan 201 chooses a corrective. Plan 202 must close a
positive generic remote service-tunnel transport path before
Plan 203 attempts counted curl/jaraco application rows. Plan
204 may close the milestone only when both branches have
independently passed.

The final acceptance requirements remain unchanged from
Plan 199 and are restated in `plans/closure/service-tunnels/204-status.md`:

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

Plan 204 landed (1) through (6) plus the docs/CI/authority
normalization pass on top of the passed Plans 200/202/203.
Items (7) and (8) — the exact-head M6 external workflow
producing the Java public-client ledger and the Plan 201
final-branch implementation — are still pending Plan 200's
exact-head external run plus Plan 201's narrow corrective.
