# Plan 203 status — M10 remote application evidence scaffolding

Status: **`partial-m10-remote-application-evidence-scaffolding-superseded-by-plan207`**.

Plan of record: [`203-m10-positive-remote-http-and-irc-application-interop.md`](../../implementation/service-tunnels/203-m10-positive-remote-http-and-irc-application-interop.md).

This status withdraws the earlier `passed-m10-positive-remote-http-and-irc-application-interop` promotion while retaining the useful test scaffolding that landed.

## What is retained as valid

Plan 203 added:

- an exact-pinned i2pd external qualification harness;
- HTTP/IRC service specifications using independent public Destination material;
- manager-level `RoutingDecision::RemoteRouter` assertions;
- a bounded documented observation-label surface;
- service-tunnel checker wiring and full-lane environment gating;
- deterministic remote fixture scaffolding and cleanup plumbing;
- preservation of the 29 retained local rows.

Those pieces are useful inputs to Plan 207.

## Why the previous pass claim is withdrawn

The original Plan 203 acceptance contract required actual system curl and exact-pinned jaraco/irc traffic through the real M10 HTTP/IRC listeners, with application facts derived from command output and independent fixture observations.

The current Plan 203 Rust driver instead:

- constructs/exercises the lower Plan 184–193 router/tunnel/Streaming stack directly;
- installs the Plan 202 capability into the manager primarily for routing classification;
- manually advances documented HTTP/IRC observation labels;
- emits terminal `http-remote-application-established=true` and `irc-remote-application-established=true` after the lower-stack exercise;
- does not bind the final aggregate pass to actual system curl/jaraco command exit codes and corresponding remote fixture facts through the production manager path.

Therefore the rows:

```text
remote-independent-http-eepsite
remote-independent-irc-service
```

are currently **non-authoritative** and must be treated as blocked pending Plan 206 + Plan 207.

## Current authority

```text
plan_203 = partial-m10-remote-application-evidence-scaffolding-superseded-by-plan207
plan_206 = registered-executable-m10-production-remote-delivery-corrective
plan_207 = registered-blocked-by-plan206
m10_remote_application_interop = not-yet-passed
remote-independent-http-eepsite = blocked/non-authoritative
remote-independent-irc-service = blocked/non-authoritative
```

Plan 207 owns genuine command-derived application evidence after Plan 206 establishes the real product transport path.
