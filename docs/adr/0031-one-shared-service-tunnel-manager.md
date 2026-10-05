# ADR 0031: One service-tunnel manager, constructed once in the composition root

- Status: Accepted
- Date: 2026-10-05
- Decision owner: repository maintainer (owner-authorized Plan 337)
- Related: ADR 0028, Plans 289, 323, 334, 337, 338

## Context

Plan 289 stated as a requirement that the I2PControl control plane "must not create a second
service/destination/tunnel runtime", and that it closes over "**one** M10 `ServiceTunnelManager`". The
`i2pcontrol_tunnels` module header repeated the claim. The source matched neither.

There were two `ServiceTunnelManager` instances:

| Instance | Built by | Specs | Router delivery backend |
|---|---|---|---|
| Control-owned | `TunnelControlState::for_config` | an empty `ServiceTunnelSet`, then reconciled with control-owned specs | never installed |
| Product | `ServiceProduct::new` / `start_over_existing_daemon` | `config.service_tunnels.tunnels` | installed, with the executable `RemoteDestinationBackend` |

`publish_service_ls2_for_service` lived only in `service_product.rs` and was only ever handed the
product manager. No production call site installed a delivery capability on the control-owned manager
(the free-function `install_router_delivery_handle` is referenced only from integration tests).

The consequence was not an ELS2 problem. A service tunnel created through TunnelManager was off the
publication path entirely: it published no LeaseSet2, encrypted or ordinary, and without the backend
every non-co-owned destination resolved as `RemoteUnresolved`, so its runtimes could not deliver over
the network either.

The defect was invisible to Plan 289's verification because every one of its rows inspects the control
state — the mirror and the durable store generation — and both of those were correct. Nothing observed
whether a control-created runtime was reachable from outside the control state.

## Decision

**The composition root constructs the one `ServiceTunnelManager` and injects the same `Arc` into both
owners.**

`i2pr_daemon::build_shared_service_manager` is the single production construction. It returns the
manager, or `None` for a profile that can never own a service runtime. The I2PControl control state and
the destination-group product both receive that instance.

Three consequences are load-bearing and were decided here rather than discovered later:

1. **`reconcile` is a whole-set transactional replace.** A control transaction must therefore reconcile
   a candidate describing the *entire* runtime surface — startup-owned specs carried through verbatim,
   plus control-owned running specs. A control-only candidate silently removes every startup-owned
   runtime on the first control transaction.

2. **The manager is shared, so ownership checks must be scoped.** `verify_agreement` cannot treat a
   startup-owned runtime as an "extra" runtime, and `shutdown` cannot tear the manager down; it
   reconciles back to the startup-only set instead.

3. **Order must be declared, not inherited.** The service graph's startup order is a topological sort
   whose ready set is a `BTreeSet` popped in lexical order, so `i2pcontrol` sorted *before*
   `ssu2-router`. The control state reconciles onto the shared manager, so it must run after the product
   has prepared that manager and installed the executable delivery backend. The I2PControl service
   declares `depends_on("ssu2-router")`, conditional on SSU2 being registered.

The destination-group product's activation gate also widened: it comes up whenever a service tunnel can
exist at all, including when the only way to create one is I2PControl. An operator adds the *first*
service tunnel through TunnelManager, so requiring a configured one made the control plane useless for
its purpose. I2PControl is disabled by default, so an ordinary profile is unaffected.

`ServiceProductSpec` gains `shared_manager`. `None` keeps the private build the controlled helper and
the integration lanes want; only production composition injects the shared instance.

### Rejected alternatives

- **Route control-owned definitions into the product manager through a bridge.** Rejected: it preserves
  two manager objects and therefore two runtimes, which is the thing Plan 289 forbade. The bridge would
  also have to forward every accessor, which is a wider surface than one shared `Arc`.
- **Let the product hand its manager to the control state through a runtime slot.** Rejected: the control
  state is constructed at *registration* time while the product is constructed at *service start* time,
  so a slot would be empty exactly when the control state needs it, and a create during that window
  would have to fail or silently queue.
- **Leave two managers and accept that a control-created server does not publish.** Rejected: that is
  the defect.

## Consequences

- Plan 289's stated invariant now holds in the source. The module header's Plan 334 KNOWN DRIFT banner
  is replaced by this description; the invariant text in Plan 289 is left as written, because making the
  source match the plan is the correct direction of correction.
- Plan 297's TLS policy install moved to the composition root, where its own doc comment always claimed
  it lived. While each owner built a private manager, only the control-owned one saw the policy, so
  startup-owned `use_ssl` services silently ran without one.
- `ServiceTunnelManager` gained a per-spec encrypted-LeaseSet2 material registry. Entries are `Arc`
  handles to a single material per service: the type-5 identity is derived from the service's persisted
  Ed25519 seed, so an `Arc` clone is another handle to *that* identity, never a second copy of it.
- Two further defects became reachable and were owned by **Plan 338**, which passed: the ELS2 material
  loader read `for_service` while the runtime writes `for_group` (three store paths exist for one
  concept, and the loader had duplicated the resolution instead of asking the owner), and the
  coordinator's `rollback_state` did not reconcile the manager (so a failed transaction could leave a
  runtime with no durable definition). Plan 338 made `ServiceTunnelManager` the single owner of the
  identity store resolution and made the rollback reconcile. See
  `plans/closure/i2pcontrol-proposal-170/337-status.md` for the original, partly incorrect diagnosis and
  its dated correction.
