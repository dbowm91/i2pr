# Plan 387 — Host-Owned Loopback Local-Service Ingress for Managed Apps

Status: **ready — parallel managed-runtime capability**

Date: 2026-10-09

Roadmap: `plans/subsystems/managed-native-app-runtime-roadmap.md`

Hard dependencies:

- Managed native app runtime/369–371 closed.
- Plans 382–383 closed.

Primary consumers:

- i2pr-tc Transmission RPC;
- future mail and IRC local client surfaces.

Classification: capability + security invariant + protocol extension.

## 1. Objective

Allow an authorized managed application to publish a bounded **host-owned**
local TCP service without receiving a listening socket or general host-network
authority.

The trusted app runtime binds loopback and forwards accepted byte streams over
the existing managed-app channel. The application sees logical stream ids, not
host sockets.

This is the generic primitive previously described as
`PublishedLocalService`; do not revive the underspecified `brokered_tcp`
reservation for inbound services.

## 2. Capability and versioning

Add one explicit capability literal, tentatively `local_service`.

Because Plan 383 can persist signed package manifests and protocol ranges, this
is an observable contract extension. Advance the managed-app protocol minor
version rather than pretending v1.0 already contained the capability.

Compatibility rules:

- v1.0 applications continue to decode and operate with the old capability set;
- an app requesting `local_service` must declare a host protocol range that
  includes the new minor version;
- unknown capability literals still fail closed;
- local-service grants remain administrator-origin and cannot be minted from an
  app request.

`brokered_tcp` remains reserved for future outbound clearnet brokering.

## 3. First profile

Implement loopback TCP only.

- bind address is runtime-selected loopback, never package/app input;
- no wildcard/LAN/public bind;
- no Unix-domain or named-pipe publication claim in this plan;
- app may request a preferred unprivileged port or port 0/automatic according
  to the frozen transaction;
- bind conflict returns a typed failure and does not fall back to a different
  externally visible port unless the request explicitly allowed automatic
  selection;
- host/runtime owns listener lifetime.

## 4. Protocol transaction

Freeze a bounded request/reply/event vocabulary equivalent to:

```text
application -> host:
  publish_local_service(request_id, service_name, preferred_port?)
  unpublish_local_service(request_id, service_id)

host -> application:
  local_service_published(request_id, service_id, endpoint)
  local_service_unpublished(request_id, service_id)
  local_service_incoming(service_id, stream_id)
```

Accepted connection bytes then use the existing logical stream data/close/reset
framing.

Names, listener count, concurrent connections, queued accepts, per-stream
buffering, and control requests all receive explicit ceilings.

The returned endpoint is informational for local tooling; it is not authority.

## 5. Ownership and isolation

- one published service belongs to one `AppPrincipal`;
- another app cannot unpublish, accept, or attach to it;
- application exit/revocation closes its listeners and active forwarded
  streams;
- manager/router shutdown closes all listeners;
- a connection accepted before revocation cannot remain usable afterwards;
- the application never receives the listener fd/socket handle;
- the runtime must not forward proxy metadata or remote host headers that create
  a false trust boundary.

For loopback, the incoming peer address is not an authentication fact.

## 6. Policy integration

Extend Plan-383 manifest/policy handling so:

- package may request `local_service`;
- administrator may grant/revoke it explicitly;
- selection/trust does not auto-grant;
- the app can publish only while the effective capability contains it;
- no port/name policy is accepted from unverified package bytes outside the
  bounded request.

A future live administrator API may change grants, but this plan uses the
existing restart-applied policy owner.

## 7. Work packages

### WP1 — contract/ADR

Freeze the minor-version bump, capability literal, transaction grammar, port
semantics, and ceilings.

### WP2 — runtime-neutral protocol model

Add strict request/reply/event types and ownership handles to
`i2pr-app-proto` or the appropriate manager protocol without socket code.

### WP3 — trusted listener owner

Implement loopback listener ownership in the trusted app runtime. No managed app
may call bind as part of this feature.

### WP4 — logical-stream forwarding

Map each accepted TCP connection to one bounded app logical stream with exact
ordered bytes and independent close/reset.

### WP5 — persistent grant integration

Add `local_service` to requested/granted/effective capability handling and
offline policy CLI.

### WP6 — qualification

Use a fixture application to publish a service, exchange bytes, exercise
concurrency/backpressure, restart, revoke, bind conflict, and cross-app denial.

## 8. Security and negative tests

Required negative evidence:

- request without grant;
- app asks for non-loopback address or attempts to supply one;
- privileged/out-of-range port;
- duplicate service id/name policy;
- listener/connection ceilings;
- cross-app service handle reuse;
- app crash with open connections;
- grant revocation/restart;
- malicious client stalls after connect;
- app stalls after incoming notification;
- no host listener remains after teardown.

The runtime must remain bounded under a connection flood.

## 9. Acceptance criteria

Plan 387 closes when:

1. an authorized app publishes loopback TCP without receiving a listener socket;
2. an independent local client exchanges exact bytes through a forwarded logical
   stream;
3. no LAN/public/wildcard bind is expressible;
4. cross-app ownership is enforced;
5. revocation/exit/shutdown closes listener and streams;
6. connection and buffering ceilings hold under adversarial load;
7. v1.0 compatibility is preserved and the new minor version is explicit;
8. `brokered_tcp` remains unavailable;
9. routine and black-box verification pass.

## 10. Stop conditions

Stop if implementation requires granting the application direct loopback
networking, overloading `brokered_tcp`, binding wildcard interfaces, or
treating loopback source identity as authentication.

## 11. Closure evidence

Create `plans/closure/managed-native-app-runtime/387-status.md` with protocol
version/capability changes, listener ownership diagram, exact endpoint policy,
cross-app/revocation/load evidence, compatibility matrix, routine-floor
results, and downstream unblock decisions for torrent/mail/IRC consumers.
