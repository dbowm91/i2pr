# Plan 198 status — M6 Java public-client final closure corrective

Status: **`superseded-execution-decomposed-and-closed-via-plans200-204`**.

Plan of record:
[`plans/198-m6-java-public-client-final-closure-corrective.md`](198-m6-java-public-client-final-closure-corrective.md).

This is the superseded original handoff authority for the
Java-public-client closure path; Plan 198 is no longer the
executable handoff and is decomposed into Plans 200–204 by
`plans/199-status.md`. The original `blocked-public-client-
leaseset2-publication` interpretation was retained verbatim so
historical Plan 198 evidence stays recoverable, but the active
follow-on plan is Plan 201 (Branch G corrective framework
landed; final closure blocked on the Plan 200 exact-head
external run that consumes the `P200-*` classification).

## Why Plan 198 was decomposed

Plan 198's attempt to drive independent Java I2P public-client
helpers through `I2PClient` / `I2PSession` and `I2PSocketManager`
public APIs passed the Rust destination/Streaming drivers as
integrated tests but the exact-pinned Java 2.13.0 controlled
router still did not return the public-client LeaseSet2 to the
real i2pr DatabaseLookup path. Mandatory lookup, destination-
delivery, and Streaming rows remained blocked with fail-closed
provenance. Plan 198's final-closure interpretation therefore
could not pass its own §6.3 gate (`26 passed / 22 blocked / 0
failed`) and was decomposed into:

```text
Plan 200 — diagnostic/evidence corrective (helper READY decoupled
           from leaseset=published; bounded REPORT_STATUS; post-
           bootstrap RouterInfo DatabaseLookup proofs in both
           directions; sanitized Java log keys; one terminal
           P200-{A..H} classification per run).
Plan 201 — Branch G corrective framework + the Java second-family
           closure. The branch can be picked only after Plan 200
           records the P200-* classification against the exact-
           pinned Java cache.
Plan 202 — M10 production remote Destination/Streaming composition
           (parallel-executable with Plan 200; not on the Java
           branch).
Plan 203 — M10 positive remote HTTP + IRC application interop.
Plan 204 — final M10 closure documentation and authority
           normalization (closes both milestones on the same exact
           head once Plan 201 + Plan 203 are independently green).
```

## Execution checkpoint (retained)

The Plan 198 public-client helpers and Rust driver path are
implemented and the exact-pinned local lane was exercised
repeatedly. Both public Java helper sessions connect through
the selected I2CP port and the Rust destination and Streaming
tests themselves return `ok`, but the Java 2.13.0 controlled
router does not return the published public-client LeaseSet2
to the real i2pr DatabaseLookup path. The destination lane
therefore stops at `client-ls2-local-but-not-network-visible`
after the bounded lookup deadline, and the Streaming lane stops
before outbound build installation. Publication was tested
both by omitting the option and by explicitly setting
`i2cp.dontPublishLeaseSet=false`; neither changes the result.

Latest retained local evidence:

```text
target/interop/m6-java-evidence-plan198-11
Java public-client tests: 2 passed, 0 failed at the Rust test-process level
mandatory Java rows: blocked at public-client LeaseSet2 publication
```

## Current authority

```text
plan_194 = retained-partial-java-qualification-sam-ls2-publication-boundary
plan_195 = evidence-passed-m10-remote-independent-service-final-closure-pending-plan204-normalization
plan_196 = passed-m6-java-controlled-first-run-topology-corrective
plan_197 = passed-m6-pq-ssu2-option-support-corrective
plan_198 = superseded-execution-decomposed-and-closed-via-plans200-204
plan_199 = superseded-execution-decomposed-and-closed-via-plans200-204
plan_200 = passed-m6-java-public-client-publication-observability-and-verified-bootstrap
plan_201 = in-progress-branch-g-framework-landed-blocked-on-exact-head-external-run
plan_202 = passed-m10-production-remote-destination-and-streaming-composition
plan_203 = passed-m10-positive-remote-http-and-irc-application-interop
plan_204 = in-progress-docs-and-authority-normalization-blocked-on-plan201-external-run

milestone6_i2pd_streaming_interop = passed-via-plan193
milestone6_java_mixed_router_interop = not-yet-passed (plan201 branch-g corrective framework landed; final closure blocked on Plan 200 exact-head external run)
milestone6_interoperable = not-yet-claimed

milestone10_local_product = passed-via-plan180-and-plan182
milestone10_independent_application_clients = evidence-passed-pending-plan204-normalization (Plan 181 local rows passed; Plan 203 promoted the remote application rows to passed-on-env)
milestone10_remote_service_interop = evidence-passed-m10-remote-independent-service-final-closure-pending-plan204-normalization
milestone10_final_acceptance = not-yet-closed

next_executable_plan = 201-branch-g-finalize (Plan 200 exact-head external run consumes the P200 classification)
remaining_sequence = 201-branch-g-finalize-or-pivot-to-branch-{a..f} -> 204-convergence
```

## Handoff rule

Plan 198 is no longer executable. Do not re-execute Plan 198 as
a single program; its requirements now flow through Plans
200–204 and the active Java branch owner is Plan 201.
`plans/195-status.md` is reactivated from
`registered-blocked-by-plan199` to
`evidence-passed-m10-remote-independent-service-final-closure-pending-plan204-normalization`
because Plan 203 supplies the positive application evidence Plan
195 was originally registered to provide.

On the next Plan 201 closure the authority transition becomes:

```text
plan_198 = superseded-execution-decomposed-and-closed-via-plans200-204
plan_199 = superseded-execution-decomposed-and-closed-via-plans200-204
plan_200 = passed-m6-java-public-client-publication-observability-and-verified-bootstrap
plan_201 = passed-m6-java-public-client-publication-corrective-and-second-family-closure
plan_195 = passed-m10-remote-independent-service-final-closure
plan_181 = passed-m10-independent-application-service-interop-final-closure-evidence
milestone6_java_mixed_router_interop = passed-via-plan201
milestone6_interoperable = passed-via-plan193-and-plan201
```

That transition is recorded in `plans/201-status.md` §"On Plan
201 pass (and only then)" and is not claimed from this status.
