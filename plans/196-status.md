# Plan 196 status — M6 Java I2P controlled first-run topology corrective

Status: **`registered-executable-m6-java-controlled-first-run-topology-corrective`**.

Plan of record:
[`plans/196-m6-java-controlled-first-run-topology-corrective.md`](196-m6-java-controlled-first-run-topology-corrective.md).

This is the newest handoff authority for the retained M6/M10 blocker line. It supersedes older status prose that names Plan 194 or an unnamed `194-followup-topology-corrective` as immediately executable.

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
plan_194 = in-progress-scaffolding-landed-blocked-by-plan196-topology-corrective
plan_196 = registered-executable-m6-java-controlled-first-run-topology-corrective
plan_195 = registered-blocked-by-plan194

milestone6_i2pd_streaming_interop = passed-via-plan193
m6_second_family_java = topology-corrective-pending-plan196
milestone6_interoperable = not-yet-claimed

milestone10_local_product = passed-via-plan180-and-plan182
milestone10_independent_application_clients = local-rows-passed-plan181-not-closed
milestone10_remote_service_interop = not-yet-passed
milestone10_final_acceptance = not-yet-closed

next_executable_plan = 196
resume_after_plan196 = 194
remaining_sequence = 196 -> resume-194 -> 195
```

## Why Plan 196 exists

Plan 193 closed the first-family i2pd qualification with 33/33 mixed-router Streaming rows passing twice on exact implementation head `3687189de651ba2b2d3483cbfded2c8e4a7278ef`.

Plan 194 then landed the Java second-family fetch/build, runner, external driver, cross-family aggregation, static evidence checking and hosted workflow scaffolding, but its first real Java run exposed a reference-startup/topology blocker before counted protocol qualification:

- a prewritten `router.config` does not reliably control a pristine Java router's first startup;
- Java selected a random UDP port instead of the requested fixed test port;
- public reseed activity occurred, violating the private controlled-topology acceptance contract;
- the harness also used several non-authoritative property names and mutated the verified Java cache's `clients.config`.

Exact-pinned Java source inspection provides a supported solution instead of a workaround:

1. `net.i2p.router.Router` explicitly supports embedded construction with `Router(Properties)` before any router threads start, followed by `setKillVMOnEnd(false)` and `runRouter()`.
2. The exact-pinned upstream `MultiRouter` utility uses precisely this pre-start property-injection pattern for isolated router instances with fixed loopback transport ports, isolated directories, `i2np.allowLocal=true`, and `router.reseedDisable=true`.
3. Upstream warns that `i2p.vmCommSystem=true` bypasses UDP/TCP, so Plan 196 explicitly forbids it for counted transport evidence.
4. Exact-pinned testnet/client configuration files provide the canonical leak-control and SAM property names; Plan 196 borrows those settings but deliberately retains the standard I2P network ID.

Pinned Java reference:

```text
Java I2P 2.13.0
commit = 9134f808337b401e8e53c73734c81fab04280c9d
```

## Plan 196 closure boundary

Plan 196 is topology-only. It passes only after the stock exact-pinned Java router starts in the controlled profile and the existing Plan 194 driver establishes an authenticated SSU2 session against it.

It is not allowed to claim or implement the remaining Java qualification rows:

```text
real one-hop tunnel build/liveness
NetDB lookup/publication
Standard LeaseSet2 resolution/publication
bidirectional ECIES/Garlic destination delivery
bidirectional Streaming
cross-family final evidence
milestone6_interoperable
```

Those return to Plan 194 after Plan 196 passes.

## Registration source floor

Plan 196 was registered from:

```text
i2pr main = 23a318e2910f27686c2974c618387e20e9b3d10a
routine CI = 34741206844 (success)
plan_193 = passed-m6-i2pd-mixed-router-streaming
plan_194 = in-progress-scaffolding-landed-blocked-at-java-first-run-topology
workspace floor recorded by Plan 194 = 2312 passed / 7 ignored
```

Routine Linux CI on that source floor includes the exploratory, NetDB, destination, Streaming and M6 cross-family static evidence checkers, plus the ordinary fmt/check/test/clippy/doc/dependency-policy gates.

## Handoff rule

Execute Plan 196 only.

Do not resume Plan 194's tunnel/NetDB/destination/Streaming qualification until Plan 196 has a passing status proving the controlled Java topology plus authenticated SSU2 preflight. Do not execute Plan 195 until Plan 194 closes the two-family Milestone 6 criterion.

On Plan 196 pass, authority becomes:

```text
plan_196 = passed-m6-java-controlled-first-run-topology-corrective
plan_194 = in-progress-resume-java-second-family-qualification
plan_195 = registered-blocked-by-plan194
m6_second_family_java = topology-and-authenticated-ssu2-preflight-passed-via-plan196
milestone6_interoperable = not-yet-claimed
next_executable_plan = 194
remaining_sequence = resume-194 -> 195
```

If the controlled topology is correct but the existing external driver exposes a genuine Java/i2pr SSU2 semantic mismatch, Plan 196 must stop and register one narrow Plan 194 protocol corrective rather than expanding into protocol changes itself.
