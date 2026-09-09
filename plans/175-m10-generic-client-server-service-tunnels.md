# Plan 175 — Milestone 10 generic client/server service tunnels

Status: **blocked until Plan 174 passes**.

## 1. Goal

Build the first complete M10 application service product on the Plan 174 foundation:

```text
generic client:
loopback TCP -> I2P Streaming -> configured remote destination

generic server:
local I2P destination -> I2P Streaming -> loopback TCP / Unix target
```

This plan also closes the missing persistence primitive for stable router-owned service destinations.

No HTTP/SOCKS/IRC parsing belongs here.

## 2. Required architecture

Reuse:

- `i2pr-client::DestinationRuntime`;
- `i2pr-client::streaming::StreamingManager`;
- `StreamingDestinationAdapter`;
- Plan 174 shared daemon Streaming runtime/pump;
- existing destination LeaseSet2/tunnel lifecycle.

Do not create service-specific Garlic, I2NP, LeaseSet, Streaming, or tunnel implementations.

## 3. Persistent router-owned service destination

### 3.1 Storage format

Add a versioned destination-identity persistence type in `i2pr-storage` or the smallest appropriate storage layer.

Stored secret material must be sufficient to reconstruct exactly one `DestinationIdentity`:

- Ed25519 signing seed/private material;
- X25519 static inbound secret;
- exact destination padding/public material needed for stable Destination bytes.

Requirements:

- explicit format version/magic;
- strict exact-length decode and complete consumption;
- atomic create-new semantics;
- no overwrite of an existing identity during ordinary startup;
- permission hardening consistent with router identity storage on Unix;
- secret-bearing decode types non-`Clone`, redacted `Debug`, zeroize on drop where practical;
- corruption/truncation/wrong-version fails closed;
- no private material in TOML, logs, errors, metrics, status snapshots, or evidence artifacts.

Add a storage-backed constructor/reconstruction seam to `DestinationIdentity` only if needed; keep raw secret access narrowly scoped to storage integration. Do not add unrestricted secret getters.

### 3.2 Identity lifecycle

For a configured server service identity reference:

```text
missing key file -> generate once with OS CSPRNG -> atomic persist -> load/verify -> activate
existing key file -> load/verify -> activate same Destination
corrupt key file -> fail service generation; never silently rotate
```

A reload/reconcile must never generate a new identity merely because a component restart occurred.

Tests must prove identical Destination hash/Base32 across restart/reload.

## 4. Generic client tunnel

Implement a daemon service listener per validated enabled generic-client spec.

Lifecycle:

1. bind configured loopback TCP address;
2. acquire service/listener and aggregate resource lease;
3. accept local connection under ceiling;
4. resolve configured `DestinationRef` through the M10 resolver and existing destination/LeaseSet routing path;
5. select/create the configured router-owned client destination (`Dedicated` or explicit shared client group);
6. open a Streaming connection to the configured I2P destination port;
7. wait for actual `ConnectionState::Established` under connect deadline;
8. transfer sole socket ownership to the shared Plan 174 byte pump;
9. on local EOF/error/cancel/remote close, close/reset Streaming as appropriate and release every lease.

A configured client tunnel is fixed-target. Do not inspect application bytes to choose the remote destination.

### 4.1 Target list option

If the plan implementation supports multiple fixed target destinations for one client tunnel, it must be explicit configuration and bounded. Selection must be deterministic in tests and use OS CSPRNG or a documented non-identifying policy in production. Failure of one target may try another only under a named small attempt ceiling; never loop indefinitely.

A single target is sufficient for Plan 175 acceptance.

## 5. Generic server tunnel

One enabled generic-server spec owns a stable router-owned service Destination and its destination-specific runtime/pool.

Lifecycle:

1. load/create stable destination identity transactionally;
2. build/register the destination product and Streaming listener on configured I2P destination port;
3. only report service ready once the destination is usable under the same local product semantics as existing SAM router-owned destinations;
4. on inbound Streaming connection, obtain authenticated peer Destination metadata from Streaming;
5. connect to the configured local target under a bounded deadline;
6. if target connection succeeds, run the shared byte pump;
7. if target connection fails/refuses/times out, close/reset the I2P stream without leaking a task or queue;
8. destroy/reconcile/shutdown releases destination runtime, Streaming manager, local target sockets, and tasks.

Generic server mode forwards bytes unchanged. Peer Destination metadata is not prepended or injected into generic application data.

## 6. Local target policy

### 6.1 TCP

Mandatory:

- IPv4/IPv6 loopback only;
- port must be nonzero;
- connect timeout bounded;
- no DNS hostname target in M10 generic server core;
- no wildcard or arbitrary LAN/WAN target.

### 6.2 Unix domain socket

On Unix, support a configured Unix-domain stream target if Tokio/runtime features permit without a new unsafe dependency.

Requirements:

- path length ceiling;
- no NUL;
- normalize only lexically; do not resolve symlinks as a security promise;
- failures typed and bounded;
- platform-gated tests.

On non-Unix platforms the Unix target must be explicitly unsupported during semantic validation, not silently ignored.

## 7. Service manager / ownership

Add a daemon M10 service manager/repository that owns:

- committed service specs;
- listener cancellation capabilities;
- service destination handles;
- shared client-group destinations;
- active connection permits;
- sanitized snapshots/counters.

It must not expose raw `DestinationIdentity` secrets.

Suggested snapshot fields:

```text
configured_services
ready_services
active_client_connections
active_server_connections
active_service_destinations
pending_connects
buffered_bytes_accounted
failed_connects_total (bounded counter, not labels per peer)
```

## 8. Startup transaction

Before any enabled service mutates runtime state:

- validate all specs;
- detect local listener bind collisions;
- validate identity references/target platform support;
- validate aggregate budgets;
- stage required persistent identities.

If one service cannot stage, the Plan 175 initial startup transaction fails rather than leaving an unexplained partial generation. Plan 180 will generalize this to reconcile/reload.

## 9. Canonical self-composed product test

Add a black-box test, e.g.:

```text
crates/i2pr-daemon/tests/service_tunnel_generic_product.rs
```

Required topology after daemon/service-manager startup:

```text
local TCP client
  -> generic client listener A
  -> Destination A / Streaming
  -> Destination B / generic server
  -> loopback echo or scripted TCP target
```

Drive bytes only through OS TCP sockets after startup.

Prove:

- small payload request/response exact digest;
- payload large enough to require multiple Streaming segments;
- simultaneous bidirectional bytes;
- half-close/EOF behavior;
- two sibling client connections isolated;
- target refusal yields bounded failure and no leak;
- one closed sibling does not tear down the other;
- post-test counters/resources return baseline.

No private bridge/delivery injection after the service listener is ready.

Add a Unix-target companion on Unix.

## 10. Restart-stable identity acceptance

A dedicated test must:

1. start server service with absent identity file in a temporary data dir;
2. record only the public Destination hash/Base32;
3. shut down cleanly;
4. create a fresh service manager/process-level composition using the same data dir;
5. assert the exact same public Destination identity;
6. corrupt/truncate a copy and prove startup fails instead of rotating.

Do not persist test keys into repository fixtures.

## 11. Adversarial/resource tests

Include:

- local accept flood at per-listener ceiling;
- aggregate ceiling shared across listeners;
- stalled local reader and writer;
- remote Streaming connect timeout;
- unknown/unresolved destination;
- stale/expired LeaseSet failure;
- local server target refused;
- local target stalls after accept;
- cancellation while connecting;
- cancellation while backpressured;
- abrupt local TCP reset;
- remote reset/close;
- repeated start/shutdown cycles;
- duplicate bind/config rejected before activation;
- corrupt identity file;
- destination runtime startup failure rolls back service state.

## 12. Security constraints

- All local client listeners remain loopback-only.
- All TCP server targets remain loopback-only.
- Do not add a generic remote TCP forwarder to clearnet/LAN.
- No service identity secret appears in `Debug`.
- No public-network traffic is required for this plan.
- No SAM or I2CP adapter is used as an internal shortcut; M10 composes directly over the shared destination/Streaming capability.

## 13. Documentation/support

Update at passing closure:

- `specs/protocols/11-service-tunnels.md` with generic client/server profile;
- `docs/architecture/i2pr-service-tunnels.md`;
- storage architecture docs for persistent destination identity format;
- sample disabled config entries;
- `specs/support.toml`: generic client/server local product rows as experimental/non-advertised/local evidence only;
- `plans/175-status.md`.

## 14. Validation floor

At minimum:

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-storage --all-targets
cargo test --locked -p i2pr-client --all-targets
cargo test --locked -p i2pr-service-tunnels --all-targets
cargo test --locked -p i2pr-daemon --test service_tunnel_generic_product -- --test-threads=1
cargo test --locked -p i2pr-daemon --test sam_stream_self_composed -- --test-threads=1
cargo test --locked -p i2pr-daemon --test sam_stream_final_acceptance -- --test-threads=1
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-sam-acceptance-evidence.sh
bash scripts/check-i2cp-acceptance-evidence.sh
cargo deny check advisories bans sources
```

## 15. Acceptance criteria

Plan 175 passes only when:

1. stable router-owned service destination persistence is versioned, atomic, secret-safe, and corruption-tested;
2. a server service restarts with the exact same Destination;
3. generic client listener is loopback-only and fixed-target;
4. generic server target is loopback TCP or platform-supported Unix socket only;
5. generic client/server bytes traverse the existing production destination/Streaming path;
6. small/large/bidirectional/sibling/half-close product tests pass using only local OS sockets after startup;
7. target refusal/timeouts/backpressure/cancellation are bounded and clean;
8. per-listener and aggregate connection ceilings are enforced;
9. no service-specific routing/Streaming stack exists;
10. SAM/M6/M9 regressions remain green;
11. full workspace floor and exact-head routine CI pass;
12. `plans/175-status.md` advances `next_executable_plan = 176`.

## 16. Stop conditions

Stop for a narrow corrective if:

- stable identity persistence requires exposing unrestricted private key access;
- the generic product only works through private test delivery injection;
- local server targets cannot be restricted to loopback/Unix without breaking the product architecture;
- a real M6 Streaming defect is exposed;
- SAM regressions require reverting Plan 174 shared-runtime invariants.

## 17. Handoff

Expected transition:

```text
plan_175 = passed-m10-generic-client-server-service-tunnels
milestone10_generic_tunnels = passed-via-plan175
milestone10_final_acceptance = not-yet-closed
next_executable_plan = 176
```