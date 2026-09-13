# Plan 195 status — M10 remote independent service interoperability and final closure

Status: **`registered-blocked-by-plan194`**.

Plan of record:
[`plans/195-m10-remote-independent-service-final-closure.md`](195-m10-remote-independent-service-final-closure.md).

Plan 195 resumes only the two remote service rows left blocked by Plan 181 after the local M10 product and independent local client matrix passed. It must not execute until Plan 194 closes M6 mixed-router interoperability. Plan 194 is currently paused behind the narrow Plan 196 Java controlled-first-run-topology corrective; Plan 196 does not replace Plan 194 as the M6 closure gate. Plan 196's first counted external run stopped at the §10.B PQ option rejection; Plan 197 has landed the narrow PQ SSU2 option support corrective (parser-only tolerance, typed `Ssu2PqKem`/`PqCapabilities` surface, bounded `MAX_SSU2_PQ_SCHEMES = 8`, no ML-KEM implementation). The current executable plan is now Plan 196 (re-run external lane).

```text
plan_193 = passed-m6-i2pd-mixed-router-streaming
plan_194 = in-progress-scaffolding-landed-blocked-by-plan196-topology-corrective
plan_196 = in-progress-corrective-implementation-landed-static-checks-green-stopped-at-§10B-authenticated-ssu2-pq-option-rejection-pq-parser-tolerance-landed-via-plan197-pending-external-re-run
plan_197 = implementation-landed-parser-tolerance-static-floor-green-pending-plan196-external-re-run
plan_195 = registered-blocked-by-plan194
milestone6_interoperable = not-yet-claimed
milestone10_remote_service_interop = not-yet-passed
milestone10_final_acceptance = not-yet-closed
next_executable_plan = 196 (re-run external lane; session-established-java row must flip on the exact-pinned Java 2.13.0 cache; the Plan 197 parser tolerance is already in place)
resume_after_plan196_external = 194 (resume Java second-family qualification)
remaining_sequence = 196-execute -> resume-194 -> 195
```

On Plan 196 external re-run pass, Plan 194 resumes the actual Java second-family tunnel/NetDB/destination/Streaming qualification. On Plan 194 pass, Plan 195 becomes executable. On Plan 195 pass:

```text
milestone10_remote_service_interop = passed-via-plan195
milestone10_final_acceptance = closed-via-plan195
next_product_layer = milestone11-planning
```
