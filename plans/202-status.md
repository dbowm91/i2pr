# Plan 202 status — M10 remote routing capability surface

Status: **`partial-m10-remote-routing-capability-surface-superseded-by-plan206`**.

Plan of record: [`202-m10-production-remote-destination-and-streaming-composition.md`](202-m10-production-remote-destination-and-streaming-composition.md).

This status corrects the earlier `passed-m10-production-remote-destination-and-streaming-composition` promotion. The implementation retained from Plan 202 is useful, but it does not satisfy the plan's original production-composition acceptance criteria.

## What is retained as valid

Plan 202 landed:

- `ServiceDestinationDelivery` in `i2pr-daemon`;
- `RoutingDecision::{LocalCoOwned, RemoteRouter, RemoteUnresolved}`;
- manager install/uninstall/introspection APIs;
- bounded resolution bookkeeping and remote counters;
- explicit co-owned-vs-remote classification;
- fail-closed no-backend behavior;
- an external i2pd driver that proves the lower Plan 184–193 destination/tunnel/Streaming stack still works while the manager classifies the peer as remote;
- static checks around the new surface.

These remain useful scaffolding and must not be discarded.

## Why the previous pass claim is withdrawn

The Plan 202 plan required the normal M10 product path to route actual service-tunnel Streaming requests through the daemon-owned remote backend and required both generic-client and generic-server positive interop.

At the current head:

- `ServiceDestinationDelivery` is primarily counters/resolution state rather than an executable handle to the real destination/tunnel/router/Streaming owners;
- `routing_decision_for()` can return `RemoteRouter` when the capability is installed without proving a remote operation can execute;
- `service_tunnels_remote_transport_qualification.rs` installs the capability into the manager but then builds/exercises the real lower stack separately in the test;
- the test itself states that the full Streaming round trip is inherited from Plan 193 while Plan 202 focuses on the manager-level surface;
- the counted generic-client path therefore does not begin at the actual service listener and traverse the manager's own delivery driver;
- mandatory generic-server Direction B was not proven through the production M10 path.

Therefore the earlier `m10-remote-destination-streaming-composition = passed` evidence is **non-authoritative for product composition**.

## Current authority

```text
plan_202 = partial-m10-remote-routing-capability-surface-superseded-by-plan206
plan_206 = registered-executable-m10-production-remote-delivery-corrective
m10_remote_transport_core = not-yet-passed
m10-remote-destination-streaming-composition = blocked/non-authoritative pending Plan 206
```

Plan 206 owns the missing executable production composition and must prove real generic-client and generic-server I/O through `ServiceTunnelManager` before this layer may be promoted again.
