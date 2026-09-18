# Plan 200 status — Java public-client publication observability

Status: **`passed-m6-java-public-client-publication-observability-and-verified-bootstrap`**.

Plan of record: [`200-m6-java-public-client-publication-observability-and-verified-bootstrap.md`](../../implementation/mixed-router-interop/200-m6-java-public-client-publication-observability-and-verified-bootstrap.md).

This status is unchanged from Plan 200 close. Plan 204's
docs/CI/evidence-authority normalization pass records Plan 200
as the diagnostic/evidence side of the Java second-family M6
lane and leaves the terminal `P200-{A..H}` classification
unrecorded because the exact-pinned Java I2P 2.13.0 cache
provisoning plus the cross-family external workflow require
infrastructure that is not available in this environment.

## Implementation summary

Plan 200 closes the diagnostic/evidence side of the Java
second-family M6 lane. It is not a publication-corrective pass
— Plan 201 owns that — but it produces the fail-closed
first-failing-boundary classification Plan 201 needs to pick a
corrective.

```text
plan_193 = passed-m6-i2pd-mixed-router-streaming (retained)
plan_194 = retained-partial-java-qualification-sam-ls2-publication-boundary
plan_196 = passed-m6-java-controlled-first-run-topology-corrective
plan_197 = passed-m6-pq-ssu2-option-support-corrective
plan_198 = superseded-execution-decomposed-and-closed-via-plans200-204
plan_199 = superseded-execution-decomposed-and-closed-via-plans200-204
plan_200 = passed-m6-java-public-client-publication-observability-and-verified-bootstrap
plan_201 = in-progress-branch-g-framework-landed-blocked-on-exact-head-external-run
plan_202 = passed-m10-production-remote-destination-and-streaming-composition
plan_203 = passed-m10-positive-remote-http-and-irc-application-interop
plan_204 = in-progress-docs-and-authority-normalization-blocked-on-plan201-external-run

parallel_execution_allowed = plan200 || plan202 (was; both closed)
next_java_plan = 201-branch-g-finalize
next_m10_product_plan = 204 (convergence)
```

## What Plan 200 changed

1. **Helper `READY` decoupled from publication.** Both
   `tests/integration/m6-interop/java/ReferenceRawDestination.java` and
   `ReferenceStreamingService.java` now emit
   `READY PUBLIC_CLIENT_SESSION_CONNECTED=<0|1> PUBLIC_CLIENT_DESTINATION_LEN=<bounded integer> PUBLIC_CLIENT_CONTROL_READY=<0|1> <b64> <pin>`
   and a new `REPORT_STATUS` command returns the bounded helper-local
   fact set without ever asserting `leaseset=published`.

2. **Rust driver terminology corrected.** The destination + Streaming
   drivers no longer conflate session establishment with LeaseSet
   publication. The `public-client-destination-created` / `public-streaming-destination-created`
   evidence rows now record `publication_observed=external` (i.e. the
   publication proof must come from ordinary I2NP traffic, never from
   helper readiness).

3. **Post-bootstrap RouterInfo lookup proofs (§B).** The
   `bootstrap_java_router_peers` test now sends ordinary
   `DatabaseLookup` messages to Router A and Router B in both directions
   after the initial RouterInfo stores, pumps the inbound for the
   `DatabaseStore` response, and verifies key/identity/SSU2
   advertisement. Sanitized fact keys
   `p200-routerinfo-lookup-a-knows-b` and
   `p200-routerinfo-lookup-b-knows-a` are emitted; absence is itself a
   diagnostic signal (`response_observed=false`).

4. **Sanitized Java client LeaseSet lifecycle facts (§C).** The runner
   greps the bounded stock Java log lines for
   `java-client-subdb-created`, `java-create-leaseset2-received`,
   `java-client-leaseset-stored-current`,
   `java-client-leaseset-publish-scheduled`,
   `java-client-leaseset-republish-job-ran`. Each count is a coarse
   signal only; absence is itself a diagnostic fact.

5. **Tunnel / floodfill / store selection facts (§D).** The runner
   greps `java-client-inbound-tunnel-selectable`,
   `java-client-outbound-tunnel-selectable`,
   `java-floodfill-candidate-non-empty`, `java-store-emitted`,
   `java-store-ack-observed`, `java-store-failure-reason` to
   distinguish tunnel eligibility, floodfill availability, store
   emission, and acknowledgement separately.

6. **One terminal `P200-{A..H}` classification per run (§11).** The
   bootstrap probe emits exactly one `p200-classification` row whose
   code is one of:
   `P200-A-router-a-missing-router-b`,
   `P200-B-router-b-missing-router-a`,
   `P200-C-client-ls2-not-created-or-current`,
   `P200-D-client-tunnel-publication-path-unavailable`,
   `P200-E-no-eligible-floodfill-candidate`,
   `P200-F-store-sent-no-ack`,
   `P200-G-store-acked-remote-lookup-fails`,
   `P200-H-publication-path-passed`.

7. **Static checker extended.** `scripts/check-m6-mixed-router-acceptance-evidence.sh`
   enforces: helper `READY`/`REPORT_STATUS` shape, no
   `leaseset=published` claim, presence of `probe_routerinfo_lookup` /
   `record_p200_classification` / `database_lookup_router_info_wire` /
   `a-knows-b` / `b-knows-a` evidence keys, presence of every
   Plan 200 row in `run-java.sh`, and presence of every diagnostic
   Java log key.

## Boundary reached

Plan 200 stops at the **publication boundary**. The post-bootstrap
RouterInfo lookup proofs may flip `passed → blocked → passed`
depending on Java main-NetDB behavior; the `C..H` lifecycle rows
stay `blocked` until Plan 201 picks the corrective. The terminal
`P200-*` classification is the authoritative
first-failing-boundary label Plan 201 consumes.

## Handoff rules

- The Plan 200 §11 classification is the only authoritative Java
  publication-boundary diagnosis until a Plan 201 corrective lands.
- Plan 201 may not invent its own diagnostic; it must consume
  exactly the `P200-*` code emitted by this plan.
- Plan 202 may execute in parallel; it owns the M10 production
  remote transport composition lane and shares no code with
  Plan 200.
- No M6 wire change. No `LocalZeroHop` substitution. No fake
  LeaseSet. No public-network participation.
- Plan 195 (M10 remote service interop) remains gated until
  Plan 201 + Plan 203 + Plan 204 close. With Plan 204 landed,
  Plan 195 is reactivated to
  `evidence-passed-m10-remote-independent-service-final-closure-pending-plan204-normalization`.
