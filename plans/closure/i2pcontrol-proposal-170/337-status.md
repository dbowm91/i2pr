# Plan 337 status — passed: one shared service manager, so a control-owned tunnel reaches the product

- Plan: [`plans/implementation/i2pcontrol-proposal-170/337-control-owned-service-tunnels-reach-the-product-layer.md`](../../implementation/i2pcontrol-proposal-170/337-control-owned-service-tunnels-reach-the-product-layer.md)
- Status: **`passed-one-shared-service-manager-control-owned-tunnel-publishes`**
- Decision date: 2026-10-05
- Classification: corrective pass against Plan 289. This plan promotes **no** ELS2 capability
  advertisement and **no** live interoperability; it is infrastructure plus wiring, exactly as its own
  acceptance clause requires.
- Implementation commits: **`7842c93`** (unification), **`6d06c73`** (publication wiring and address
  exposure), **`82621e0`** (composition-root, restart, and secret-leak evidence)
- Plan of record it corrects:
  [`289-tunnelmanager-control-state-and-existing-service-adapter.md`](../../implementation/i2pcontrol-proposal-170/289-tunnelmanager-control-state-and-existing-service-adapter.md)
- Blocker it resolves: [`334-status.md`](334-status.md)

## What this plan decided

**The composition root constructs the one `ServiceTunnelManager` and injects the same `Arc` into both
owners.** Not a routed lookup, not a second manager with a forwarding seam: one instance, built once,
in `build_shared_service_manager`.

The alternative considered and rejected was a "control-owned definitions are routed into the product
manager" bridge, which would have preserved two manager objects and therefore two runtimes — the exact
thing Plan 289 forbade.

## Why the source was wrong

Plan 289 stated, as a requirement rather than a description, that the control plane "must not create a
second service/destination/tunnel runtime" and closes over "**one** M10 `ServiceTunnelManager`". The
`i2pcontrol_tunnels` module header repeated it. The source matched neither: two managers existed, and
the control-owned one never received a router delivery capability. A service tunnel created through
TunnelManager was therefore off the publication path entirely — it published no LeaseSet2, encrypted or
ordinary, and could not deliver over the network.

Plan 334 recorded this as doc-versus-source drift and closed blocked, because changing another plan's
architectural invariant is not that plan's call. This plan made the source satisfy the invariant
instead of rewriting the text.

## What changed

### One manager, constructed once

- `i2pr_daemon::build_shared_service_manager` is the single production construction, called from the
  composition root. It returns `None` only for a profile that can never own a service runtime.
- `ServiceProductSpec` gains `shared_manager`. `None` keeps the private build that the controlled
  helper and the integration lanes want; production passes the shared `Arc`.
- `TunnelControlState::for_config` takes the manager instead of building one. The two integration
  tests that used production-shaped construction now call the same composition-root function, so they
  cannot drift from it.

### Ordering is explicit, not alphabetical

The graph's startup order is a topological sort whose ready set is a `BTreeSet` popped in lexical
order, so `i2pcontrol` sorted **before** `ssu2-router`. The control state reconciles onto the shared
manager, so it must run after the product has prepared that manager and installed the executable
delivery backend. `ServiceSpec::depends_on` now declares that edge, conditional on SSU2 actually being
registered. Pinned by `plan337_the_control_plane_starts_after_the_router_service`.

### The product comes up whenever a service tunnel can exist

The product gate widened from "at least one startup tunnel is enabled" to
`service_tunnels_active`, which also admits an enabled control plane. An operator adds the *first*
service tunnel to a router through TunnelManager, so requiring a configured one made the control plane
useless for exactly its purpose. I2PControl is disabled by default, so an ordinary profile is
unaffected. Pinned by `plan337_a_control_only_router_still_gets_the_service_runtime`.

### Plan 297's TLS policy reached startup services

`set_service_tls_policy`'s own doc comment always claimed the composition root called it. It did not:
only the control-owned manager ever saw the policy, so startup-owned `use_ssl` services silently ran
without one. The install now sits in `build_shared_service_manager`, where the comment says it is.

### Three control-state changes are load-bearing on a shared manager

- **`candidate_set` describes the whole runtime surface.** `reconcile` is a whole-set transactional
  replace, so a control-only candidate would have silently removed every startup-owned runtime on the
  first control transaction. Startup-owned specs are now carried through verbatim, so they diff as
  unchanged.
- **`verify_agreement` scopes its "no extra runtime" rule** to names this coordinator does not own.
- **`shutdown` reconciles back to the startup-only set** instead of tearing the manager down, which on
  a shared instance would stop the product's services.

### Publication and address

`ServiceTunnelManager` carries a per-spec ELS2 material registry holding `Arc` handles to a single
material per service — the identity is derived from the persisted Ed25519 seed, so an `Arc` clone is
another handle to *that* identity, never a second copy. `sync_els2_materials` installs or drops it at
the end of a committed transaction, so a mode edit and a delete both take effect.

`publish_service_ls2_for_service` now takes its `DatabaseStore` from `service_publication_store`, which
selects the record's **own** storage key: the day's blinded key for an encrypted service, the
destination hash otherwise, with the floodfill chosen for whichever it is. The selector is separated
from the async publication so the key choice is directly testable; it is the production path, not a
reimplementation.

`get`/`rawConfig` report `encrypted_address` for a service that publishes one. The field is **absent**,
not null, for an ordinary service, so every pre-existing output shape is unchanged.

## Evidence

| Requirement | Row | Result |
|---|---|---|
| Exactly one manager owns every service runtime | `plan337_the_composition_root_builds_exactly_one_service_manager` | PASS. Both owners receive the composition root's `Arc` from the single call site. |
| The control state holds that exact instance | `plan337_control_reconciles_onto_the_product_manager` | PASS. `Arc::ptr_eq`; a control-created runtime lands on it. |
| A control-created server reaches the publication sweep | `plan337_control_created_server_reaches_the_product_publication_sweep` | PASS. The sweep reads the committed generation, so a later reconcile is visible to it. |
| An encrypted service publishes type-5 at the **blinded** key | `an_encrypted_service_publishes_type5_at_its_blinded_storage_key` | PASS. Store key equals `blinded_storage_key(record.blinded_public_key())` and is **not** the destination hash. |
| An ordinary service is unchanged | `an_ordinary_service_publishes_its_lease_set_at_the_destination_hash` | PASS. `LeaseSet2` payload at the destination hash. |
| Isolation: a control delete spares a startup sibling | `plan337_control_delete_leaves_a_startup_sibling_intact` | PASS. Sibling runtime and spec id survive. |
| Backend installed before any control reconcile | `plan337_the_control_plane_starts_after_the_router_service` | PASS. `ssu2-router` precedes `i2pcontrol`. |
| The product exists when only the control plane can add a tunnel | `plan337_a_control_only_router_still_gets_the_service_runtime` | PASS. Manager built; the mirror case builds nothing. |
| Restart | `plan337_a_control_owned_server_survives_a_restart_without_rotating_its_identity` | PASS. Fresh manager over the same directory; destination unchanged. |
| Secrets | `plan337_no_lease_set_secret_reaches_a_control_response_or_a_debug_rendering` | PASS. Lookup secret and client key absent from responses and `Debug`; the refusal reason is static. |
| The control coordinator is unchanged | Plan 289/290/291 lifecycle rows | PASS, unmodified. |
| Startup-owned specs are unaffected | Plan 213/214/215 and the service-tunnel product suites | PASS, unmodified. |
| **A control-created encrypted server publishes** | `plan337_encrypted_control_server_without_a_persisted_identity_fails_closed` | **MET, as a refusal** — see the gap below. |

The two Plan 334 rows that pinned the defect as a negative (`plan334_control_manager_is_separate_from_the_product_manager`,
`plan334_control_created_server_is_validated_but_not_published`) are **deleted**, not reworded. Each has
no meaning now that the defect is gone, and leaving a test named after a fixed defect invites exactly
the drift this plan exists to remove. Their positive replacements are the rows above.

## Known gaps — not this plan's to close

Two defects were found by the rows above and are pinned rather than worked around. Both are recorded
as **Plan 338**.

1. **A control-created server cannot hold a persisted identity, so every encrypted mode refuses.** The
   ELS2 identity is derived from the service's persisted `ServiceDestinationRecord`. Plan 323's
   persistent-identity options are validated against client tunnels only, and a non-persistent dedicated
   group generates its identity in memory, so no record is written and the material cannot be built. The
   create is refused with the static reason `service identity record unavailable`. That refusal is the
   correct outcome — publishing an ordinary LeaseSet2 for a service configured as encrypted would look
   like success and hand clients an unencrypted service. **The practical consequence is that no
   encrypted mode Plan 334 maps is yet usable through the control surface**, even though the mapping,
   validation, persistence, redaction, and record builder all work.

2. **A failed control transaction can leave a runtime with no durable definition.** `rollback_state`
   rewrites the in-memory mirror but never reconciles the manager, and the reconcile-back inside
   `commit_locked` runs before the mirror is rolled back, so its candidate still contains the failed
   names. This is pre-existing — `sync_names` and store-publish failures reach it too — and Plan 337 only
   made it reachable for an encrypted create. Fixing it means changing every rollback path in the Plan
   289 coordinator, which is not something this plan should smuggle in alongside a composition-root
   change.

## Routine floor

- `cargo fmt --all --check`; `cargo check --locked --workspace --all-targets`;
  `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`;
  `RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps`; all clean.
- `cargo test --locked --workspace --all-targets -- --test-threads=1`: **3,950 passed / 0 failed /
  35 ignored / 144 suites** (was 3,941 / 143 before this plan; eleven rows added, two Plan 334
  defect pins deleted, one test binary added — net +9 rows, +1 suite).
- `bash scripts/check-dependency-direction.sh`, `check-runtime-boundaries.sh`,
  `check-service-tunnel-boundaries.sh`: clean. No dependency edge changed.
- `python3 -m unittest discover -s tests/planning -p 'test_*.py'`: clean.
- `bash scripts/check-global-plan-number-uniqueness.py`: clean.
- Fixture manifests and the NTCP2/SSU2/I2CP vector scripts: clean. No committed fixture bytes changed.
- All fifteen acceptance-evidence scripts: clean.
- `cargo deny check advisories bans sources`: clean. No dependency changed.

## Invariants held

- No second service/destination/tunnel runtime: the end state is **one** manager, asserted at the
  composition root.
- The control transaction coordinator's ordering, drain policy, and deadlines are unchanged; only the
  manager it reconciles changed.
- Startup-vs-control provenance, cross-class name-collision fail-closed, and the refusal to rewrite
  `router.toml` or ordinary service-tunnel files are unchanged.
- No secret reaches a response, `Debug`, error, log, or the log ring, on the two new surfaces.
- No new `i2pr-*` production edge, no new dependency, no `unsafe`.
- Listeners stay loopback-only; I2CP/I2PControl stay disabled by default and non-advertised.
- M10/M11/M12 behavior and Plan 213/214/215 qualification are untouched.

## Not claimed

- **No ELS2 capability is claimed by this plan.** The encrypted-LeaseSet2 support claim stays where
  Plans 332/333/334 put it, and is itself still incomplete on the control surface (gap 1).
- **No live interoperability.** Plan 335 remains blocked behind Plan 334, which remains blocked behind
  this plan's gap 1.
- **The i2pd/Java type-11 signature-transcript divergence** (ADR 0005) is untouched and still blocks
  Plan 335 independently.
- **The client-count ceiling divergence** is untouched: ELS2 permits 65,535, `i2pr-netdb` caps at 255,
  this control surface at 24, Emissary at 99, and no specification reconciles them.

## Docs

- `docs/architecture/i2pr-daemon.md`: the Plan 334 KNOWN DRIFT banner is replaced by the resolved
  ownership description, and `src/service_els2.rs` and the manager registry are described.
- `docs/adr/`: a new ADR records the one-manager composition-root decision and the ordering edge.
- No edge changed, so `docs/architecture/dependency-graph.md` is unchanged.
- `specs/CONFORMANCE.md` and `specs/support.toml` are **not** changed: they describe Plan 334's control
  surface, which Plan 337 does not reclose.
