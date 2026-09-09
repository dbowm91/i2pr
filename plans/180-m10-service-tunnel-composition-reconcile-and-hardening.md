# Plan 180 — Milestone 10 service-tunnel composition, reconcile, and hardening

Status: **blocked until Plan 179 passes**.

## 1. Goal

Turn the individually proven M10 products into one coherent daemon-owned service-tunnel subsystem with transactional generation replacement, bounded draining, shared resource accounting, and a complete local-product adversarial matrix.

Required result:

```text
validated ServiceTunnelSet
  -> stage complete generation
  -> atomically commit listeners/destinations/runtime handles
  -> drain replaced generation under deadline
  -> preserve unaffected services
  -> full generic/HTTP/SOCKS/IRC local product remains bounded
```

This is the final local product/hardening pass before independent acceptance. It does not broaden I2P router interoperability claims.

## 2. Architectural constraints

Retain the Plan 173 architecture:

- `i2pr-service-tunnels` owns runtime-neutral config/protocol/policy only;
- `i2pr-daemon` owns all sockets, tasks, timers, persistent service orchestration, and generation state;
- `i2pr-client` remains the only destination/Streaming/routing implementation;
- server identities use the Plan 175 storage path;
- service-specific parsers may not construct Garlic/I2NP/tunnel traffic;
- SAM/I2CP remain separate externally exposed APIs and are not internal transport shortcuts for service tunnels.

Do not introduce a global mutable router service locator to simplify reconcile.

## 3. Service generation model

Introduce or finalize an explicit committed-generation type in the daemon, for example:

```text
ServiceTunnelGeneration {
    generation_id,
    normalized_specs,
    listener_handles,
    service_destinations,
    shared_client_groups,
    connection_admission,
    cancellation,
    sanitized_snapshot,
}
```

One generation is authoritative at a time.

The manager must be able to stage a replacement while the current generation continues serving existing connections.

## 4. Transactional reconcile

Expose an internal daemon capability such as:

```text
ServiceTunnelManager::reconcile(next: ServiceTunnelSet)
```

This is not a new remote administration API. Milestone 13 owns broad operational/reload UX.

Required algorithm:

1. parse/normalize the full candidate configuration;
2. validate all hard ceilings, aliases, bind collisions, target policies, platform constraints, and persistent identity references;
3. classify each service against the committed generation;
4. stage all newly required identities/destinations/listeners/runtime handles without disturbing the old generation;
5. if any staging step fails, tear down only staged state and keep the old generation unchanged;
6. once every candidate service is stage-ready, atomically publish the new generation;
7. stop accepting new connections on replaced/removed old listeners;
8. allow existing old-generation connections to drain under a hard deadline;
9. after deadline, cancel/reset remaining old connections and release all old handles/resources;
10. update sanitized counters only from committed state.

No partial generation may become externally visible.

## 5. Diff classification

Use typed classifications, at minimum:

```text
Unchanged
MutableInPlace
ReplaceListener
ReplaceDestination
Remove
Add
```

Define what is safe to retain versus replace.

Examples:

- unchanged service spec -> retain listener/destination where practical;
- resource-limit change that can be safely swapped for future admissions -> `MutableInPlace` only if semantics are explicit;
- listener bind/port change -> replace listener;
- target destination change -> replace client connection target/runtime binding;
- server identity path or destination policy change -> replace destination;
- protocol kind change -> full replacement;
- disabled/removed -> drain/remove.

Do not infer "mutable" merely to reduce churn. Default to replacement when ownership is ambiguous.

## 6. Stable identity and shared-client-group invariants

Reconcile must prove:

- unchanged server service never rotates identity;
- listener-only changes do not regenerate server identity;
- a failed candidate identity load never mutates the committed service;
- shared client groups are explicit, reference-counted/bounded, and released only after the last committed service stops using them;
- a service cannot accidentally switch from Dedicated to SharedClientGroup or vice versa without a deliberate replacement;
- group-name collisions and incompatible group policy fail before commit.

## 7. Listener and bind transaction

Handle the difficult same-address replacement case explicitly.

If the new generation wants the exact same bind endpoint as the old listener, choose one safe strategy and test it:

- retain the existing listener when protocol/runtime semantics are unchanged; or
- stop old acceptance at commit and transfer/rebind under a short bounded commit window if platform semantics permit.

Do not require `SO_REUSEPORT` or platform-specific unsafe socket tricks as a correctness dependency.

A candidate bind collision with another service in the same generation fails before commit.

## 8. Connection draining

Old-generation connections may continue to use the exact resources they were admitted under until:

- normal EOF/close;
- remote terminal Streaming state;
- explicit cancellation;
- generation drain deadline.

New connections after commit must use only the new generation.

Drain accounting must distinguish:

```text
active_current_generation
active_draining_generation
forced_drain_closes_total
```

Counters are bounded/sanitized and must not carry destination names or application data.

## 9. Unified resource accounting

Finalize aggregate M10 resource ceilings across all service kinds:

- configured services;
- active listeners;
- active connections aggregate and per listener;
- pending local accepts;
- pending Streaming connects/accepts;
- pending local server-target connects;
- buffered bytes local->I2P and I2P->local;
- protocol-specific retained HTTP/SOCKS/IRC parser bytes;
- active service destinations;
- shared client groups;
- draining generations;
- shutdown/drain time.

One service kind may not bypass aggregate limits by using its own private semaphore/counter hierarchy.

Admission order must avoid allocating large protocol state before obtaining the relevant connection/resource permit.

## 10. Daemon graph composition

When `[service_tunnels].enabled = true`, register exactly one optional supervised service-tunnel manager in the daemon graph.

Requirements:

- disabled-by-default configuration leaves graph/product unchanged;
- manager startup is bounded and reports failure through supervisor health semantics;
- one manager owns all configured M10 listeners rather than registering an unbounded daemon `ServiceSpec` per user entry;
- all listener/per-connection children are owned by bounded `ChildScope`/cancellation hierarchy;
- manager shutdown cancels acceptance first, drains connections, then releases destinations/identities/resources.

No service-tunnel listener may outlive the manager supervisor child.

## 11. Canonical integrated local product

Add a final local product suite, e.g.:

```text
crates/i2pr-daemon/tests/service_tunnels_final_acceptance.rs
```

One test generation should contain, where practical:

- generic client A -> generic server B;
- HTTP client proxy -> generic HTTP server tunnel;
- SOCKS5 client proxy -> generic service;
- IRC client -> IRC server -> loopback IRC fixture;
- at least two simultaneous listeners sharing the global budget;
- one stable server identity reused by an unchanged reconcile.

After startup, tests use only OS sockets/application protocols; no private delivery injection moves application bytes.

## 12. Reconcile acceptance matrix

Prove at minimum:

1. no-op reconcile retains services and public server identity;
2. add one service without disturbing existing connections;
3. remove one service while sibling service remains usable;
4. change a client target and prove only new connections use the new target;
5. change a bind port and prove old connection drains while new port accepts;
6. change server target while server Destination stays stable when identity policy is unchanged;
7. invalid new alias/config leaves old generation fully usable;
8. candidate bind collision leaves old generation usable;
9. corrupt candidate identity leaves old generation usable;
10. staged destination/runtime failure rolls back all staged resources;
11. forced drain after deadline closes only old-generation residual connections;
12. repeated reconcile cycles do not increase resource baselines.

## 13. Cross-service adversarial matrix

Run resource/adversarial cases across the subsystem, not only individual protocol suites:

- aggregate connection flood split across HTTP/SOCKS/generic/IRC;
- slowloris HTTP headers and IRC registration simultaneously;
- stalled server target plus stalled local proxy client;
- Streaming connect failure while other services remain live;
- destination/LeaseSet unavailability for one client service;
- abrupt listener client resets;
- abrupt server local-target resets;
- cancellation during protocol handshake;
- cancellation during raw pump backpressure;
- shutdown during active reconcile staging;
- shutdown during old-generation drain;
- repeated malformed protocol connections without task/buffer growth.

Failures must be isolated to the owning service/connection unless an aggregate invariant itself requires subsystem rejection.

## 14. Secret/privacy audit

Add explicit tests/static assertions that:

- persistent service private keys are not `Debug`-printed;
- service snapshots contain no private Destination material;
- HTTP URLs/headers, SOCKS target strings, IRC lines/usernames/realnames are not copied into persistent evidence/logging fields;
- authenticated peer Destination may appear only where explicitly documented as public identity (e.g. IRC server projected hostname), not in generic metrics labels;
- config errors do not echo secret bytes;
- reconcile failure diagnostics use service IDs and typed reasons, not application payloads.

## 15. Static boundary checker

Create a focused checker if the existing runtime/dependency scripts cannot express the complete M10 invariants, e.g.:

```text
scripts/check-service-tunnel-boundaries.sh
```

It should reject at least:

- Tokio/socket dependencies in `i2pr-service-tunnels`;
- service crate imports of transport/tunnel-build internals;
- service-specific Garlic/I2NP construction;
- non-loopback client listeners or server TCP targets in production config paths;
- private testkit dependencies in production;
- duplicate SAM-style raw byte pump reintroduction;
- service-tunnel manager using unbounded Tokio channels/semaphores by omission.

Prefer semantic/Cargo-metadata checks over fragile text matching where feasible.

Wire the checker into routine Linux CI after it exists.

## 16. Documentation/support normalization

Before Plan 180 closure, normalize:

- `README.md`;
- `AGENTS.md`;
- `plans/README.md`;
- `.opencode/skills/i2pr-local-dev/SKILL.md`;
- `specs/support.toml`;
- `specs/CONFORMANCE.md`;
- `specs/protocols/11-service-tunnels.md`;
- `docs/architecture/i2pr-service-tunnels.md`;
- relevant daemon/client/storage docs.

Claims at this stage must be local-product only. Do not call self-composed tests mixed-router interoperability.

## 17. Validation floor

At minimum:

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-service-tunnels --all-targets
cargo test --locked -p i2pr-storage --all-targets
cargo test --locked -p i2pr-client --all-targets
cargo test --locked -p i2pr-daemon --test service_tunnel_generic_product -- --test-threads=1
cargo test --locked -p i2pr-daemon --test service_tunnel_http_product -- --test-threads=1
cargo test --locked -p i2pr-daemon --test service_tunnel_socks5_product -- --test-threads=1
cargo test --locked -p i2pr-daemon --test service_tunnel_irc_client_product -- --test-threads=1
cargo test --locked -p i2pr-daemon --test service_tunnel_irc_server_product -- --test-threads=1
cargo test --locked -p i2pr-daemon --test service_tunnels_final_acceptance -- --test-threads=1
cargo test --locked -p i2pr-daemon --test sam_stream_final_acceptance -- --test-threads=1
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-service-tunnel-boundaries.sh
bash scripts/check-sam-acceptance-evidence.sh
bash scripts/check-i2cp-acceptance-evidence.sh
cargo deny check advisories bans sources
```

## 18. Acceptance criteria

Plan 180 passes only when:

1. all enabled M10 services compose under one supervised manager;
2. candidate generations are fully staged before commit;
3. staging failure preserves the previous committed generation;
4. unchanged service identities do not rotate;
5. old connections drain under a hard deadline while new admissions use only the committed generation;
6. aggregate ceilings cover every service kind and draining generation;
7. no-op/add/remove/target/bind/identity-failure reconcile matrix passes;
8. integrated generic/HTTP/SOCKS/IRC local product moves application bytes only through public OS listener paths after startup;
9. adversarial cross-service matrix returns every resource counter to baseline;
10. static service-tunnel boundary checker is enforced in routine CI;
11. SAM/M9 and earlier M10 focused regressions remain green;
12. documentation/support claims are normalized to local-product scope;
13. full workspace floor and exact-head routine CI pass;
14. `plans/180-status.md` records evidence and advances `next_executable_plan = 181`.

## 19. Stop conditions

Write a narrow corrective rather than weakening the gate if:

- atomic generation replacement requires a global mutable router context;
- listener replacement cannot preserve old connections without unsafe/platform-specific tricks;
- stable identities rotate across an unchanged reconcile;
- aggregate resource accounting cannot bound a protocol-specific retained buffer;
- final self-composed product needs private delivery injection after startup;
- a real M6 Streaming/destination correctness defect is exposed.

## 20. Handoff

Expected transition:

```text
plan_180 = passed-m10-service-tunnel-composition-reconcile-and-hardening
milestone10_local_product = passed-via-plan180
milestone10_final_acceptance = not-yet-closed
next_executable_plan = 181
next_product_layer = milestone10-independent-acceptance
```
