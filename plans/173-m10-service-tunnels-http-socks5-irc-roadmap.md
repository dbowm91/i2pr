# Plan 173 — Milestone 10 service tunnels, HTTP, SOCKS5, and IRC roadmap

Status: **registered planning authority after Milestone 9 closure**.

Source floor: `01c03bccafc7e6fab9f691425f2af20b802bc7cd`.

## 1. Objective

Deliver the Milestone 10 application-facing tunnel set without creating a second router, destination, or Streaming stack:

- generic TCP client tunnel: local loopback listener -> remote I2P destination;
- generic TCP server tunnel: local I2P destination -> loopback TCP or Unix-socket target;
- `.i2p`-only HTTP proxy including HTTP CONNECT;
- SOCKS5 CONNECT proxy;
- IRC client tunnel profile with privacy filtering/rewriting;
- IRC server tunnel profile with authenticated peer-Destination hostname projection;
- bounded listener/connection resources, transactional startup/reconcile/shutdown, and independent acceptance evidence.

Expected sequence:

```text
173 roadmap / architecture authority
  -> 174 service-tunnel foundation + shared Streaming runtime
  -> 175 generic client/server tunnels + persistent server identities
  -> 176 HTTP .i2p proxy + CONNECT
  -> 177 SOCKS5 CONNECT
  -> 178 IRC client profile
  -> 179 IRC server profile
  -> 180 product composition / reconcile / hardening
  -> 181 independent acceptance / final M10 closure
```

No Plan 174+ implementation may begin before this roadmap is registered.

## 2. Current source-of-truth constraints

Milestone 9 is closed via Plan 172. Retain the M6/M7/M8/M9 products and their evidence unchanged.

Current architecture facts:

1. `i2pr-client::streaming::StreamingManager` is runtime-neutral and destination-scoped.
2. `i2pr-client::StreamingDestinationAdapter` is the single existing Streaming <-> destination-routing composition seam. M10 must reuse it; service tunnels may not construct Garlic/I2NP/tunnel traffic independently.
3. `i2pr-daemon::sam::raw_stream` contains a mature bounded TCP <-> Streaming pump, but it is coupled to `SamServiceState`. M10 must extract/recompose the generic mechanism rather than copy it.
4. The roadmap already names `crates/i2pr-service-tunnels`; that crate does not exist on the source floor.
5. `DestinationIdentity` has no storage-backed constructor yet. Stable server-tunnel destinations therefore require a deliberate persistence pass, not ad-hoc secret serialization.
6. `GUARDRAILS.md` requires application tunnel services to avoid transport/tunnel-build internals, all long-lived socket work to be supervised, and all application listeners/connections to be bounded.

## 3. Architecture lock

### 3.1 New runtime-neutral crate

Create:

```text
crates/i2pr-service-tunnels
```

Ownership:

- service-tunnel configuration model and validation;
- destination references / static name aliases;
- HTTP proxy parser and rewrite policy;
- SOCKS5 state machine;
- IRC line parser/filter/profile policy;
- typed service events/errors/snapshots that carry no socket handles or secrets.

Prohibited in this crate:

- Tokio;
- `TcpListener`, `TcpStream`, Unix sockets;
- timers/tasks;
- filesystem access;
- `i2pr-transport-*` or tunnel-build internals;
- direct NetDB mutation;
- Garlic/I2NP construction.

Expected dependency direction:

```text
i2pr-client
    ^
    |
i2pr-service-tunnels   (policy/protocol only)
    ^
    |
i2pr-daemon            (only socket/task/composition owner)
```

If a more precise acyclic dependency shape is needed during Plan 174, preserve the guardrail that service-tunnel protocol/policy code only sees narrow destination/Streaming-facing types.

### 3.2 Shared daemon Streaming endpoint runtime

Extract a reusable daemon-side capability from the proven SAM implementation:

```text
local TCP/Unix socket
       <-> bounded supervised byte pump
       <-> StreamingManager
       <-> StreamingDestinationAdapter
       <-> existing DestinationRuntime / routing
```

The generic capability must own:

- one local socket after handoff;
- bounded read/write chunking;
- Streaming send-window backpressure;
- drain of bytes for exactly one Streaming connection;
- cancellation / EOF / remote-close convergence;
- cleanup of connection/listener ownership.

SAM should reuse the shared primitive where practical, but its SAM wire/state semantics must remain unchanged. A refactor that breaks Plan 151/152 acceptance is not an acceptable M10 foundation.

### 3.3 Service destination ownership

- Server tunnels require stable router-owned destinations. Persist destination signing/X25519 secrets through a versioned, permission-hardened `i2pr-storage` format with atomic create/load behavior.
- Client tunnels may use ephemeral router-owned destinations by default.
- Multiple client listeners may share a named client destination only through an explicit bounded `SharedClientGroup`; sharing may never be implicit.
- A `Dedicated` policy creates one destination/pool per service.
- Identity rotation is never a side effect of reload.

### 3.4 Destination resolution

M10 does not implement a full address-book application or subscription manager.

Mandatory accepted references:

1. canonical Base32 `.b32.i2p` destination hash;
2. bounded static aliases from strict service-tunnel configuration;
3. explicitly configured full Destination material only where a generic tunnel configuration can safely carry it as public data.

Rules:

- no clearnet DNS fallback;
- no IP-literal fallback for HTTP/SOCKS destinations;
- unknown `.i2p` names fail explicitly;
- resolved destination hashes flow through existing LeaseSet lookup/routing seams;
- address-book/subscription integration remains future work unless a later corrective proves it is required for M10 closure.

## 4. Protocol/reference basis

Implement clean-room behavior from specifications and observed reference behavior. Do not copy Java I2P source.

### 4.1 I2P tunnel behavior reference

Retain the exact Java I2P reference used in M9:

```text
repository = i2p/i2p.i2p
version    = 2.13.0
commit     = 9134f808337b401e8e53c73734c81fab04280c9d
```

Primary clean-room reference paths:

```text
apps/i2ptunnel/java/src/net/i2p/i2ptunnel/I2PTunnelClient.java
apps/i2ptunnel/java/src/net/i2p/i2ptunnel/I2PTunnelServer.java
apps/i2ptunnel/java/src/net/i2p/i2ptunnel/I2PTunnelHTTPClient.java
apps/i2ptunnel/java/src/net/i2p/i2ptunnel/socks/SOCKS5Server.java
apps/i2ptunnel/java/src/net/i2p/i2ptunnel/irc/IRCFilter.java
apps/i2ptunnel/java/src/net/i2p/i2ptunnel/irc/IrcInboundFilter.java
apps/i2ptunnel/java/src/net/i2p/i2ptunnel/irc/IrcOutboundFilter.java
apps/i2ptunnel/java/src/net/i2p/i2ptunnel/I2PTunnelIRCServer.java
```

Also use current official I2P I2PTunnel/SOCKS/IRC documentation for supported behavior and operational defaults. Record the source URLs/revision date in a new service-tunnel protocol dossier; copied code is prohibited.

### 4.2 HTTP

Target HTTP/1.1 proxy semantics from RFC 9110/9112:

- absolute-form requests for ordinary HTTP proxying;
- authority-form for `CONNECT host:port`;
- successful CONNECT switches to blind bidirectional forwarding after the 2xx header;
- remove `Connection`-nominated fields and known hop-by-hop fields before forwarding;
- do not emit Content-Length/Transfer-Encoding on successful CONNECT responses;
- restrict CONNECT targets to `.i2p` destinations and an explicit bounded port policy.

M10 deliberately excludes clearnet outproxy behavior.

### 4.3 SOCKS5

Target RFC 1928, only:

```text
VER = 5
method = NO AUTH (0x00)
CMD = CONNECT (0x01)
ATYP = DOMAINNAME for .i2p targets
```

Required explicit failures:

- no acceptable authentication method -> `0xff`;
- BIND / UDP ASSOCIATE -> command not supported (`0x07`);
- IPv4/IPv6 literal target -> address type/ruleset rejection;
- unresolved `.i2p` -> host unreachable (`0x04`) where appropriate;
- policy rejection -> `0x02`;
- local target refusal/stream failure mapped deterministically.

SOCKS UDP, SOCKS4/4a, username/password auth, Tor extensions, and clearnet outproxy are not M10 targets.

### 4.4 IRC

Reference current I2P IRC filtering behavior plus IRCv3 message-tags semantics.

Bounds:

- non-tag IRC message portion: maximum 512 bytes including terminator semantics;
- IRCv3 message tags: maximum 8191 bytes including leading `@` and trailing separator; client-originated tag data maximum 4094 bytes;
- do not truncate overlong input; reject/drop according to profile and account the event.

Privacy posture:

- inbound command allowlist;
- outbound command allowlist;
- `USER` hostname/servername sanitization;
- PING/PONG rewriting only where required to prevent local-address disclosure;
- CTCP/DCC filtering, with ACTION allowed and address-bearing DCC blocked by default;
- IRC server derives the projected hostname from the authenticated remote Destination, never from client-supplied hostname text.

No TLS termination is required. I2P native end-to-end encryption remains the transport privacy layer for ordinary IRC-in-I2P usage.

## 5. Configuration model

Plan 174 must introduce a strict `[service_tunnels]` configuration surface with bounded tunnel entries. Suggested typed model:

```text
ServiceTunnelSpec
  id
  enabled
  kind = generic-client | generic-server | http-client | socks5-client | irc-client | irc-server
  listener / target
  destination reference or identity reference
  destination policy = dedicated | shared-client-group(name)
  i2p source/destination port
  per-listener connection ceiling
  per-direction buffered-byte ceiling
  connect/read/write/shutdown deadlines
  profile-specific options
```

Rules:

- all client-facing TCP listeners bind loopback only in M10;
- generic server local TCP targets must be loopback; Unix-domain targets are allowed on Unix;
- no arbitrary remote administrative exposure;
- duplicate IDs or bind collisions fail before mutation;
- unknown fields fail parsing;
- all counts/lengths/deadlines have central hard ceilings.

## 6. HTTP profile boundary

The M10 HTTP proxy is intentionally smaller than Java I2PTunnel:

Mandatory:

- `.i2p`/`.b32.i2p` only;
- GET/HEAD/POST and other structurally valid HTTP methods forwarded without semantic body rewriting;
- Host normalization to the resolved I2P authority;
- CONNECT;
- hop-by-hop removal;
- privacy-header policy;
- bounded error responses;
- one request/tunnel lifecycle can force `Connection: close` initially.

Explicitly deferred unless a later plan justifies them:

- clearnet outproxies;
- proxy authentication;
- address-helper/jump-service UI;
- x-i2p-gzip compression;
- HTTP/2 or HTTP/3 proxy termination;
- response-content rewriting;
- persistent browser-to-proxy pipelining/keepalive optimization.

The HTTP proxy must interoperate with a **generic server tunnel**; no HTTP-aware server-tunnel counterpart is required.

## 7. Generic tunnel boundary

### Client tunnel

```text
loopback TCP listener
  -> fixed configured I2P destination + destination port
  -> Streaming connect
  -> shared raw byte pump
```

Multiple configured remote destinations may be supported only if selection is explicit and bounded. If implemented, use OS CSPRNG in production and deterministic injection in tests; never infer targets from payload bytes.

### Server tunnel

```text
persistent local I2P destination
  -> Streaming accept
  -> loopback TCP or Unix target
  -> shared raw byte pump
```

The peer Destination is authenticated Streaming metadata and may be projected to higher profiles such as IRC server; generic server mode does not rewrite application bytes.

## 8. Lifecycle and reload boundary

M10 owns a transactional service-tunnel reconciler, not the future Milestone 13 administrative plane.

Required lifecycle:

```text
parse/normalize all specs
 -> validate identities, aliases, binds, targets, budgets
 -> stage destination/runtime/listener resources
 -> commit complete generation
 -> drain replaced generation
```

On any staging failure, preserve the previous committed generation.

Plan 180 may expose the reconciler through tests/internal daemon capability. Do not invent a new unauthenticated remote control endpoint or broad SIGHUP/admin system merely to satisfy the word "reload" in the roadmap; Milestone 13 owns router-wide operations/reload UX.

## 9. Resource model

Central M10 hard ceilings must cover at least:

- number of configured service tunnels;
- listeners;
- active connections per listener and aggregate;
- pending Streaming connects/accepts;
- buffered local->I2P and I2P->local bytes;
- HTTP request-line/header bytes/count;
- SOCKS handshake bytes/time;
- IRC tag bytes/core-line bytes/registration lines;
- pending local target connects;
- shutdown/drain time.

No queue may use memory growth as backpressure.

## 10. Testing strategy

Each plan must use the lowest-cost evidence that proves its layer:

1. runtime-neutral parser/policy unit tests;
2. deterministic Streaming/service runtime tests;
3. real loopback TCP/Unix black-box product tests;
4. independent ordinary application clients in Plan 181;
5. independent I2P service infrastructure only where needed to satisfy the roadmap exit criterion.

After listener startup, canonical M10 product tests must not call private delivery/bridge APIs to move application bytes.

## 11. External interoperability and the Milestone 6 debt

Milestone 6 mixed-router destination/Streaming interoperability remains explicitly unclaimed. M10 must not hide that by calling self-composed application tests "network interoperability".

Plan 181 must distinguish:

```text
m10_local_application_product
m10_independent_local_application_clients
m10_remote_independent_i2p_service_interop
```

A valid independent local application-client test may use curl/another normal client against an i2pr client proxy and an i2pr server tunnel in the same private localhost composition. That proves the application tunnel layer, not mixed-router Streaming.

The final roadmap criterion for ordinary IRC infrastructure/browser-to-independent-eepsite requires remote independent I2P service behavior. If this is blocked solely by the retained M6 mixed-router debt:

- retain passed M10 local/application-client evidence;
- do not weaken the M10 exit criterion;
- stop and register one narrow M6/M10 interop corrective using the simplest available ordinary reference-router lane;
- do not rebuild the historical rootless/VM harness stack.

## 12. Plan sequence

### Plan 174 — foundation/shared Streaming runtime

Create the crate/config/types, extract the generic bounded daemon Streaming endpoint/pump, and requalify SAM without adding service listeners.

### Plan 175 — generic client/server tunnels

Add persistent server destination storage, generic client/server products, TCP/Unix target support, and restart-stable destination evidence.

### Plan 176 — HTTP proxy

Add bounded HTTP/1.1 proxy parsing/rewriting, `.i2p` resolution policy, privacy/hop-by-hop filtering, and CONNECT.

### Plan 177 — SOCKS5

Add bounded RFC 1928 no-auth CONNECT state machine and real proxy composition; reject all non-M10 commands/address families.

### Plan 178 — IRC client

Add bounded IRC/IRCv3 parsing, allowlists, privacy rewriting, CTCP/DCC policy, and client tunnel composition.

### Plan 179 — IRC server

Add bounded registration interception and authenticated peer-Destination hostname projection over the generic server product.

### Plan 180 — full local product/reconcile/hardening

Compose all configured services under daemon supervision; transactional generation replacement; adversarial/backpressure/restart/shutdown matrix; canonical self-composed product tests.

### Plan 181 — independent acceptance/final closure

Fail-closed external runner/checker/manual workflow; standard clients; remote ordinary I2P evidence when feasible; exact final support ledger and closure.

## 13. Cross-plan non-goals

Do not introduce during M10:

- SOCKS UDP ASSOCIATE;
- SOCKS4/4a;
- clearnet HTTP/SOCKS outproxy;
- transparent proxying;
- arbitrary non-loopback client listeners;
- arbitrary non-loopback server targets;
- HTTP/2/3 proxy protocol stacks;
- TLS interception/termination;
- full address-book/subscription application;
- router-wide admin/reload API;
- transit-tunnel or floodfill work;
- public-network stress/fuzz/malformed traffic;
- a second destination/Streaming/routing implementation.

## 14. Full validation floor

Every implementation plan must preserve at least:

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-i2cp-vectors.sh
bash scripts/check-sam-acceptance-evidence.sh
bash scripts/check-ssu2-acceptance-evidence.sh
cargo deny check advisories bans sources
```

Plans touching shared SAM runtime must additionally run Plan 151/152 focused regressions. Plans touching destination identity/storage must run Plan 120/166/172 focused regressions.

## 15. Milestone 10 final acceptance

M10 may close only when all of the following are true:

1. Plans 174–180 have explicit passed status records.
2. `i2pr-service-tunnels` is runtime-neutral and dependency-boundary checks pass.
3. Generic client and server tunnels move bidirectional streams through the production destination/Streaming path.
4. Server destination identity is stable across restart and secret handling meets project storage guardrails.
5. HTTP proxy reaches an I2P server tunnel with a standard HTTP client; CONNECT works and clearnet targets are rejected.
6. SOCKS5 CONNECT works with a standard SOCKS5-capable client; unsupported methods/commands are rejected correctly.
7. IRC client filtering/rewriting is bounded and privacy-relevant leak vectors have negative tests.
8. IRC server registration derives hostname from authenticated peer Destination rather than untrusted USER data.
9. Per-listener/per-connection/aggregate resource ceilings are proven under saturation and slow-peer conditions.
10. Startup/reconcile/shutdown are transactional and repeated cycles return resource baselines.
11. Independent ordinary application-client evidence is command-derived and fail-closed.
12. At least one ordinary remote independent I2P service trajectory closes the roadmap interoperability requirement, or M10 remains explicitly open with a registered corrective rather than silently weakening the criterion.
13. Current-head routine CI is green.
14. The dedicated external workflow is green on the exact final head, preferably twice.
15. README/support/conformance/architecture docs state the same precise supported and unsupported profile.
16. No claim implies clearnet outproxy, SOCKS UDP, remote admin exposure, HTTP/2/3 proxying, or full M6 mixed-router closure unless separately proven.

## 16. Handoff

After this roadmap is registered:

```text
plan_173 = registered-m10-service-tunnels-roadmap
milestone10_planning_authority = plan173
milestone10_final_acceptance = not-yet-closed
next_executable_plan = 174
next_product_layer = milestone10-service-tunnels
```

Execute Plans 174–181 in order. A later plan may insert a narrow corrective only when an executable test/reference comparison proves the need.