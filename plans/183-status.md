# Plan 183 status — M6 mixed-router destination/Streaming interop program

Status: **`registered-m6-mixed-router-streaming-interop-program`**.

Plan of record:
[`plans/183-m6-mixed-router-streaming-interop-program.md`](183-m6-mixed-router-streaming-interop-program.md).

## Current authority

```text
plan_192 = registered-m6-i2cp-wire-format-corrective (next executable; inbound-delivery boundary E I2CP-style Data body)
plan_191 = stopped-by-inbound-delivery-boundary-E (4 inbound-delivery rows documented; 2 rows recorded blocked; 2 ordering rows flipped passed)
plan_190 = passed-m6-inbound-netdb-reply-path-tunnel-id-corrective (local rows passed; remote lane proves 3 destination rows flipped blocked -> passed)
plan_188 = blocked-by-plan191-and-plan192 (real outbound/inbound i2pd installs retained-passed; 5/7 destination rows flipped via plan188 installs + plan190 reply-path correction; 2 inbound-delivery rows blocked on plan192; 2 ordering rows passed)
plan_183 = registered-m6-mixed-router-streaming-interop-program
milestone10_final_acceptance = not-yet-closed
next_executable_plan = 192 (resolve inbound-delivery boundary E wire-format; plan 183 scopes M10 remote-Streaming work)
```

## What landed

Registration only: the program Plan 181 §6.3 requires before
Milestone 10 can close (independent-router HTTP/IRC service
rows). Entry criteria, non-goals, and the resume condition are
stated in the plan of record. No `crates/` changes, no execution
claim, no M10 closure claim.

Triggering evidence: the Plan 181 full-lane qualification
attempt against exact-pinned i2pd 2.61.0
(`635b013a612ff47278ef02acf8580a28e10e26c5`), classified
`m6-mixed-router-streaming-blocker` with command/log provenance
(see `plans/181-status.md`).

## Handoff

Scope and execute the 183 series next. Resume Plan 181 only
after passing remote rows exist.
