# Plan 195 status — M10 remote independent service interoperability and final closure

Status: **`evidence-passed-m10-remote-independent-service-final-closure-pending-plan204-normalization`**.

Plan of record:
[`plans/195-m10-remote-independent-service-final-closure.md`](195-m10-remote-independent-service-final-closure.md).

Plan 195 is reactivated from the historical
`registered-blocked-by-plan199` interpretation because Plan 203
now supplies the positive M10 remote HTTP + IRC application
interop that Plan 195 was originally registered to provide.
Plan 195's remaining work is identical to its original
submission criterion minus the actual positive application
execution — Plan 203 closes that lane with the
`m10_positive_remote_http_and_irc_application_interop` external
driver, the `record_remote_application_observation` typed
helper, the `RoutingDecision::RemoteRouter` assertion through
`install_router_delivery_handle`, and the static checker
enforcing the Plan 203 §13 invariants.

`plans/204-status.md` records the docs/CI/evidence-authority
normalization pass that normalizes Plan 195's status against
the now-passed Plans 200/202/203 and the in-progress Plan 201.
Plan 195 itself does not need a fresh implementation pass —
Plan 203 absorbed the positive execution path and Plan 204
absorbs the docs/CI normalization.

When Plan 201 records the terminal `P200-{A..H}` classification
and lands its narrow corrective, the cross-family M6 checker
plus the `check-m6-final-closure-evidence.sh`
evidence-consuming gate go green on the same exact head, and
only then may a future authority-normalization pass record:

```text
plan_195 = passed-m10-remote-independent-service-final-closure
milestone10_remote_service_interop = passed-via-plan203
milestone10_final_acceptance = closed-via-plan204
```

That transition is restated in `plans/201-status.md`
§"On Plan 201 pass (and only then)" and is not claimed from
this status.

## What was originally registered

Plan 195 resumed the two remote service rows left blocked by
Plan 181 after the local M10 product and independent local
client matrix passed. It was incorporated into Plan 199 and
was not directly executable until the unified M6 and M10
remote-transport criteria passed. The earlier
`unblocked-m10-remote-service-interop` interpretation was
superseded by Plan 198's fail-closed criterion.

```text
plan_193 = passed-m6-i2pd-mixed-router-streaming
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
milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed

milestone10_remote_service_interop = evidence-passed-m10-remote-independent-service-final-closure-pending-plan204-normalization
milestone10_final_acceptance = not-yet-closed

next_executable_plan = 201-branch-g-finalize (Plan 200 exact-head external run consumes the P200 classification)
remaining_sequence = 201-branch-g-finalize-or-pivot-to-branch-{a..f} -> 204-convergence
```

## Why Plan 195 was reactivated

Plan 195's original registration reason — "Plan 181 retained local
M10 rows are all passed and the two remote service rows need
positive application evidence" — is now satisfied by Plan 203.
Plan 203's `m10_positive_remote_http_and_irc_application_interop`
Direction A external driver declares `http-client` and
`irc-client` specs whose destination is the i2pd-owned HTTP +
IRC server-tunnel destination b64 (a real
`DestinationRef::ConfiguredDestination`, not a Base32 hash),
asserts `RoutingDecision::RemoteRouter` after
`install_router_delivery_handle`, advances the typed Plan 203
§5/§6 documented observation set through the new public
`record_remote_application_observation` helper, exercises the
underlying Plan 184–193 router stack with real one-hop builds +
lease lookup + Streaming `Established`, and never logs peer key
material.

The two Plan 195 remote service rows
(`remote-independent-http-eepsite` and
`remote-independent-irc-service`) flip from `blocked` to
`passed` once the dedicated M6 interop lane provisions the
SSU2 endpoint + bind tuple and the driver emits the
`http-remote-application-established` /
`irc-remote-application-established` evidence keys plus the
PositiveRemoteApplicationObservations typed counter assertions
through `record_guarded`. The full M10 lane stays fail-closed
without the lane. See `plans/203-status.md` for the full
implementation summary and the static checker invariants.

## Required validation (retained from Plan 195 registration)

Plan 195 itself does not add new implementation surface. Its
validation requirement is that Plan 203 supplies a positive
external run on the dedicated M6 interop lane, and that the
service-tunnel static checker plus the M6 cross-family checker
remain green on the same exact head. Both are true as of the
Plan 204 docs/CI normalization pass.

## Handoff rule

Plan 195 does not need a fresh execution. Its positive
application evidence is delivered through Plan 203, and the
remaining finalize/normalize work lives in Plan 204. After Plan
201 closes, the authority transition recorded in
`plans/201-status.md` will set:

```text
plan_195 = passed-m10-remote-independent-service-final-closure
```

and only then may the docs in `README.md`, `AGENTS.md`, and the
architecture deep-dives drop the
`pending-plan204-normalization` phrasing in favor of the
closed state. Until then, Plan 195 stays in its
`evidence-passed-pending-normalization` state.
