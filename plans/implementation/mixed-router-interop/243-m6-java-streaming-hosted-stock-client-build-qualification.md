# Plan 243 — M6 Java Streaming hosted stock-client-build qualification and lookup continuation

Status: **registered-ready-m6-java-streaming-hosted-stock-client-build-qualification**

## 1. Objective

Execute the already-corrected Plan-242 Java Streaming lane on a host that actually has the exact Java reference artifacts and an i2pr daemon binary, collect three counted same-SHA attempts with no tuning, and classify the first live boundary.

Plan 242 is a harness/semantics corrective. Its source lock, corrected bootstrap gate, corrected non-zero-hop pair gate, bounded path observer, driver, unit tests, static checks, and full workspace floor are green. Its live external lane was not executed on the closure host because that host lacked the required Java cache plus i2pr daemon pair.

Plan 243 supplies that missing execution authority. It must not add another selector, bootstrap, lookup, or protocol workaround before the frozen Plan-242 lane is actually run.

## 2. Retained authority

Retain unchanged:

- Java I2P 2.13.0 at 9134f808337b401e8e53c73734c81fab04280c9d;
- i2pd 2.61.0 at 635b013a612ff47278ef02acf8580a28e10e26c5;
- controlled A/B/C Java topology;
- Plan-230 reachability/capability fixture corrections;
- Plan-232 route-derived lease-gateway correction and raw-Destination bidirectional authority;
- Plans 237–240 response/admission/lookup attribution;
- Plan-241 one-hop Streaming helper profile;
- Plan-242 source-accurate selector semantics, stock-candidate bootstrap gate, non-zero-hop client-pair gate, and bounded path-role observation;
- Plan-240 exact Streaming target-job correlation;
- Plan-225 ordered lookup-return chain;
- Plan-239 OCMOSJ continuation;
- Plan-231/232 downstream tunnel/i2pr observations;
- all 45-second bounded response windows.

No retained historical result is reinterpreted as new live evidence.

## 3. Plan-242 implementation authority

Plan-242 implementation-and-closure commit on main:

9b0e61c6c3f780a19e843e63d25804fb8756616a

Plan 243 must execute from an implementation SHA containing the Plan-242 harness unchanged except for minimal Plan-243 host-preflight/evidence surfaces if required.

If execution exposes a parser/probe/harness defect that prevents faithful observation, fix only that defect on a new SHA and restart the three-attempt budget. Do not tune Java routing or tunnel behavior.

## 4. Host qualification gate

Before any counted attempt, prove the execution host has all required artifacts and commands. Record sanitized booleans/versions for:

- repository worktree at the intended implementation SHA;
- cargo/Rust toolchain capable of building the workspace;
- built i2pr-daemon binary at the expected path, or a deterministic build step that produces it;
- Java runtime compatible with exact Java I2P 2.13.0 jars;
- javac;
- exact Java reference jar/cache set required by tests/integration/m6-interop/run-java.sh;
- required shell/Python helpers;
- loopback ports required by controlled A/B/C topology available before startup;
- filesystem permissions for fresh disposable RouterContexts and evidence directories;
- exact-pinned Java source-lock checker runnable against the staged/pinned source input.

If any required artifact is missing, emit:

P243-H-HOST-NOT-QUALIFIED reason=<bounded-reason>

and do not consume a counted interop attempt.

Bounded reasons: java-runtime-missing, javac-missing, java-reference-cache-missing, i2pr-daemon-missing, source-lock-input-missing, port-preflight-failed, workspace-sha-mismatch, filesystem-preflight-failed.

A host-preflight failure is an environment result, not an M6 protocol result.

## 5. Freeze the Plan-242 lane

After host qualification, run the existing lane verbatim:

I2PR_M6_JAVA_DRIVER=streaming bash tests/integration/m6-interop/run-java.sh

Plan 243 must not alter ReferenceStreamingService tunnel settings, alter ReferenceRawDestination, force the explicit-peer branch, manipulate Java RNG, retry until Router C is selected, add a second explicit peer, modify A/B/C addresses or roles, inject profiles/RouterInfos/LeaseSets/tunnels, enable netDb.alwaysQuery, widen any response/build window, change publication target, patch Java source/jars, or change production i2pr behavior.

The Plan-242 checker forbidden list remains authoritative.

## 6. Counted-attempt discipline

Once the host gate passes:

- commit all Plan-243 implementation/evidence changes first;
- use one exact implementation SHA for the counted budget;
- run three counted attempts;
- use fresh disposable A/B/C RouterContexts for every attempt;
- use unique evidence directories;
- make no between-attempt tuning;
- do not reuse a successful RouterContext;
- do not retry merely to obtain the one-in-four explicit-C branch;
- do not treat branch frequency as a pass criterion;
- keep raw Java logs scratch-only;
- retain Plan-242 45-second windows.

If a parser/probe/harness defect is found, mark the affected run non-counted, commit the narrow correction on a new SHA, and restart the attempt budget.

## 7. Per-attempt required evidence

### A. Stock bootstrap/candidate state

Record topology validity, non-zero exploratory inbound/outbound availability, B selectable/not-forever-banlisted, C selectable/not-forever-banlisted, B/C profile presence as diagnostics only, and whether the stock candidate-population gate passed.

Possible earliest terminals:

- P243-A-NONZERO-EXPLORATORY-NOT-READY
- P243-A-NO-STOCK-CLIENT-TUNNEL-CANDIDATE
- P243-A-TOPOLOGY-INVALID

### B. Helper profile and installed client pools

Re-prove the Plan-241 helper profile: inbound/outbound length 1, variance 0, quantity 1, backupQuantity 0, allowZeroHop false.

Record Plan-242 extended pool observations for both directions: installed count, non-zero count, zero-hop count, tunnel length including local, remote-hop count, first/last controlled role, contains B, contains C, exact one remote hop, exact one remote hop via C, and unexpected-peer flag.

Continuation requires only at least one non-zero inbound and outbound client tunnel with zero-hop absent.

Possible terminals:

- P243-B-HELPER-PROFILE-NOT-ONE-HOP
- P243-B-NONZERO-CLIENT-TUNNEL-NOT-BUILT direction=inbound|outbound|both
- P243-B-ZERO-HOP-CONTRADICTION
- P243-B-UNEXPECTED-PEER-IN-CLIENT-TUNNEL direction=inbound|outbound|both

Exact-via-C remains diagnostic only.

### C. Exact Streaming lookup

When the pair gate passes, run the existing driver and retain the exact correlation key:

helper DBID + target hash + exact Streaming ISJ job ID + Streaming epoch

Record B candidate eligibility, B membership in exact toTry, selected lookup outbound tunnel non-zero, selected tunnel bounded role/path facts, retained Plan-240 pre-query guard state, and exact B target DLM dispatch.

Possible terminals include retained Plan-240 pre-query classifications plus:

- P243-C-LOOKUP-ZERO-HOP-SELECTION-CONTRADICTION
- P243-C-B-QUERY-DISPATCHED

P243-C-B-QUERY-DISPATCHED is a stage marker; continue to the lookup-return chain.

## 8. Lookup-return continuation

After exact B query dispatch, evaluate in strict order:

1. B receives exact target DLM.
2. B target LS remains current and query-answerable.
3. B emits the answer.
4. A helper inbound-client tunnel receives reply DSM.
5. Helper client sub-DB installs target LS.
6. Lookup-success callback fires.

Possible terminals:

- P243-D-B-LOOKUP-NOT-RECEIVED
- P243-D-B-TARGET-LS-NOT-QUERY-ANSWERABLE
- P243-D-B-ANSWER-NOT-EMITTED
- P243-D-A-CLIENT-DSM-NOT-RECEIVED
- P243-D-A-CLIENT-SUBDB-NOT-INSTALLED
- P243-D-LOOKUP-SUCCEEDED

Do not infer a downstream failure from an upstream absence.

## 9. OCMOSJ and downstream continuation

Only after P243-D-LOOKUP-SUCCEEDED, resume Plan-239 ordering: usable target lease -> outbound response tunnel -> Garlic construction -> DispatchJob -> TunnelDispatcher.dispatchOutbound -> dispatch counters.

Then, only after dispatch is proven, reuse Plan-231/232: Router-A outbound gateway enqueue -> transit/endpoint processing -> route-derived target IBGW -> exact expected i2pr TunnelData -> tunnel recovery -> Garlic decode -> Streaming adapter.

Possible terminals:

- P243-E-OCMOSJ-NO-USABLE-LEASE
- P243-E-OCMOSJ-NO-OUTBOUND-TUNNEL
- P243-E-OCMOSJ-GARLIC-PREP-FAILED
- P243-E-DISPATCH-OUTBOUND-PROVEN
- P243-F-OUTBOUND-GATEWAY-NOT-ENQUEUED
- P243-F-TRANSIT-NOT-PROCESSED
- P243-F-TARGET-IBGW-NOT-PRESENT
- P243-F-TARGET-IBGW-NO-DISPATCH
- P243-F-I2PR-NO-EXPECTED-TUNNELDATA
- P243-G-I2PR-TUNNEL-RECOVERY-FAILED
- P243-G-I2PR-GARLIC-DECODE-FAILED
- P243-G-I2PR-STREAMING-ADAPTER-FAILED
- P243-G-DIRECTION-A-ESTABLISHED

No production i2pr corrective is authorized before exact expected TunnelData reaches i2pr.

## 10. Direction-A establishment

If Direction A establishes, record Java accept returned, SYN accepted, SYN-ACK observed by i2pr, i2pr state Established, small payload sent, and payload digest match.

P243-G-DIRECTION-A-ESTABLISHED does not close M6 by itself. Return authority to Plan 201 for remaining bidirectional/publication/final-closure rows.

## 11. Three-attempt interpretation

The closure must distinguish repeatability from stochastic selector branch choice.

Acceptable summaries include the same terminal 3/3; two runs reaching the same deeper terminal with one earlier stock-build stop; or three different typed terminals if every difference is explained by stock state with no tuning.

Do not require Router C 3/3, explicit branch observed at all, identical tunnel peer choice, or identical tunnel IDs.

If all three fail before a non-zero pair because the entire stock candidate population is absent, a later bootstrap-population corrective may be authorized. If usable candidates exist but the pair repeatedly fails, a stock client-build attribution successor may be authorized. If any run passes the pair gate, the deepest proven live downstream terminal governs the next plan.

## 12. Evidence implementation

Prefer no new Java probe and no new protocol code. Plan 243 may add only minimal host-preflight and evidence aggregation around existing Plan-242 surfaces.

Expected surfaces only if needed:

- tests/integration/m6-interop/run-java.sh
- crates/i2pr-daemon/tests/java_tunnel_external.rs
- scripts/check-m6-mixed-router-acceptance-evidence.sh
- Plan-243 closure/status docs.

Reuse P242-CLIENT-TUNNELS, the Plan-241 helper-profile command, Plan-240/241 driver, source-lock checker, and existing role/path parser.

A new Java helper/probe is not authorized unless execution proves observation impossible with retained probes; if that happens, stop and create a new plan rather than silently widening Plan 243.

## 13. Focused tests and static guards

Add only Plan-243 host/execution-authority tests, such as:

- p243_host_not_qualified_does_not_consume_attempt
- p243_host_gate_requires_exact_workspace_sha
- p243_host_gate_requires_java_reference_cache
- p243_host_gate_requires_i2pr_daemon
- p243_counted_attempt_requires_host_qualified
- p243_counted_attempt_reuses_plan242_nonzero_pair_gate
- p243_exact_via_c_not_required
- p243_explicit_branch_not_required
- p243_three_attempt_budget_no_retry_until_c
- p243_lookup_continuation_requires_nonzero_pair
- p243_production_change_requires_expected_tunneldata
- p243_direction_a_does_not_close_m6
- p243_no_production_change

Do not duplicate Plan-242 selector-semantics tests.

## 14. Verification floor

Before counted execution, pass cargo fmt/check; retained focused P225/P226/P228/P230/P237/P238/P239/P240/P241/P242 tests; any P243 focused tests; clippy with warnings denied; rustdoc/doc tests; cargo-deny; shell syntax checks; M6 evidence checker; exact Java source-lock checker; javac against exact-pinned jars; NTCP2 harness unit discovery.

After implementation/evidence changes, attempt the full serial workspace floor:

cargo test --locked --workspace --all-targets -- --test-threads=1

Record unrelated failures honestly. Do not modify another subsystem under Plan 243.

## 15. Acceptance criteria

Plan 243 closes correctly when:

1. A capable execution host is positively qualified before the attempt budget.
2. Exact Plan-242 harness semantics remain unchanged.
3. Plan-242 source-lock/static/unit floors remain green.
4. Counted attempts use one committed implementation SHA with no tuning.
5. Three counted attempts execute on fresh A/B/C RouterContexts unless a deterministic harness defect requires a new SHA/restart.
6. Every attempt emits exactly one ordered Plan-243 terminal/classification.
7. Exact-via-C and explicit-branch occurrence remain diagnostic only.
8. If a non-zero pair builds, the live Streaming driver runs.
9. If B is queried, reply/subDB stages are evaluated in order.
10. If lookup succeeds, OCMOSJ/downstream attribution continues.
11. No i2pr-owned defect is claimed before expected TunnelData.
12. Plan 201 and Plan 204 are updated together at closure.
13. Plan-242 status documentation is normalized to name 9b0e61c6c3f780a19e843e63d25804fb8756616a as its implementation-and-closure authority.

## 16. Successor authorization

Only the deepest live Plan-243 terminal may authorize the next plan:

- unqualified host -> environment/provisioning work only;
- stock candidate population absent 3/3 -> bootstrap-population corrective;
- usable candidates but non-zero pair repeatedly fails -> stock client-build attribution;
- lookup chooses zero-hop despite non-zero-only pool -> selector contradiction attribution;
- B query dispatches but reply chain fails -> exact lookup-return corrective;
- lookup succeeds but OCMOSJ fails -> exact OCMOSJ-stage corrective;
- expected TunnelData reaches i2pr then fails -> production i2pr corrective for that exact owned stage may be authorized;
- Direction A establishes -> return to Plan 201 final Java-family/publication closure.

Do not pre-register those branches.

## 17. Closure record

plans/closure/mixed-router-interop/243-status.md must record implementation SHA, host qualification facts, exact pins, evidence directories for all counted attempts, source-lock checksum, per-attempt candidate state, installed pool path-role facts, exact-via-C/explicit-C diagnostics, exact Streaming ISJ correlation if reached, B query/reply/subDB facts if reached, OCMOSJ/downstream/i2pr facts if reached, exact terminal per attempt, no-tuning proof, no-production-change proof, verification, and Plan-201/204 unblock audit.

## 18. Registration disposition

plan_242 = passed-m6-java-streaming-stock-one-hop-selector-semantics-corrective-with-corrected-bootstrap-and-pair-gate
plan_243 = registered-ready-m6-java-streaming-hosted-stock-client-build-qualification

plan_201 = blocked-after-plan242-corrected-gates-pending-plan243-and-publication-closure
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan243
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification

active_plan = none
next_executable_plan = 243-m6-java-streaming-hosted-stock-client-build-qualification
milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed
milestone10_final_acceptance = closed

## 19. Smaller-model handoff

Do not redesign the harness.

First prove the host has the exact Java artifacts and i2pr daemon. Then run the existing Plan-242 Streaming lane three times on one SHA.

Do not wait for Router C. Do not force the explicit branch. A normal non-zero-hop inbound/outbound pair is enough to continue.

Stop at the first real live boundary and record it.