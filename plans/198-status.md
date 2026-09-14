# Plan 198 status — M6 Java public-client final closure corrective

Status: **`blocked-public-java-client-leaseset2-publication`**.

Plan of record:
[`plans/198-m6-java-public-client-final-closure-corrective.md`](198-m6-java-public-client-final-closure-corrective.md).

This is the newest handoff authority for the retained M6/M10 blocker line. It supersedes the Plan 194 status interpretation that treated the exact-pinned Java SAM LeaseSet2 publication limitation as sufficient for final M6 closure.

## Execution checkpoint

The Plan 198 public-client helpers and Rust driver path are implemented and
the exact-pinned local lane was exercised repeatedly. Both public Java helper
sessions connect through the selected I2CP port and the Rust destination and
Streaming tests themselves return `ok`, but the Java 2.13.0 controlled router
does not return the published public-client LeaseSet2 to the real i2pr
DatabaseLookup path. The destination lane therefore stops at
`plan194-java-stop` after 45 seconds with no lookup reply, and the Streaming
lane stops before outbound build installation. Publication was tested both by
omitting the option and by explicitly setting
`i2cp.dontPublishLeaseSet=false`; neither changes the result. The final gate
remains fail-closed: Plan 198 is not passed and Milestone 6 remains
unclaimed.

Latest retained local evidence:

```text
target/interop/m6-java-evidence-plan198-11
Java public-client tests: 2 passed, 0 failed at the Rust test-process level
mandatory Java rows: blocked at public-client LeaseSet2 publication
```

## Why M6 is reopened

The current Java second-family run is valuable but does not satisfy Plan 194's own final acceptance contract:

```text
Java lane = 26 passed / 22 blocked / 0 failed
```

The blocked rows are not ancillary. They include the mandatory LeaseSet2-lookup-dependent, destination-delivery-dependent, and Streaming layers. Plan 194 §12 requires those layers plus a successful exact-head full two-family external workflow before `milestone6_interoperable` may be claimed.

Routine exact-head CI on the source floor is green, but routine CI is not a replacement for the missing external two-family closure evidence.

Therefore:

```text
plan_194_final_closure_claim = superseded-by-plan198
milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed
```

The passing sub-evidence from Plans 193, 196, and 197 remains retained.

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
plan_193 = passed-m6-i2pd-mixed-router-streaming
plan_196 = passed-m6-java-controlled-first-run-topology-corrective
plan_197 = passed-m6-pq-ssu2-option-support-corrective
plan_194 = retained-partial-java-qualification-sam-ls2-publication-boundary
plan_198 = blocked-public-java-client-leaseset2-publication
plan_195 = registered-blocked-by-plan198

milestone6_i2pd_streaming_interop = passed-via-plan193
milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed

milestone10_local_product = passed-via-plan180-and-plan182
milestone10_independent_application_clients = local-rows-passed-plan181-not-closed
milestone10_remote_service_interop = not-yet-passed
milestone10_final_acceptance = not-yet-closed

next_executable_plan = 198 (resolve the exact-pinned Java public-client LeaseSet2 publication boundary)
remaining_sequence = 198 -> 195
```

## Corrective direction

Plan 198 keeps the Java SAM result as diagnostic compatibility evidence but removes it as the counted service-destination gate.

The counted Java reference application must use the exact-pinned router's **public client APIs**, reusing the high-level lifecycle already proven in the repository's Plan 172 Java driver:

```text
tests/integration/i2cp/external/java/i2cp_java_session_driver.java
```

That path already demonstrates public `I2PClient` / `I2PSession.connect()` with zero-hop quantity-1 Standard LeaseSet2 (`i2cp.leaseSetType=3`, `i2cp.leaseSetEncType=4`). Plan 198 changes the reference service profile so `i2cp.dontPublishLeaseSet` is absent/false, allowing the Java-owned service destination to publish its LS2 normally.

For Streaming, use exact-pinned public `I2PSocketManagerFactory` / `I2PSocketManager` / `I2PServerSocket` / `I2PSocket` APIs. The helper must remain out-of-tree/test-only and must not implement I2CP/Streaming wire framing or call private router APIs.

Reference-side zero-hop tunnels are permitted; all counted **i2pr** paths must remain real mixed-router one-hop tunnels.

## Required closure condition

Plan 198 cannot pass with a bounded blocked set.

Final evidence must report:

```text
mandatory i2pd rows: all passed, 0 blocked, 0 failed, 0 missing
mandatory Java rows: all passed, 0 blocked, 0 failed, 0 missing
```

and must include at least one successful exact-head manual:

```text
M6 mixed-router external interoperability
```

workflow run covering both exact-pinned families plus the final evidence checker.

## Registration source floor

Plan 198 was registered from:

```text
i2pr main = aba33e4fe2e0f27ca0786f4a25a549aec5cc2df6
routine CI = 34794444271 (success)
workspace = 2333 passed, 9 ignored
Java external evidence = 26 passed / 22 blocked / 0 failed
```

Reference pins remain:

```text
i2pd 2.61.0 = 635b013a612ff47278ef02acf8580a28e10e26c5
Java I2P 2.13.0 = 9134f808337b401e8e53c73734c81fab04280c9d
```

## Handoff rule

Execute Plan 198 only.

Do not execute Plan 195 until Plan 198 records command-derived all-pass two-family evidence, exact-head routine CI success, and at least one exact-head full external workflow success.

On Plan 198 pass, and only then, authority becomes:

```text
plan_194 = retained-partial-java-qualification-sam-ls2-publication-boundary
plan_198 = passed-m6-java-public-client-complete-second-family-closure
milestone6_java_mixed_router_interop = passed-via-plan198
milestone6_interoperable = passed-via-plan193-and-plan198
plan_195 = unblocked-m10-remote-service-interop
next_executable_plan = 195
remaining_sequence = closed-m6-via-plan193-and-plan198 -> 195
```
