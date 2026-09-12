# Plan 194 status — M6 Java second-family qualification and final mixed-router closure

Status: **`registered-blocked-by-plan193`**.

Plan of record:
[`plans/194-m6-java-second-family-mixed-router-closure.md`](194-m6-java-second-family-mixed-router-closure.md).

Historical `plans/189-m6-java-i2p-second-family-qualification-and-closure.md` remains retained for the already-landed Plan 189 cross-family evidence scaffold, but its execution role is superseded by Plan 194 so the active sequence is monotonic and unambiguous.

```text
plan_193 = registered-executable-m6-i2pd-mixed-router-streaming
plan_194 = registered-blocked-by-plan193
plan_195 = registered-blocked-by-plan194
m6_second_family_java = not-yet-started
milestone6_interoperable = not-yet-claimed
next_executable_plan = 193
```

Plan 194 may begin only after Plan 193 records passing exact-head i2pd Streaming evidence. On pass, Plan 194 is the only authority allowed to set `milestone6_interoperable = passed-via-plan194` and advance to Plan 195.