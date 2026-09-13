# Plan 195 status — M10 remote independent service interoperability and final closure

Status: **`registered-blocked-by-plan194`**.

Plan of record:
[`plans/195-m10-remote-independent-service-final-closure.md`](195-m10-remote-independent-service-final-closure.md).

Plan 195 resumes only the two remote service rows left blocked by Plan 181 after the local M10 product and independent local client matrix passed. It must not execute until Plan 194 closes M6 mixed-router interoperability. Plan 194 is currently paused behind the narrow Plan 196 Java controlled-first-run-topology corrective; Plan 196 does not replace Plan 194 as the M6 closure gate.

```text
plan_193 = passed-m6-i2pd-mixed-router-streaming
plan_194 = in-progress-scaffolding-landed-blocked-by-plan196-topology-corrective
plan_196 = registered-executable-m6-java-controlled-first-run-topology-corrective
plan_195 = registered-blocked-by-plan194
milestone6_interoperable = not-yet-claimed
milestone10_remote_service_interop = not-yet-passed
milestone10_final_acceptance = not-yet-closed
next_executable_plan = 196
resume_after_plan196 = 194
remaining_sequence = 196 -> resume-194 -> 195
```

On Plan 196 pass, Plan 194 resumes the actual Java second-family tunnel/NetDB/destination/Streaming qualification. On Plan 194 pass, Plan 195 becomes executable. On Plan 195 pass:

```text
milestone10_remote_service_interop = passed-via-plan195
milestone10_final_acceptance = closed-via-plan195
next_product_layer = milestone11-planning
```
