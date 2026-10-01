# Plan 277 — M12 daemon floodfill composition, role lifecycle, and controlled advertisement

Status at registration:
**registered-m12-daemon-role-blocked-on-plan276**

Classification: capability integration.

Hard dependency: Plan 276 passed.

## 1. Objective

Compose the completed floodfill core into the daemon with bounded supervised queues, authenticated
inbound provenance, direct SSU2 replication, lookup reply delivery, persistence/maintenance
ownership, health-driven role lifecycle, and truthful controlled RouterInfo caps=f advertisement.

Normal product floodfill configuration remains unavailable until Plan 279.

## 2. Role model

Implement an explicit state machine equivalent to:

~~~text
Disabled -> Eligible -> Activating -> Active -> Draining -> Disabled
                         |             |
                         +-> Failed <--+
~~~

Activation requires a typed FloodfillEligibilitySnapshot, not a configuration boolean. At minimum
the snapshot covers controlled operator/test intent, qualified SSU2 address, direct reachability,
NetDB readiness, store/queue headroom, clock sanity, runtime health, and any ADR-frozen
bandwidth/uptime prerequisites.

## 3. Invariants

- caps=f is impossible without Active readiness.
- Health/resource loss first withdraws advertisement, then stops new floodfill admission, then
  drains bounded in-flight effects.
- Default normal daemon remains floodfill-disabled.
- Plan 277 uses an internal/test qualification permit, not a normal user-facing enable toggle.
- Flood replication uses direct SSU2 only; no exploratory/client tunnel fallback.
- Lookup replies honor requested direct/tunnel destination and ECIES protection.
- Authenticated peer identity and inbound delivery context are preserved into Plan 271 provenance.
- All channels are bounded by count and bytes; no per-request detached task.
- Shutdown/cancellation returns queues, permits, secrets, and service state to baseline.
- Local RouterInfo publication composes only qualified SSU2 publication snapshots and truthful
  capability/version fields.

## 4. Required production changes

A. Daemon FloodfillCoordinator owning FloodfillService, stores, persistence handle, maintenance
tick, resource permits, and bounded effect queues.

B. Central I2NP dispatcher routes DatabaseStore/DatabaseLookup to the coordinator only when the
controlled role is active; normal client NetDB response paths remain distinct.

C. Outbound effect adapter:
- DeliveryStatus / direct replies over authenticated SSU2 as appropriate;
- reply-tunnel responses through existing router/tunnel delivery seams;
- DirectFloodAction over SSU2 only, with bounded connect/send concurrency and deadline;
- no tunnel fallback for flood replication.

D. Local RouterInfo composition:
- consume validated SSU2 publication snapshot;
- add caps=f only from active FloodfillAdvertisementPermit;
- preserve netId/capability/version policy from ADR 0027;
- remove the current generic caps=f rejection only behind typed authority, not arbitrary Mapping;
- republish/withdraw on state transitions with debounce/rate bounds.

E. Privacy-safe health/status counters.

## 5. Scope / non-goals

No normal user config to enable floodfill, no public network participation, no two-family claim,
no Java/i2pd source patching. Qualification permit is available only to controlled test/external
lanes.

## 6. Work packages

1. Coordinator ownership and bounded channels.
2. Inbound provenance threading.
3. Effect dispatch for acks/replies/floods.
4. Maintenance/persistence supervision.
5. Eligibility and role transitions.
6. Canonical local RouterInfo/SSU2 address composition.
7. Controlled caps=f advertisement/withdrawal.
8. cancellation/restart/queue-pressure integration tests.
9. static checks that normal config cannot activate the permit.

## 7. Failure / cancellation / restart / contention

Essential coordinator failure withdraws caps=f and deactivates floodfill; router may continue in
non-floodfill degraded mode when safe. Queue saturation rejects/shears floodfill work before
router-owned/client work.

Shutdown: publish/queue withdrawal when possible, stop admission, cancel pending direct dials,
drain bounded effects to deadline, persist allowed state, release permits, then terminate. Forced
shutdown skips graceful network notification but next restart starts without caps=f until fresh
eligibility.

## 8. Compatibility and migration

No normal config schema addition yet. Local RouterInfo builder changes must preserve zero/disabled
behavior for all ordinary profiles. Controlled test profile must be impossible to activate via
untrusted network input.

## 9. Required tests

- role transition matrix;
- config alone cannot activate;
- eligibility false -> no f;
- active -> f plus qualified SSU2 address;
- health loss -> f withdrawal before new admission;
- direct flood never routes through tunnel;
- lookup direct/reply-tunnel effects;
- queue count/byte saturation;
- coordinator cancellation and restart baseline;
- persistence recovery;
- authenticated peer provenance preserved;
- router.version/caps/netId truthfulness;
- normal daemon regression remains non-advertised.

## 10. Exact verification

~~~bash
cargo fmt --all --check
cargo test --locked -p i2pr-daemon --all-targets
cargo test --locked -p i2pr-netdb --all-targets
cargo test --locked -p i2pr-netdb-persist --all-targets
cargo test --locked -p i2pr-transport-ssu2 --all-targets
cargo clippy --locked -p i2pr-daemon -p i2pr-netdb --all-targets -- -D warnings
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-m12-floodfill-boundaries.sh
~~~

## 11. Documentation

Document the controlled qualification profile, role state machine, health inputs, shutdown order,
and explicit absence of normal floodfill configuration.

## 12. Acceptance criteria

- Full local daemon path can act as a floodfill under controlled permit.
- Normal daemon remains default-off and cannot broadly advertise f.
- Health withdrawal and resource saturation are fail-closed.
- All queues/tasks are owned/bounded.
- Direct-only replication is preserved.
- Exact local RouterInfo advertisement is truthful.
- No critical/high finding remains.

## 13. Stop conditions

Stop if qualified SSU2 address publication cannot be composed without broad unrelated transport
activation, if role withdrawal cannot precede ongoing admission, or if daemon provenance loses
authenticated source/delivery context.

## 14. Closure evidence

Role/state matrix, queue budgets, shutdown traces, RouterInfo before/active/withdraw bytes,
commands, source-boundary audit, and unblock audit. On pass, move Plan 278 to ready.

## 15. Handoff

Plan 278 exercises this exact controlled daemon path against unmodified exact-pinned i2pd.
