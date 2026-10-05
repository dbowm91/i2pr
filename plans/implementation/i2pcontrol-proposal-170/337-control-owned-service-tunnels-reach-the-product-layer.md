# Plan 337 — Control-owned service tunnels must reach the product layer

Status: **registered-control-owned-tunnels-off-the-publication-path**

Classification: **corrective pass** against Plan 289, on the Proposal 170 / I2PControl line.
Discovered while implementing Plan 334; Plan 334 cannot close until this lands.

Hard dependencies: Plan 289 (passed, but the invariant it states does not hold in the source).
Interface dependencies: Plans 290, 291, 292, 323 (control-side spec building), 334 (the ELS2
control surface this unblocks).

## Why this plan exists

Plan 289's plan-of-record states, as a requirement and not a description:

> It must not create a second service/destination/tunnel runtime. (line 13)
> Plan 289 closes when the seven lifecycle actions are real over **one** M10 `ServiceTunnelManager`
> for the six existing families. (line 128)

The `i2pr-control_tunnels` module header repeats it: "durable administrative ownership over **the
one existing M10 [`ServiceTunnelManager`]**. It never creates a second service/destination/tunnel
runtime."

**The source does not match.** There are two `ServiceTunnelManager` instances:

| Instance | Built by | Specs | Router delivery backend |
|---|---|---|---|
| Control-owned | `TunnelControlState::for_config` (`i2pcontrol_tunnels.rs`) | an **empty** `ServiceTunnelSet`, then reconciled with control-owned specs | **never installed** |
| Product | `ServiceProduct::new` (`service_product.rs`) | `config.service_tunnels.tunnels` (startup config) | installed, with the executable `RemoteDestinationBackend` |

`publish_service_ls2_for_service` lives only in `service_product.rs` and is only ever handed the
product manager. `install_router_delivery` is never called on the control-owned manager in
production; the free-function `install_router_delivery_handle` is referenced only from integration
tests.

### The consequence

**A service tunnel created through I2PControl is not on the publication path at all.** A
control-created *server* tunnel publishes no LeaseSet2 — encrypted or ordinary. Its runtimes cannot
deliver over the network either, because without the backend every non-co-owned destination resolves
as `RemoteUnresolved`.

This is broader than ELS2 and older than Plan 334. It is also the reason Plan 334 closed blocked:
Plan 334's ten-mode mapping is implemented, validated, persisted, and redacted, and
`i2pr-daemon::service_els2` builds a real type-5 record at the day's blinded storage key — but there
is no publication for that record to enter.

## Why Plan 289's verification missed it

Plan 289's evidence is `create`/`get`/`stop`/`start`/`delete` lifecycle assertions over the control
state, plus startup-recovery and disabled-mode isolation. Those exercise the control manager's own
durable intent and generation agreement, which is exactly the transaction coordinator's
responsibility — and that coordinator is correct. Nothing in that evidence set observes whether a
control-created runtime is *observable from outside the control state*, so a second manager instance
is invisible to it. The per-family rows added by Plans 290/291 assert `create/get/stop/start/delete`
"with exact Proposal spellings" for `httpserver` and `httpbidirserver`, which is the same shape: the
definition is real, and nothing checks the service is reachable.

Two executable rows now pin the gap so it cannot regress silently:

- `plan334_control_manager_is_separate_from_the_product_manager` — the control-owned manager holds
  no delivery capability and is not `Arc`-equal to a product-style manager, while a manager *can*
  hold a capability, so the asymmetry is in the construction rather than the type.
- `plan334_control_created_server_is_validated_but_not_published` — a control-created encrypted
  server tunnel is a real, validated definition whose posture resolves to a type-5 publisher, while
  the `ServiceTunnelSpec` it builds carries no LeaseSet publication intent at all.

## Objective

Make a control-created service tunnel the *same* runtime as a startup-configured one, so Plan 289's
stated invariant holds in the source and a control-created server publishes a LeaseSet2.

One outcome, one ownership boundary: the `ServiceTunnelManager` instance the control state
reconciles onto.

## Invariants this plan must not break

- No second service/destination/tunnel runtime. The end state is **one** manager.
- The control transaction coordinator's existing semantics are unchanged: validate-mirror-stage-
  reconcile-publish-verify, immediate-release drain, 5 s reconcile-back, reconcile-back under a hard
  deadline on publish failure. This plan changes *which* manager is reconciled, not the protocol.
- Startup-vs-control provenance, cross-class name-collision fail-closed, and the refusal to rewrite
  `router.toml` or ordinary service-tunnel files.
- No secret reaches a response, `Debug`, error, log, or the log ring.
- No new `i2pr-*` production edge; no new dependency; no `unsafe`.
- The listener, adapter, and publication surfaces stay loopback-only and the I2CP listener stays
  disabled by default and non-advertised.
- M10/M11/M12 behavior, Plan 213/214/215 application qualification, and the Plan 226/227/228
  destination-group ownership model are untouched.

## Scope

In scope:

1. **Unify the manager.** Either the control state is handed the product manager, or control-owned
   definitions are routed into the product manager, so exactly one instance owns every service
   runtime. The composition root is the right owner of that decision; `lib.rs` currently constructs
   the control state inside the I2PControl service registration and the product inside the
   destination-group/SSU2 owner registration, so this is a composition-root change.
2. **Give the unified manager its delivery backend**, so control-owned runtimes can deliver and
   publish. The backend already exists and is already installed for the product manager.
3. **Publication wiring for a control-owned server**, including floodfill selection keyed on the
   record's own storage key (the blinded key for a type-5 record, the destination hash otherwise).
4. **The `.b32.i2p` address exposure** for a control-owned encrypted service, so an operator can
   hand a client something that resolves.
5. **Regression evidence** that a control-created server is now published and reachable, replacing
   the two pinning rows with a positive assertion.

Out of scope, explicitly:

- Any ELS2 cryptographic change. Plan 332/333 own it; Plan 334 owns the mode mapping.
- LeaseSet *content* changes, client authorization policy, or the `encrypted (aes)` refusal.
- Startup-config migrations for existing definitions.
- The pre-existing `parse_configured_destination` substring defect (`priv`), which is recorded in
  Plans 333 and 334 closures and belongs to a service-tunnel parsing plan.
- The Plan 326 successor reclosure, which is Plan 335.

## Work packages

1. Decide and record the unification shape (ADR candidate: one manager, constructed once in the
   composition root, injected into both the control state and the product layer). Do not rewrite
   Plan 289's invariant text; make the source satisfy it.
2. Land the composition-root change with no behavior change for startup-owned specs.
3. Install the delivery backend on the unified manager before any control reconcile, so a control
   creation cannot land on a manager that cannot deliver.
4. Wire publication for control-owned server specs.
5. Expose the encrypted-service address through the control surface, redacted per Plan 334's rule.
6. Convert the two Plan 334 pinning rows into positive assertions and keep the negative ones.

## Failure, cancel, and restart semantics

- A control create/edit that cannot obtain a publishable runtime must fail the same transaction the
  way it does today: stage, reconcile, verify, publish, with reconcile-back on publish failure. It
  must not leave a durable definition whose runtime was never installed.
- A control delete must remove the runtime from the shared manager, which is a superset of today's
  behavior; a startup-owned sibling in the same generation must be unaffected.
- On restart, a control-owned definition must reconcile into the same shared manager, reusing the
  persisted service-destination record, and must not rotate a persistent server identity.
- Publication failure keeps the existing bounded retry policy; it must not fall back to publishing an
  ordinary LeaseSet2 for a service configured as encrypted.

## Evidence required

| Requirement | Shape |
|---|---|
| Exactly one manager owns every service runtime | An assertion over the composition root, not over one function's behaviour. |
| A control-created server publishes a LeaseSet2 at the destination hash | Positive row through the real publication path. |
| A control-created encrypted server publishes a type-5 record at the **blinded** storage key | Positive row; the Plan 334 material is consumed by the sweep. |
| The published address resolves | The `.b32.i2p` is exposed and decodes to the right flags. |
| The control transaction coordinator is unchanged | Existing Plan 289/290/291 lifecycle rows stay green unmodified. |
| Startup-owned specs are unaffected | Existing startup qualification and Plan 213/214/215 rows stay green. |
| Restart | A control-owned definition survives a restart and republishes the same service. |
| Isolation | A control delete removes only its own runtime; siblings survive. |
| Secrets | No PSK, lookup secret, DH key, or signing seed in any response, `Debug`, error, or log. |

## Acceptance

This plan passes when a service tunnel created through I2PControl publishes a real LeaseSet2 — and,
for a mode Plan 334 maps, a real type-5 record at the day's blinded storage key with the correct
`.b32.i2p` address exposed — and Plan 289's "one existing `ServiceTunnelManager`" invariant holds in
the source. At that point Plan 334's remaining scope is one call site plus the address exposure, and
Plan 334 can be reclosed.

This plan **must not** be counted as ELS2 capability. It is infrastructure plus wiring; the
encrypted-LeaseSet2 support claim stays where Plans 332/333/334 put it.

## Stop conditions

Stop and re-plan if unification would require changing the M11 transit owner, the destination-group
ownership model, or the `ServiceTunnelSet` durable format; if it would require a new `i2pr-*`
production edge; or if the composition root cannot supply one manager to both owners without
changing the Plan 213/214/215 application qualification contract.

## Docs

- `docs/architecture/i2pr-daemon.md`: replace the Plan 334 KNOWN DRIFT banner with the resolved
  ownership description, and update the `src/i2pcontrol_tunnels.rs` row.
- `docs/architecture/dependency-graph.md` only if an edge changes.
- `specs/CONFORMANCE.md`: the Proposal 170 LeaseSet mode mapping section, once Plan 334 recloses.
- `specs/support.toml`: the `control.i2pcontrol-leaseset-modes` surface, once Plan 334 recloses.

## Related

- `plans/closure/i2pcontrol-proposal-170/334-status.md` — records the discovery and the blocked
  closure.
- `plans/implementation/i2pcontrol-proposal-170/289-tunnelmanager-control-state-and-existing-service-adapter.md`
  — the plan-of-record whose stated invariant does not hold.
- `plans/implementation/i2pcontrol-proposal-170/335-encrypted-leaseset-live-interoperability.md` —
  blocked behind Plan 334, and therefore behind this.
