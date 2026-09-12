# Plan 193 status — M6 i2pd mixed-router Streaming qualification

Status: **`registered-executable-m6-i2pd-mixed-router-streaming`**.

Plan of record:
[`plans/193-m6-i2pd-mixed-router-streaming-qualification.md`](193-m6-i2pd-mixed-router-streaming-qualification.md).

This status is the newest handoff authority for the retained M6/M10 blocker line. It intentionally resolves the duplicate-numbering defect created when both the short-build corrective and the deferred Streaming pass were left under Plan 188. Historical files remain in place for evidence provenance, but their execution roles are superseded as follows:

```text
historical plans/188-m6-mixed-router-streaming-with-i2pd.md -> superseded-by-plan193
historical plans/189-m6-java-i2p-second-family-qualification-and-closure.md -> superseded-for-execution-by-plan194
plan_189 cross-family ledger/checker/workflow scaffold -> retained
```

## Current authority

```text
plan_184 = passed-m6-authenticated-i2np-runtime-and-reference-preflight
plan_185 = passed-m6-live-one-hop-exploratory-tunnels-and-liveness
plan_186 = passed-m6-mixed-router-netdb-lookup-and-publication
plan_187_local_product = retained-passed
plan_188_short_build_corrective = retained-passed
plan_190 = passed-m6-inbound-netdb-reply-path-tunnel-id-corrective
plan_191 = stopped-boundary-diagnosis-retained
plan_192 = passed-m6-i2cp-wire-format-corrective
plan_193 = registered-executable-m6-i2pd-mixed-router-streaming
plan_194 = registered-blocked-by-plan193
plan_195 = registered-blocked-by-plan194

m6_destination_remote_interop_i2pd = passed-through-raw-destination-message-plane-via-plan192
milestone6_i2pd_streaming_interop = not-yet-passed
m6_second_family_java = not-yet-started
milestone6_interoperable = not-yet-claimed

milestone10_local_product = passed-via-plan180-and-plan182
milestone10_independent_application_clients = local-rows-passed-plan181-not-closed
milestone10_remote_service_interop = not-yet-passed
milestone10_final_acceptance = not-yet-closed

next_executable_plan = 193
remaining_sequence = 193 -> 194 -> 195
```

## Source floor

Registration source floor:

```text
i2pr main = 05d6d52870a81eda8891dc492d43c6d4b79bbae8
plan_192 = passed-m6-i2cp-wire-format-corrective
routine CI = 34668461179 (success)
workspace = 2283 passed, 6 ignored
```

Plan 192 proved the complete i2pd destination message plane through real SSU2, real one-hop tunnels, live NetDB, Standard LeaseSet2, ECIES/Garlic, and the corrected short-transport/I2CP-style Data envelope in both directions. Streaming and Java second-family qualification were explicitly not yet run.

## Handoff rule

Execute Plan 193 only. Do not begin Plan 194 until Plan 193 has a passing status with exact-head external i2pd Streaming evidence. Do not begin Plan 195 until Plan 194 closes the two-family M6 criterion.

On Plan 193 pass, update this authority chain to:

```text
plan_193 = passed-m6-i2pd-mixed-router-streaming
milestone6_i2pd_streaming_interop = passed-via-plan193
milestone6_interoperable = not-yet-claimed
next_executable_plan = 194
```

If Plan 193 hits a specific Streaming semantic defect, register one narrow corrective above 195 rather than reusing or mutating the historical Plan 188 numbering.
