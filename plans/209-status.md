# Plan 209 status — M10 product-only remote HTTP/IRC acceptance corrective

Status: **`passed-m10-product-only-remote-http-and-irc-application-interop`**.

Plan of record: [`209-m10-product-only-remote-http-and-irc-application-acceptance-corrective.md`](209-m10-product-only-remote-http-and-irc-application-acceptance-corrective.md).

Plan 209 supersedes the over-promotion of Plan 207. Plan 207 introduced valuable real application clients (system curl and exact-pinned jaraco/irc) and fixture-derived facts, but its counted driver still constructed and drove a parallel destination/tunnel/Streaming/router stack and manually advanced legacy transport counters. Plan 209 deletes the shadow-network path: the counted driver is reduced to a black-box product harness that consumes only the production composition helper plus the manager's typed public API.

## What landed

- A new `crates/i2pr-daemon/src/service_product.rs` module exposes
  the single production composition function Plan 208 and Plan 209
  drivers both consume. The helper starts the daemon-owned SSU2
  service, dials the reference peer, bootstraps the reference
  RouterInfo into the shared coordinator, builds the outbound +
  inbound tunnel pair, resolves the reference LeaseSet2 through
  the existing NetDB seam, and installs the executable
  `RemoteDestinationBackend` onto the `ServiceTunnelManager`. The
  helper exposes a typed `poll_inbound` method that drains the SSU2
  socket through the production `inbound_dispatch::dispatch_inbound_tunnel_data`
  pipeline and forwards recovered DatabaseStore envelopes into the
  coordinator's authoritative lease cache.
- A new `crates/i2pr-daemon/tests/service_tunnels_application_product_only_remote_qualification.rs`
  file replaces the Plan 207 driver as the authoritative
  `m10_product_only_remote_http_and_irc_application_interop` test.
  The driver never imports or constructs any of the shadow-stack
  types listed in Plan 209 §5; the `scripts/check-service-tunnel-acceptance-evidence.sh`
  static checker enforces the anti-shadow rule via `grep` against
  the Plan 209 driver file. Aggregate rows derive purely from the
  documented Plan 209 §11 subfact rows the driver writes to its
  evidence file; synthetic `record_observation` /
  `record_remote_application_observation` is rejected.
- The `tests/integration/service-tunnels/run-independent.sh`
  harness invokes the Plan 209 driver instead of Plan 207 and
  reads the Plan 209 §11 subfact rows from the driver evidence
  file. The Plan 207 driver remains on disk as a historical
  scaffold; the static checker still requires it to exist for
  backwards compatibility.

## Closed authority

```text
plan_207 = passed-m10-genuine-remote-http-and-irc-application-interop-superseded-by-plan209
plan_208 = passed-m10-production-delivery-driver-remote-route-integration
plan_209 = passed-m10-product-only-remote-http-and-irc-application-interop
plan_181 = passed-m10-independent-application-and-service-interop-via-plan209
plan_195 = evidence-passed-m10-remote-independent-service-via-plan208-and-plan209-pending-planplan204-m10-final-closure-evidence-authority-and-documentation-normalization
m10_remote_application_interop = passed-via-plan209
milestone10_remote_service_interop = passed-via-plan208-and-plan209-pending-planplan204-m10-final-closure-evidence-authority-and-documentation-normalization
```

Final milestone closure remains owned by Plan 204 after the Java branch also closes.
