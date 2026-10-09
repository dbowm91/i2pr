# Plan 408 — Host-Owned Loopback Local-Service Ingress for Managed Apps

Status: **passed-host-owned-loopback-local-service-ingress**

Global number reconciliation: the imported draft used Plan 387, which Proposal 170 owns on main. This successor is Plan 408; the source draft is preserved at `plans/archive/managed-native-app-runtime/387-host-owned-local-service-ingress.md`.

Date: 2026-10-09

Roadmap: `plans/subsystems/managed-native-app-runtime-roadmap.md`

Hard dependencies:

- Managed native app runtime/369–371 closed.
- Plans 382–383 closed.
- Managed native app runtime/407 — qualified Linux Secured host and private persistent app-data root — closed.

Primary consumers:

- i2pr-tc Transmission RPC;
- future mail and IRC local client surfaces.

Classification: capability + security invariant + protocol extension.

## 1. Objective

Allow an authorized managed application to publish a bounded **daemon-owned**
loopback TCP service without receiving a listening socket or general host-network
authority. `i2pr-daemon`, which already owns router listeners, binds and accepts
connections; `i2pr-appd` remains a separate listener-free process and carries
only typed events over its existing inherited anonymous pipe.

The daemon forwards accepted byte streams through the manager protocol and the
existing managed-app channel. The application sees logical stream ids, not host
sockets.

This owner follows the hard process boundary in `AGENTS.md` and
`docs/architecture/i2pr-appd.md`: neither appd nor apphost may own a listener,
port, or discovery endpoint. The daemon remains the only router process that
binds this loopback listener.

This is the generic primitive previously described as
`PublishedLocalService`; do not revive the underspecified `brokered_tcp`
reservation for inbound services.

## 2. Capability and versioning

Add one explicit capability literal, `local_service`.

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
- `i2pr-daemon` owns listener lifetime; appd carries only typed request and
  incoming-stream events over the inherited anonymous pipe.

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

Frozen bounds: names are unique ASCII `[A-Za-z0-9_-]` tokens of 1–64 bytes;
up to 8 services and 16 accepted local-service streams per session; preferred
ports are 1024–65535, while an omitted port requests OS-selected ephemeral
allocation. A bind conflict fails without retry. Each stream has an 8-frame
inbound queue and each data frame is at most 65,536 bytes. The endpoint reports
only the selected port; binding is always `127.0.0.1`.

Accepted connection bytes then use the existing logical stream data/close/reset
framing.

Names, listener count, concurrent connections, queued accepts, per-stream
buffering, and control requests all receive explicit ceilings.

The returned endpoint is informational for local tooling; it is not authority.

## 5. Ownership and isolation

- `i2pr-daemon` owns the `TcpListener`, accepted sockets, port selection, and
  teardown. `i2pr-appd` owns no socket and receives no descriptor; it requests
  publication and carries incoming stream events over its existing anonymous
  daemon channel.
- `i2pr-daemon` correlates manager session, app principal, service id, and
  logical stream id before forwarding any bytes. The app protocol remains
  runtime-neutral; no socket types or Tokio ownership are added to
  `i2pr-app-proto`.
- The service capability is grantable only through persistent administrator
  policy. A Secured app receives no direct network syscall authority; its
  published listener is a distinct explicit host capability. UnsafeDirect
  retains its existing no-containment claim.

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

Why ready: Plans 369–371 provide the supervised appd/apphost lifecycle and
bounded app protocol, Plans 382–383 provide signed package identity and
administrator-origin persistent grants, and Plan 407 qualifies the Secured
profile. This plan supplies the currently absent local-service protocol and
daemon listener owner. The ownership question is resolved by the existing hard
boundary: the daemon binds the socket; appd never does.

### WP1 — contract/ADR

Freeze the minor-version bump, capability literal, transaction grammar, port
semantics, ceilings, and daemon socket ownership. Add an ADR that explicitly
keeps `i2pr-appd` listener-free and assigns the loopback `TcpListener` to
`i2pr-daemon`.

### WP2 — runtime-neutral protocol model

Add strict request/reply/event types and ownership handles to
`i2pr-app-proto` or the appropriate manager protocol without socket code.

### WP3 — daemon-owned listener

Implement loopback listener ownership in `i2pr-daemon`, using its existing
socket/runtime ownership. Extend the private manager bridge with bounded,
direction-strict publication requests and incoming-stream events. `i2pr-appd`
must remain listener-free, and no managed app may call bind as part of this
feature.

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

Plan 408 closes when:

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

Create `plans/closure/managed-native-app-runtime/408-status.md` with protocol
version/capability changes, the daemon-owned listener / inherited-pipe event
ownership diagram, exact endpoint policy, cross-app/revocation/load evidence,
compatibility matrix, routine-floor results, and downstream unblock decisions
for torrent/mail/IRC consumers.
