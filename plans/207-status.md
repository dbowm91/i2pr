# Plan 207 status — genuine remote HTTP/IRC application interoperability corrective

Status: **`registered-blocked-by-plan206`**.

Plan of record: [`207-m10-genuine-remote-http-and-irc-application-interop-corrective.md`](207-m10-genuine-remote-http-and-irc-application-interop-corrective.md).

Plan 207 corrects the Plan 203 evidence gap. The current Plan 203 scaffolding proves manager routing classification plus lower-layer remote Streaming, but the final HTTP/IRC rows are not yet authoritative because their documented facts can be advanced manually rather than being derived from actual system curl and exact-pinned jaraco/irc commands through the production M10 listeners.

Current authority:

```text
plan_203 = partial-m10-remote-application-evidence-scaffolding-superseded-by-plan207
plan_206 = registered-executable-m10-production-remote-delivery-corrective
plan_207 = registered-blocked-by-plan206
plan_181 = passed-local-matrix-remote-rows-blocked-pending-plan207
plan_195 = blocked-m10-remote-independent-service-pending-plan206-and-plan207
milestone10_remote_service_interop = not-yet-passed
```

Plan 207 must use actual system curl and the exact-pinned jaraco/irc public API through real i2pr HTTP/IRC client listeners. Aggregate pass rows must be derived from command exit codes, parsed application results, remote fixture facts, and Plan 206 backend observations from the same run. Manual observation-label loops are not acceptance evidence.

On success:

```text
plan_207 = passed-m10-genuine-remote-http-and-irc-application-interop
milestone10_remote_service_interop = evidence-passed-via-plan206-and-plan207-pending-plan204
```

Final authority convergence remains blocked on both Plan 205 and Plan 207.
