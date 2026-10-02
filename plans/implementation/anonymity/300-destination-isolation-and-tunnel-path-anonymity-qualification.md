# Plan 300 — Destination isolation and tunnel-path anonymity qualification

Status: stopped-target-isolation-and-reference-diversity-owner-gap

Classification: invariant + capability + evidence.

Hard dependencies: Plan 296 closed. Existing destination, exploratory/client tunnel, NetDB provenance, and service-tunnel composition authorities remain prerequisites. Independent of M12 floodfill progression.

## Objective

Turn existing Destination separation and tunneled client behavior into an explicit anonymity contract, close target-identity reuse and path-diversity gaps, and qualify those properties with deterministic and composed tests.

## Why ready after Plan 296

The required owners already exist: service Destination policy, client tunnel pools/build requests, NetDB namespace/provenance types, and router-backed service composition. This does not require floodfill-server capability.

## Current implementation evidence

Positive foundations:
- service tunnels default to `DestinationPolicy::Dedicated`;
- sharing requires explicit `SharedClientGroup`;
- client tunnel identities are ephemeral unless intentionally persisted for server services;
- client Destination identity shape is compatible with deployed I2P families;
- NetDB distinguishes main-router and client namespaces/provenance;
- destination lookup/publication requires real non-zero-hop tunnel material rather than local zero-hop delivery.

Open anonymity questions:
- whether any general-purpose listener can reach multiple unrelated remote Destinations under one client identity;
- whether path selection mechanically forbids repeated routers and enforces family/network diversity when sufficient candidates exist;
- whether inbound/outbound endpoint reuse creates avoidable correlation;
- whether service-Destination tunnel length/profile should converge on the common deployed client-tunnel profile rather than the current shorter experimental default;
- how insufficient candidate diversity degrades without silently violating the qualified profile.

## Invariants

1. Main-router identity and client Destination identity remain cryptographically/operationally separate.
2. Client NetDB requests/publications remain tunnel-routed; no direct shortcut.
3. Client namespaces never fall back to main-router LeaseSet authority.
4. Dedicated service identities remain default.
5. No router appears twice within one qualified tunnel path.
6. Diversity fallback is explicit and cannot be counted as qualified after violating a required floor.
7. Private keys, peer identities, and path membership are not emitted into ordinary logs/evidence.

## Scope

### In

Per-service target-fanout audit; target/first-party client-Destination isolation where one listener can dial unrelated Destinations; warnings/qualification exclusion for SharedClientGroup; path candidate exclusion/diversity policy; inbound/outbound endpoint reuse policy; service-Destination tunnel length/profile convergence based on pinned reference behavior; deterministic path-policy tests; composed tests proving client lookup/publication remains tunnel-routed and namespace-separated.

### Out

Global passive-adversary simulation, arbitrary latency padding, onion-routing redesign, floodfill-server selection/advertisement, transport-level IP obfuscation beyond I2P, hostile browser/app fingerprint claims.

## Required work

### A. Freeze current target-fanout ownership

For every service kind, document whether one local listener is fixed to one remote Destination or may dial multiple arbitrary Destinations. Test actual runtime composition, not only config enums.

For any multi-target client service, prove whether one client Destination identity is reused across targets. If yes, implement bounded target/first-party isolation for the qualified profile so unrelated remote services cannot trivially correlate the same client Destination. Cache/lifecycle rules are bounded and cancellable.

### B. Path-diversity contract

Research/freeze current Java/i2pd peer-selection constraints before choosing exact family/IP-prefix rules. At minimum the qualified policy mechanically prohibits duplicate routers within one path.

Where reference-compatible metadata is available, add family/network diversity and inbound/outbound endpoint reuse exclusions. Do not invent arbitrary prefix lengths without documented/reference justification.

Represent candidate rejection with typed deterministic-test reasons.

### C. Insufficient-diversity semantics

Define a minimum qualified floor and explicit degraded/unqualified outcome when the candidate population cannot satisfy it. Availability fallback may exist for experimental/operator modes, but cannot count as anonymity-qualified.

Never spin/retry unboundedly searching for nonexistent diversity.

### D. Service client tunnel profile

Compare current service-Destination hop count/quantity/variance against pinned Java/i2pd/current I2P client-tunnel defaults. If current fixed profile is outside selected compatibility target, introduce a named qualified service profile and migrate default only with deterministic build/runtime tests.

Keep test-only short-hop profiles explicit; they cannot accidentally become qualified production default.

### E. Composed unlinkability regression suite

Prove:
- main RouterInfo identity is not service Destination identity;
- dedicated services do not share client Destination identity;
- explicit shared group does share and is labeled linkable/unqualified;
- client LeaseSet lookup/publication uses client tunnel routes and does not become direct router traffic;
- client namespace records cannot be served as main-router authority;
- path duplicates/diversity violations are rejected;
- restart preserves only identities intentionally persistent.

## Ordered work packages

1. Inventory service target fanout/identity reuse.
2. Freeze reference diversity/tunnel-profile rules.
3. Introduce pure path/isolation contracts with deterministic tests.
4. Wire policy into existing owners without parallel builder.
5. Implement target isolation where required.
6. Add composed namespace/lookup/publication regressions.
7. Run local/reference qualification where applicable.
8. Update docs/evidence.

## Failure/cancellation/restart/contention

Target-isolation caches/pools are bounded by service/global budgets, have explicit eviction/teardown, and spawn no ownerless tasks. Path selection is bounded over a finite candidate snapshot.

Insufficient qualified candidates return typed degraded/unavailable outcome; no busy loop. Restart preserves server identities only where existing persistence semantics require; ephemeral client isolation identities remain ephemeral.

## Compatibility and migration

Changing default hop profile or target identity reuse may affect performance/remote continuity and is documented as a privacy/security migration. Explicit shared groups remain available but excluded from qualified profile. No server Destination private-key migration.

## Required tests

- per-service target fanout classification;
- dedicated vs shared identity behavior;
- arbitrary-target isolation if applicable;
- duplicate-router path rejection;
- reference-backed family/network/endpoint diversity;
- insufficient-candidate termination/no retry storm;
- qualified hop/quantity/variance profile;
- client lookup/publication non-zero-hop/tunnel-routed;
- client namespace cannot answer main-router server queries;
- restart/cancel/resource release;
- no path/identity details in ordinary diagnostics.

## Exact verification commands

~~~text
cargo fmt --all --check
cargo test --locked -p i2pr-service-tunnels --all-targets -- --test-threads=1
cargo test --locked -p i2pr-client --all-targets -- --test-threads=1
cargo test --locked -p i2pr-tunnel --all-targets -- --test-threads=1
cargo test --locked -p i2pr-netdb --all-targets -- --test-threads=1
cargo test --locked -p i2pr-daemon --all-targets -- --test-threads=1
bash scripts/check-service-anonymity-boundaries.sh
bash scripts/check-destination-tunnel-evidence.sh
bash scripts/check-netdb-tunnel-evidence.sh
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
~~~

If a new path-anonymity checker is introduced, closure records/runs it.

## Documentation updates

Document dedicated/shared correlation semantics, target isolation, qualified/degraded path policy, and selected service tunnel profile. Keep global security-model non-claim until Plan 301.

## Acceptance criteria

Plan 300 passes only if:
1. every service kind has tested target-fanout/identity-reuse classification;
2. multi-target services cannot reuse one qualified client identity across unrelated targets without explicit unqualified sharing;
3. duplicate-router paths are mechanically impossible in qualified policy;
4. extra diversity rules are reference/documentation-backed and deterministic;
5. insufficient diversity terminates boundedly and cannot be mislabeled qualified;
6. qualified service tunnel profile is reference-derived;
7. client NetDB/namespace separation regressions stay green;
8. no parallel tunnel/NetDB/Destination owner is created.

## Stop conditions

Stop if exact diversity rules cannot be justified from protocol/reference behavior; target isolation requires changing server identity persistence; or current builder cannot express policy without an architectural owner change requiring separate ADR.

## Closure evidence required

Retain target-fanout matrix, reference diversity/profile pins, deterministic policy fixtures, composed identity/NetDB route assertions, resource/cancellation results, external evidence where used, and routine floor.

## Handoff notes

May execute in parallel with Plans 297/298 after Plan 296. Plan 301 requires closure, but no mainline milestone depends on it.
