# Plan 204 status — final M6/M10 evidence and authority convergence

Status: **`blocked-on-m6-java-second-family-plan218-for-final-convergence`**.

## 2026-09-18 dependency amendment

M10 product/application closure is already authoritative through Plans 213, 214
and the hosted Plan 215 re-verification. Plan 204 is now a convergence-only
normalization pass and MUST NOT reopen or downgrade those M10 results.

The remaining hard dependency is independent M6 Java second-family closure:

```text
Plan 217 — Java closure harness/evidence corrective
    -> Plan 218 — direct-I2CP Java final qualification
    -> Plan 204 — cross-milestone authority/docs convergence
```

Plan 205 is retained as a conditional fallback only if Plan 218, after Plan 217
closes, proves a genuine stock-Java direct-I2CP public-client boundary.

Current authority:

```text
plan_213 = passed-m10-router-backed-generic-external-qualification
plan_214 = passed-m10-product-only-remote-http-and-irc-application-closure
plan_215 = passed-m10-hosted-plan214-tunnel-config-generation-corrective-and-exact-head-reverification
milestone10_final_acceptance = closed

plan_217 = registered-ready-m6-java-closure-harness-corrective
plan_218 = registered-blocked-on-plan217-m6-java-final-qualification
milestone6_java_mixed_router_interop = not-yet-passed

plan_204 = blocked-on-m6-java-second-family-plan218-for-final-convergence
```

The prior Plan 204 narrative is retained below for traceability; its older
references to Plans 213/214 as unfinished and Plan 205 as the active Java branch
are superseded by this amendment.

## Retained prior status narrative

Plan of record: [`204-m10-final-closure-evidence-authority-and-documentation-normalization.md`](204-m10-final-closure-evidence-authority-and-documentation-normalization.md).

Plan 204 remains a later cross-milestone convergence/normalization pass. It must not hide product/evidence defects, and it must not force the independently testable M10 product to remain open solely because the Java M6 second-family branch is unresolved.

## Current execution graph

```text
Java / M6 branch:
  independent Plan 205 / evidence-driven successor
    -> M6 Java second-family closure

M10 branch:
  Plan 212 source closure (retained)
    -> router-backed per-service Destination state
    -> real per-service outbound/inbound tunnel material
    -> real local LS2 from real inbound lease metadata
    -> production receive-id ownership
    -> per-service target LS2 lookup
    -> server LS2 publication
    -> Garlic -> pop_payload -> canonical Streaming receive

  Plan 213 qualification
    -> replace immutable-false generic A/B scaffold with real application I/O
    -> independent exact-pinned i2pd SAM STREAM service + initiator
    -> command/state-derived evidence only
    -> generic Direction A + Direction B hosted exact-head proof twice

  Plan 214 final application qualification
    -> harden retained Plan 211 HTTP/IRC evidence path
    -> system curl HTTP row with target fixture + production counters
    -> exact-pinned jaraco IRC row with target fixture/privacy/DCC + counters
    -> hosted full-lane repeatability twice
    -> M10 closure

parallel outcomes:
  Plan 214 may close Milestone 10 independently
  Java branch may close Milestone 6 independently

later convergence:
  closed Java M6 branch + already-closed M10 -> Plan 204 authority/docs convergence
```

## Why Plans 213/214 were required after Plan 212 source closure

Plan 212 fixed the product seam, but its external generic driver is not yet executable proof: Direction A/B success booleans are immutable `false`, no counted application byte exchange occurs, the default M10 runner skips generic destination provisioning, and several mandatory qualification facts are unconditional literals.

The retained Plan 211 application driver also needs final evidence hardening: it directly constructs a placeholder manager, stamps pin/privacy facts, uses insufficient target-side/DCC derivation, and does not guarantee continuous `ServiceProduct::poll_inbound()` progress while blocking external clients execute.

Those are qualification defects. They do not invalidate the retained Plan 212 source architecture, but they prevent M10 closure until corrected and executed.

## Current authority

```text
plan_200 = retained diagnostic/evidence pass
plan_201 = retained/in-progress Java corrective history
plan_204 = blocked-on-independent-java-m6-branch-and-m10-plan213-plan214-qualification
plan_205 = registered/in-progress Java second-family branch

plan_202 = retained-partial-routing-capability-surface
plan_203 = retained-partial-application-observation-scaffolding
plan_206 = retained-partial-executable-backend-seams
plan_207 = retained-partial-real-application-client-harness
plan_208 = retained-partial-production-call-graph-corrective
plan_209 = retained-partial-black-box-composition-harness
plan_210 = retained-partial-structural-corrective-superseded-by-plan212
plan_211 = retained-source-harness-superseded-for-final-evidence-by-plan214
plan_212 = source-closure-landed-qualification-owned-by-plan213
plan_213 = registered-executable-after-plan212-source-closure
plan_214 = registered-blocked-by-plan213

milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed
m10_remote_transport_core = not-yet-passed
m10_generic_remote_product = not-yet-passed
m10_remote_application_interop = not-yet-passed
milestone10_remote_service_interop = not-yet-passed
milestone10_final_acceptance = not-yet-closed
```

The retained M10 local matrix stays passed. Remote rows remain non-authoritative until Plan 213 and Plan 214 pass.

## M10 closure authority

After Plan 213 passes:

```text
plan_212 = passed-source-and-generic-external-qualification-via-plan213
plan_213 = passed-m10-router-backed-generic-external-qualification
m10_remote_transport_core = passed-via-plan212-and-plan213
m10_generic_remote_product = passed-via-plan213
plan_214 = executable
```

After Plan 214 then passes its two consecutive hosted full-lane runs on one exact source SHA:

```text
plan_214 = passed-m10-http-irc-product-only-external-requalification-and-final-closure
m10_remote_application_interop = passed-via-plan214
milestone10_remote_service_interop = passed-via-plan213-and-plan214
milestone10_final_acceptance = closed-via-plan214
next_product_layer = milestone11-planning
```

This M10 transition does **not** require Java M6 second-family closure.

Plan 204 later consumes independently closed M10 authority plus independently closed Java M6 authority and normalizes cross-milestone documentation. It must not rerun or downgrade valid M10 closure merely because Java closure lands later.
