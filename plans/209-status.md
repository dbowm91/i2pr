# Plan 209 status — M10 product-only remote HTTP/IRC acceptance corrective

Status: **`retained-partial-black-box-composition-harness-superseded-by-plan211`**.

Plan of record: [`209-m10-product-only-remote-http-and-irc-application-acceptance-corrective.md`](209-m10-product-only-remote-http-and-irc-application-acceptance-corrective.md).

## Retained value

Plan 209 corrected the largest Plan 207 harness defect by moving lower-stack i2pr construction behind the production `ServiceProduct` composition boundary. The counted application driver no longer directly constructs the parallel Streaming/routing/tunnel/router stack. Real system `curl`, exact-pinned `jaraco/irc`, exact-pinned i2pd setup, and the anti-shadow static checker are useful retained work.

## Why Plan 209 is not final remote application evidence

The post-implementation audit found additional product/evidence gaps:

- `ServiceProduct` provisions router-level exploratory material but does not provision real destination tunnel material for each service Destination;
- service runtimes are still prepared through the local `SamLocalProductFabric` path;
- `ServiceProduct` performs a reference LeaseSet2 lookup using `SHA256(reference.router_info_bytes)` instead of the actual remote service Destination hash;
- recovered Garlic is not actually delivered through the owning service DestinationDispatcher/ECIES/Streaming path;
- the counted Plan 209 application driver can build an empty `ServiceTunnelSet` while relying on externally supplied expected listener ports;
- several mandatory application/policy/cleanup success facts are not yet causally tied to command, fixture, and per-application counter-delta evidence.

Plan 210 fixes the product path. Plan 211 then re-runs HTTP/IRC as real black-box product acceptance.

## Corrected authority

```text
plan_207 = retained-partial-real-application-client-harness
plan_209 = retained-partial-black-box-composition-harness-superseded-by-plan211
plan_210 = registered-executable
plan_211 = registered-blocked-by-plan210
m10_remote_application_interop = not-yet-passed
milestone10_remote_service_interop = not-yet-passed
milestone10_final_acceptance = not-yet-closed
```

Do not delete the useful `ServiceProduct`/curl/jaraco scaffolding. Plan 211 should tighten and reuse it after Plan 210 lands.
