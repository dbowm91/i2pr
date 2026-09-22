# Plan 242 — M6 Java Streaming stock one-hop selector semantics corrective and lookup continuation

Status: **registered-ready-m6-java-streaming-stock-one-hop-selector-semantics-corrective**

## 1. Objective

Correct the Plan-241 harness interpretation of Java client-tunnel peer selection, then resume the exact Streaming lookup/response continuation already implemented by Plans 240–241.

Plan 241 proved one inbound and one outbound non-zero-hop client tunnel can install with zero-hop absent, but stopped because neither tunnel was exact-via-C. Exact-pinned Java source now proves that explicitPeers is intentionally probabilistic: TunnelPeerSelector.shouldSelectExplicit() returns true only when an explicit peer is configured and random.nextInt(4) == 0. Only that branch calls selectExplicit(); otherwise ClientPeerSelector.selectPeers() uses normal stock fast-peer selection.

Therefore exact-via-C is not a valid hard prerequisite. A genuine non-zero-hop outbound client tunnel is sufficient to bypass the Plan-240 outTunnel length <= 1 zero-hop/unknown-RI guard.

Plan 242 corrects that harness gate. It does not patch Java, force Router C, manipulate RNG, or change production i2pr.

## 2. Exact pins and retained authority

Keep Java I2P 2.13.0 at 9134f808337b401e8e53c73734c81fab04280c9d and i2pd 2.61.0 at 635b013a612ff47278ef02acf8580a28e10e26c5.

Retain unchanged:

- A = service/floodfill, B = publication/floodfill, C = transit/non-floodfill;
- loopback-only controlled topology;
- Plan-230 reachability/capability correction;
- Plan-232 route-derived lease-gateway correction and raw-Destination bidirectional pass;
- Plans 237–240 response/admission/lookup attribution;
- Plan-241 one-hop Streaming SessionConfig with allowZeroHop=false;
- the 45-second response windows;
- Router-B publication target.

## 3. Exact Java selector semantics

Source-lock these facts:

1. TunnelPeerSelector.getLength(settings) derives requested remote-hop count from length, lengthOverride, and lengthVariance.
2. For non-exploratory client pools, shouldSelectExplicit(settings) reads explicitPeers and returns true only when random.nextInt(4) == 0.
3. Only that branch calls selectExplicit(settings, length).
4. Otherwise ClientPeerSelector.selectPeers() follows normal client fast-peer selection.
5. In explicit mode, selectExplicit validates explicit peer selectability, trims/fills to requested length, and inserts the local router into route order.
6. Plan 240's zero-hop guard depends on the selected outbound tunnel length, not on whether Router C was selected.

The checker must reject claims that explicitPeers means every build uses C, that explicit peer selection is deterministic, or that exact-via-C is required to bypass the zero-hop guard.

## 4. Corrective scope

Authorized test/harness changes:

- replace Plan-241 exact-via-C hard gating with a non-zero-hop client-pair gate;
- keep the Plan-241 one-hop helper SessionConfig unchanged;
- keep explicitPeers=C as a source-accurate optional/debug selection hint;
- record actual installed client-tunnel length and controlled-peer role;
- record whether C was used, but as diagnostic only;
- enter the existing Plan-241 Streaming driver once the client pair is genuinely non-zero-hop;
- reuse Plan-240 exact target-job correlation and Plan-225 post-query ordering;
- continue to Plan-239 OCMOSJ and Plan-231/232 delivery stages only when each preceding stage is proven.

Forbidden:

- Java source changes;
- forcing shouldSelectExplicit or RNG state;
- retry-until-C behavior;
- direct tunnel/profile/RI/LeaseSet injection;
- netDb.alwaysQuery;
- topology, publication, or timing changes;
- production i2pr changes;
- SAM pivot.

## 5. Correct the pre-helper/bootstrap gate

Plan 241 required Router C specifically to be naturally profiled before helper startup because it also required exact-via-C tunnels. That dependency is not valid under stock selector semantics.

Plan 242 must instead prove the minimum prerequisites for a stock one-hop client build:

- controlled topology valid;
- non-zero exploratory infrastructure present in both directions;
- helper requests length=1, variance=0, quantity=1, backups=0, allowZeroHop=false;
- at least one usable stock client-tunnel candidate exists.

Router-C readiness remains observable because explicit mode may choose it, but C profile absence alone must not stop helper startup if another valid stock candidate exists.

If no usable candidate population exists, stop with:
P242-A-NO-STOCK-CLIENT-TUNNEL-CANDIDATE

If non-zero exploratory infrastructure is absent, stop with:
P242-A-NONZERO-EXPLORATORY-NOT-READY

Do not fake candidate availability.

This consumes the Plan-241 bootstrap-repeatability branch unless Plan 242 proves the entire stock candidate population itself is absent or unstable.

## 6. Installed client-pool observation

Extend the existing P227/P228/P241 read-only pool observer rather than creating a new general harness.

For inbound and outbound client pools record bounded facts:

- configured length and variance;
- allowZeroHop;
- installed count;
- zero-hop count;
- non-zero count;
- tunnel length including local router;
- remote-hop count;
- first and last remote role as B, C, other-controlled, or unknown;
- contains B;
- contains C;
- exact one-remote-hop;
- exact one-remote-hop-via-C.

Do not persist arbitrary peer hashes when a controlled role label is sufficient.

If an unknown peer appears in the isolated topology, stop with:
P242-B-UNEXPECTED-PEER-IN-CLIENT-TUNNEL

## 7. Corrected continuation gate

Before sending the counted Streaming SYN require:

Helper profile:
- inbound.length=1;
- outbound.length=1;
- variance=0;
- allowZeroHop=false.

Installed client pool:
- inbound installed >= 1;
- outbound installed >= 1;
- inbound non-zero >= 1;
- outbound non-zero >= 1;
- inbound zero-hop = 0;
- outbound zero-hop = 0.

Do not require exact-via-C.

Record inbound/outbound contains-C and exact-via-C as diagnostic only.

If a non-zero client tunnel is missing, stop with:
P242-B-NONZERO-CLIENT-TUNNEL-NOT-BUILT direction=inbound|outbound|both

If any zero-hop tunnel exists despite allowZeroHop=false, stop with:
P242-B-ZERO-HOP-CONTRADICTION

## 8. Exact Streaming lookup continuation

Once the corrected gate passes, run the already-implemented Plan-241 driver.

Retain exact correlation:
helper DBID + target hash + exact Streaming ISJ job ID + Streaming epoch.

Re-prove:

- Router B lookup-candidate eligible;
- B in exact toTry;
- selected lookup outbound tunnel non-zero-hop;
- zero-hop-unknown guard did not fire.

Record the lookup-selected outbound tunnel's bounded path facts.

If lookup selects zero-hop despite an authoritative non-zero-only client pool, stop:
P242-C-LOOKUP-ZERO-HOP-SELECTION-CONTRADICTION

If the exact target DLM dispatches to B, record:
P242-C-B-QUERY-DISPATCHED

If another retained Plan-240 pre-query guard fires, emit that earliest exact terminal and stop.

## 9. Post-query continuation

Only after B query dispatch evaluate the retained Plan-225 chain in order:

B receives target DLM
→ B target LS current/query-answerable
→ B emits answer
→ A helper inbound-client tunnel receives reply DSM
→ helper client subDB installs target LS
→ lookup success callback.

Allowed terminals:

P242-D-B-LOOKUP-NOT-RECEIVED
P242-D-B-TARGET-LS-NOT-QUERY-ANSWERABLE
P242-D-B-ANSWER-NOT-EMITTED
P242-D-A-CLIENT-DSM-NOT-RECEIVED
P242-D-A-CLIENT-SUBDB-NOT-INSTALLED
P242-D-LOOKUP-SUCCEEDED

Never infer a later stage from an earlier absence.

## 10. OCMOSJ and delivery continuation

Only after P242-D-LOOKUP-SUCCEEDED resume Plan-239 ordering:

usable target lease
→ client outbound response tunnel
→ Garlic construction
→ DispatchJob
→ TunnelDispatcher.dispatchOutbound
→ client.dispatchTime / client.dispatchSendTime.

Then, only after dispatch is proven, reuse Plan-231/232:

Router A outbound gateway
→ transit/endpoint processing
→ route-derived target IBGW
→ exact expected i2pr TunnelData
→ tunnel recovery
→ Garlic decode
→ Streaming adapter.

No production i2pr corrective is authorized before exact expected TunnelData reaches i2pr.

If Direction A establishes, record SYN/SYN-ACK, Established state, and a small payload digest match, then return final authority to Plan 201. A Direction-A pass alone does not close M6.

## 11. Expected implementation surfaces

Prefer only:

- crates/i2pr-daemon/tests/java_tunnel_external.rs
- tests/integration/m6-interop/run-java.sh
- tests/integration/m6-interop/java/ControlledRouter.java
- existing P227/P228/P230/P241 probe source
- scripts/check-m6-mixed-router-acceptance-evidence.sh
- scripts/interop/check-m6-java-response-source-lock.sh

ReferenceStreamingService.java should require no further behavior change; its Plan-241 one-hop profile remains correct.

ReferenceRawDestination.java remains frozen.

## 12. Focused tests

Add equivalents of:

- p242_explicit_peers_is_probabilistic_not_mandatory
- p242_exact_via_c_is_diagnostic_not_gate
- p242_nonzero_pair_is_lookup_prerequisite
- p242_zero_hop_remains_forbidden
- p242_c_profile_absence_alone_does_not_block_helper
- p242_no_candidate_population_stops_before_helper
- p242_unknown_client_tunnel_peer_fails_closed
- p242_installed_path_records_remote_hop_count
- p242_lookup_selected_tunnel_must_be_nonzero
- p242_plan240_exact_job_correlation_retained
- p242_b_query_requires_exact_dispatch
- p242_b_receipt_requires_query
- p242_b_answer_requires_b_receipt
- p242_client_dsm_requires_b_answer
- p242_subdb_install_requires_client_dsm
- p242_ocmosj_resume_requires_lookup_success
- p242_i2pr_terminal_requires_expected_tunneldata
- p242_direction_a_pass_does_not_close_m6
- p242_no_production_change

Retain P225/P226/P228/P230/P237/P238/P239/P240/P241 focused suites.

## 13. Attempt discipline

- commit implementation before counted attempts;
- maximum three counted attempts per implementation SHA;
- no tuning between attempts;
- fresh A/B/C RouterContexts;
- no RNG manipulation;
- no retry-until-explicit-C;
- no requirement that explicit mode occur;
- no zero-hop fallback;
- same 45-second Streaming windows;
- parser/probe defect requires a new SHA and restarted budget;
- raw logs scratch-only;
- unique evidence directories.

## 14. Verification floor

Run the normal locked workspace format/check/clippy/doc/cargo-deny floor, shell syntax and M6 evidence checker, exact Java source-lock checker, javac of staged helpers, NTCP2 harness unit discovery, focused p225/p226/p228/p230/p237/p238/p239/p240/p241/p242 java_tunnel_external tests, and the full serial workspace all-target test run.

## 15. Acceptance criteria

Plan 242 closes correctly only when:

1. The one-in-four explicit-peer semantics are source-locked.
2. Plan-241 one-hop/zero-hop-forbidden helper settings remain unchanged.
3. Exact-via-C is removed as a hard continuation requirement.
4. The C-specific profile gate is replaced with actual stock candidate/pool readiness evidence.
5. Installed client paths are observed with bounded length/role facts.
6. Unknown peers fail closed.
7. A genuine non-zero-hop inbound/outbound pair is sufficient to enter the Streaming driver.
8. Exact Plan-240 target-job correlation is retained.
9. Lookup reply/install stages remain ordered.
10. OCMOSJ resumes only after lookup success.
11. No i2pr-owned defect is claimed before expected TunnelData.
12. No Java source/topology/RNG/profile/publication/timing/raw-helper/production workaround is introduced.
13. Plan 201 and Plan 204 are updated together at closure.

## 16. Successor authorization

Register a successor only from the exact Plan-242 boundary:

- no stock candidate population → bootstrap/profile-population repeatability corrective;
- non-zero client tunnel not built → client build-path attribution;
- lookup still selects zero-hop → selector contradiction attribution;
- B query dispatches but return chain fails → exact lookup-reply corrective;
- lookup succeeds but OCMOSJ fails → resume Plan-239 stage;
- expected TunnelData reaches i2pr then fails → exact production corrective may be authorized;
- Direction A establishes → return to Plan 201 final Java-family closure.

Do not pre-register those branches.

## 17. Closure evidence

plans/closure/mixed-router-interop/242-status.md must record implementation SHA(s), pins, source-lock proof of one-in-four explicit selection, unchanged helper profile, stock candidate facts, installed route lengths/roles, whether C was used as diagnostic only, zero-hop counts, lookup-selected tunnel facts, exact Streaming ISJ correlation, any B query/reply/subDB facts, OCMOSJ/tunnel/i2pr continuation, per-attempt terminals, no-production-change proof, verification results, and the Plan-201/204 unblock audit.

## 18. Registration disposition

plan_241 = passed-m6-java-streaming-one-hop-client-tunnel-fixture-corrective-with-a-b-build-stage-boundary
plan_242 = registered-ready-m6-java-streaming-stock-one-hop-selector-semantics-corrective

plan_201 = blocked-after-plan241-a-b-build-stage-pending-plan242-and-publication-closure
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan242
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification

active_plan = none
next_executable_plan = 242-m6-java-streaming-stock-one-hop-selector-semantics-corrective

milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed
milestone10_final_acceptance = closed

## 19. Smaller-model handoff

Do not make Java use Router C.

The key source fact is that explicitPeers is selected only one build out of four. Plan 241 already proved one inbound and one outbound non-zero-hop client tunnel can install with zero-hop absent.

Correct the harness so:

non-zero-hop pair = continuation allowed
exact-via-C = diagnostic only

Do not block helper startup only because C lacks a profile when another stock client-tunnel candidate exists.

Then run the existing Streaming driver and stop at the first new exact boundary.
