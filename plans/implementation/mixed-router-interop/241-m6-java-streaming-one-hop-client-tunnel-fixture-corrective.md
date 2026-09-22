# Plan 241 — M6 Java Streaming one-hop client-tunnel fixture corrective and lookup continuation

Status: **registered-ready-m6-java-streaming-one-hop-client-tunnel-fixture-corrective**

## 1. Objective
Remove the exact Plan-240 controlled-fixture cause of the Java Streaming response lookup failure using only ordinary public I2CP SessionConfig options, then continue the already-instrumented lookup/response path until the next real boundary. Plan 240 proved the exact Streaming ISJ, Router B eligibility and toTry membership, and then stopped at `P240-C-B-ZERO-HOP-UNKNOWN-RI`: Router B was never queried because the selected client outbound tunnel was zero-hop while the helper client NetDB did not contain Router B RI.

## 2. Corrective authorization
The current `ReferenceStreamingService` explicitly requests `inbound.length=0`, `outbound.length=0`, quantities 1, backups 0, and `inbound.allowZeroHop=true` / `outbound.allowZeroHop=true`. The fixture therefore guarantees a zero-hop client pool. Exact-pinned Java `TunnelPoolManager.selectOutboundTunnel(destination, closestTo)` selects from that destination client pool, and `IterativeSearchJob.sendQuery()` applies `_facade.lookupLocallyWithoutValidation(peer)` when the selected tunnel length is <= 1. Plan 240 explicitly authorized a successor limited to this zero-hop-tunnel / RI-availability boundary. This is a test-fixture corrective, not a production protocol workaround.

## 3. Retained authority and pins
Keep Java I2P 2.13.0 at `9134f808337b401e8e53c73734c81fab04280c9d` and i2pd 2.61.0 at `635b013a612ff47278ef02acf8580a28e10e26c5`. Retain Plan 230 controlled reachability/profile bootstrap, Plan 232 raw bidirectional authority, Plans 237–240 response/admission/lookup attribution, A=service B=publication C=transit roles, Router-B publication target, the 45-second response windows, and exact target/job correlation. `ReferenceRawDestination.java` and Plan-232 raw authority are frozen.

## 4. Exact-pinned mechanics
`TunnelPoolManager.selectOutboundTunnel(destination, closestTo)` gets `_clientOutboundPools.get(destination)` and calls `pool.selectTunnel(closestTo)`; it does not fall back to exploratory when a client pool exists. `TunnelPool.selectTunnel(Hash closestTo)` uses `TunnelInfoComparator`; when `allowZeroHop=false`, zero-hop tunnels sort after non-zero-hop tunnels. Plan 240 also exposed the important facade split: `sendQuery()` reads Router B RI from `ctx.netDb().lookupRouterInfoLocally(peer)` for router-global send preparation, but the zero-hop safety guard checks `_facade.lookupLocallyWithoutValidation(peer)`, where `_facade` is the helper client NetDB. Thus main-NetDB B-RI presence and helper-client B-RI absence are compatible facts. A genuine non-zero-hop client tunnel bypasses that guard.

## 5. Streaming helper corrective
Mirror the already-proven optional explicit-peer contract in `ReferenceRawDestination` for the Streaming helper. For the Plan-241 lane use only ordinary SessionConfig options: `inbound.length=1`, `outbound.length=1`, inbound/outbound quantity 1, backupQuantity 0, `inbound.allowZeroHop=false`, `outbound.allowZeroHop=false`, and inbound/outbound `explicitPeers=<Router-C>`. Do not alter lease-set type, encryption type, publication flags, message reliability, Destination generation, Streaming behavior, response scheduling, or router-global tunnel settings.

## 6. Pre-helper bootstrap gate
Before starting the corrected Streaming helper, reuse the Plan-230 public/read-only Router-C bootstrap gate: C main RI present and valid; reachability predicate eligible; profile naturally present; selectable; not banlisted; C remains non-floodfill transit; and non-zero exploratory infrastructure is available both directions. No profile injection, `heardAbout()` forcing, RI store, direct tunnel install, or topology tuning. If this does not become ready within retained Plan-230 bounds, emit `P241-A-TRANSIT-BOOTSTRAP-NOT-READY` and stop. Never fall back to zero-hop.

## 7. Streaming client-pool proof
After helper connect and before the Direction-A SYN, reuse P228/P230 public tunnel-pool inspection. Record: inbound/outbound counts, inbound/outbound non-zero counts, inbound/outbound zero-hop counts, exact-via-C booleans, and configured length/allowZeroHop values. Continuation requires at least one inbound and outbound tunnel, both non-zero, exact-via-C in both directions, and zero zero-hop tunnels. If corrected settings are active but the pair does not build, emit `P241-B-ONE-HOP-CLIENT-TUNNEL-NOT-BUILT direction=<inbound|outbound|both>` and stop.

## 8. Main-NetDB vs helper-client NetDB B-RI visibility
Immediately before the exact Streaming lookup epoch record read-only `router_a_main_b_ri_raw_present`, `router_a_main_b_ri_valid_present`, `helper_client_b_ri_raw_present`, and `helper_client_b_ri_valid_present`. These are diagnostic facts, not a requirement to populate the client DB. The allowed/expected shape is main B RI present, helper-client B RI absent, non-zero-hop client tunnel selected. Never copy/store B RI into the helper client DB.

## 9. Exact Streaming lookup continuation
Once the bootstrap and client-pair gates pass, run the retained Plan-240 exact classifier with helper DBID + target hash + exact Streaming ISJ job ID, Router B eligible/in exact toTry, and negative-cache false. If the old zero-hop guard fires despite a proven non-zero outbound pool, emit `P241-C-ZERO-HOP-GUARD-CONTRADICTION`. If any zero-hop outbound remains in the authoritative pool, emit `P241-C-ZERO-HOP-STILL-IN-POOL`. If the exact target DLM is dispatched to B, emit the continuation fact `P241-C-B-QUERY-DISPATCHED` and proceed. If a different exact Plan-240 pre-query guard stops B, reuse that earliest supported terminal and stop.

## 10. Retained post-query chain
Only after actual B query dispatch evaluate the retained P225 chain in order: B receives exact target DLM -> B target LS is current/query-answerable -> B emits answer -> A helper inbound-client tunnel receives encrypted DSM -> helper client NetDB installs target LS -> OCMOSJ lookup success. Allowed stop terminals: `P241-D-B-LOOKUP-NOT-RECEIVED`, `P241-D-B-TARGET-LS-NOT-QUERY-ANSWERABLE`, `P241-D-B-ANSWER-NOT-EMITTED`, `P241-D-A-CLIENT-TUNNEL-DSM-NOT-RECEIVED`, `P241-D-A-CLIENT-SUBDB-NOT-INSTALLED`, `P241-D-LOOKUP-SUCCEEDED`. Never infer later stages from earlier ones.

## 11. OCMOSJ and i2pr continuation
Only after `P241-D-LOOKUP-SUCCEEDED` may the run resume Plan-239 ordering: usable target lease -> outbound response tunnel -> Garlic -> DispatchJob -> `dispatchOutbound` -> Router-A outbound gateway -> transit -> target IBGW -> exact expected i2pr TunnelData -> recovery/Garlic -> Streaming adapter. Stop at the first proven boundary. No production-i2pr defect is authorized before exact expected TunnelData arrives at i2pr.

## 12. Direction-A continuation and final authority
If Direction A establishes, record SYN accepted, SYN-ACK received, state Established, and a small payload digest match. A Direction-A pass does not close M6. Plan 201 still owns bidirectional Streaming, multi-packet payloads, refresh/republication, close/EOF/sibling isolation, Plan-200/201 client-LS2 lifecycle rows, and final wrapper/M6 checker.

## 13. Expected implementation surfaces
Prefer test-only changes in `ReferenceStreamingService.java`, `run-java.sh`, `java_tunnel_external.rs`, `check-m6-mixed-router-acceptance-evidence.sh`, and `check-m6-java-response-source-lock.sh`. Reuse P228/P230/P240 probes. Do not add another helper if existing probes can expose client-pool settings/tunnel paths and main-vs-client B-RI visibility.

## 14. Static checker requirements
Reject implementations that modify `ReferenceRawDestination.java`; patch Java reference source/jars; store B RI into the client DB; directly install tunnels; mutate profiles/tiers; set `netDb.alwaysQuery`; change A/B/C roles or addresses; change Plan-230 C1/C2 semantics; widen timing windows; permit zero-hop in the Plan-241 Streaming lane; omit explicit Router C; claim B query from selector membership; claim lookup success without client-subDB target LS presence; claim an i2pr defect before exact expected TunnelData; add P241 production `src/` code; promote raw Java logs; or forgive mandatory rows with `|| true` / `REQUIRED_FAILED` suppression.

## 15. Source-lock additions
Lock exact-pinned source facts for `TunnelPoolManager.selectOutboundTunnel(destination, closestTo)`, `_clientOutboundPools.get(destination)`, `pool.selectTunnel(closestTo)`, `TunnelPool.selectTunnel(Hash closestTo)`, `_settings.getAllowZeroHop()`, `TunnelInfoComparator(target, avoidZeroHop)` and its zero-hop-last rule, plus `IterativeSearchJob` main-NetDB RI lookup, client-facade lookup, `outTunnel.getLength() <= 1`, the zero-hop-unknown log, and `dispatchOutbound(outMsg, outTunnel.getSendTunnelId(0), peer)`. Retain all Plan-236–240 source-lock rows.

## 16. Required focused tests
Add equivalents of: `p241_streaming_helper_requires_one_hop_profile`; `p241_streaming_helper_zero_hop_is_forbidden`; `p241_raw_helper_is_unchanged`; `p241_explicit_peer_must_match_router_c`; `p241_bootstrap_gate_precedes_helper_start`; `p241_client_pair_requires_nonzero_both_directions`; `p241_zero_hop_pool_cannot_continue`; `p241_main_ri_and_client_ri_are_distinct_facts`; `p241_client_ri_absence_does_not_fail_nonzero_lookup`; `p241_zero_hop_guard_requires_selected_zero_hop`; `p241_b_query_requires_exact_streaming_job`; `p241_b_receipt_requires_query`; `p241_client_subdb_install_requires_a_dsm`; `p241_ocmosj_resume_requires_lookup_success`; `p241_i2pr_terminal_requires_expected_tunneldata`; `p241_direction_a_pass_does_not_close_m6`; `p241_no_production_change`. Retain P225/P226/P228/P230/P237/P238/P239/P240 suites.

## 17. Attempt discipline
Commit before counted execution. Maximum three counted attempts per implementation SHA, no tuning between attempts, fresh A/B/C RouterContexts, no fallback to zero-hop, new SHA/budget for parser/probe defects, raw logs scratch-only, unique evidence dirs. Known Plan-230 bootstrap stochasticity must be represented as a typed terminal, not corrected mid-budget.

## 18. Verification floor
Run `cargo fmt --all --check`; `cargo check --locked --workspace --all-targets`; focused p225/p226/p228/p230/p237/p238/p239/p240/p241 `java_tunnel_external` tests; workspace clippy with `-D warnings`; rustdoc with `-D warnings`; workspace doc tests; `cargo deny check advisories bans sources`; shell syntax and `check-m6-mixed-router-acceptance-evidence.sh`; exact source-lock checker; javac of staged helpers against exact-pinned jars; NTCP2 harness unit discovery; and full serial `cargo test --locked --workspace --all-targets -- --test-threads=1`. Record exact outcomes.

## 19. Acceptance criteria
Plan 241 closes correctly only when Plan-240 authority is retained; the Streaming zero-hop fixture is removed for this lane; raw helper remains unchanged; C bootstrap is proven before helper start; a genuine one-hop client pair through C is installed or an exact build-stage terminal closes the run; zero-hop tunnels are absent from continuation; B-RI visibility is recorded separately for main/client facades; the exact Streaming ISJ is correlated; the zero-hop-unknown terminal disappears or is proven contradictory; the retained B-query/reply/client-subDB chain is evaluated in order when reached; no production/Java-source/topology/profile/publication/timing workaround is introduced; counted attempts obey same-SHA/no-tuning discipline; and Plan-201/204 are updated together at closure.

## 20. Successor authorization
Register a successor only from the exact final boundary: bootstrap not ready -> bootstrap repeatability; one-hop pair not built -> stock client-build corrective; zero-hop selected despite verified non-zero pool -> selector contradiction attribution; B query sent but reply chain fails -> B-answer/A-DSM corrective; lookup succeeds but OCMOSJ fails later -> resume exact Plan-239 D/E attribution; expected TunnelData reaches i2pr then fails -> production corrective may be authorized for that exact i2pr-owned stage; Direction A establishes -> final bidirectional/publication closure remains with Plan 201. Do not pre-register these.

## 21. Closure record
`plans/closure/mixed-router-interop/241-status.md` must record implementation SHA(s), exact pins, helper SessionConfig before/after, proof raw helper unchanged, Router-C bootstrap facts, actual Streaming client-pool tunnel lengths/paths, main-vs-client Router-B RI visibility, exact Streaming ISJ correlation, Plan-240 zero-hop terminal disposition, B-query/reply/client-subDB facts if reached, OCMOSJ/i2pr continuation if reached, per-attempt terminal, no-production-change proof, focused/full verification, and Plan-201/204 unblock audit.

## 22. Registration disposition
```text
plan_240 = passed-m6-java-streaming-target-leaseset-lookup-failure-attribution-with-b-zero-hop-unknown-ri-boundary
plan_241 = registered-ready-m6-java-streaming-one-hop-client-tunnel-fixture-corrective

plan_201 = blocked-after-plan240-b-zero-hop-unknown-ri-pending-plan241-and-publication-closure
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan241
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification

next_executable_plan = 241-m6-java-streaming-one-hop-client-tunnel-fixture-corrective
milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed
milestone10_final_acceptance = closed
```

## 23. Smaller-model handoff
Do not investigate the whole Streaming stack. The old helper is explicitly zero-hop. Reuse Plan-230 Router-C bootstrap; start Streaming helper with the proven raw-helper one-hop SessionConfig; prove installed inbound/outbound client tunnels are one-hop via C and zero-hop absent; record B RI in main vs helper client NetDB; send the same Direction-A SYN; correlate the exact Streaming ISJ; prove the old zero-hop guard no longer fires; if B is queried walk P225; if lookup succeeds resume Plan-239 OCMOSJ; stop at the first new proven boundary.