# Plan 338 — Persisted service identity for a control-owned server, and rollback that reaches the manager

Status: **registered-control-servers-have-no-persisted-identity-and-rollback-leaves-ghost-runtimes**

Classification: **corrective pass** against Plan 289 (coordinator) and Plan 323
(persistent identity), on the Proposal 170 / I2PControl line.
Discovered while implementing Plan 337. Plan 337 closed with both of these pinned
as known gaps; neither is fixable inside Plan 337's ownership boundary.

Hard dependencies: Plan 289 (the coordinator whose rollback path is incomplete),
Plan 323 (the persistent-identity options, currently client-only).
Interface dependencies: Plan 324 (destination signing/encryption policy),
Plan 334 (the LeaseSet security block whose modes require a persisted identity),
Plan 337 (the shared manager and the publication path this plan feeds).

## Why this plan exists

Plan 337 unified the control-owned and product-owned `ServiceTunnelManager`, so a
service tunnel created through TunnelManager finally reaches the publication path.
That work exposed two further defects, both found by the Plan 337 evidence rows and
both pinned by them rather than worked around.

### Gap 1 — a control-created server cannot have a persisted identity, so ELS2 refuses it

An encrypted-LeaseSet2 identity is derived from the service's **persisted** identity
record: `service_els2::build_service_els2_material` reads
`ServiceDestinationRecord::signing_seed` so the published address names the
destination the inner LeaseSet2 is signed by. A control-created server has no such
record:

- Plan 323's persistent-identity options (`PersistentClientKey`, `priv_key_file`) are
  validated against **client** tunnels only — `normalize_definition` rejects them for
  a server with `"PersistentClientKey applies to client tunnels only"`.
- A non-persistent dedicated destination group generates its identity in memory.
  `ServiceTunnelManager::create_bridge_for_group` touches
  `ServiceDestinationStore` only for a `KeyReference` policy or a `persistent` group,
  so a default control-created server writes no record at all.

So `sync_els2_materials` cannot build material, and the create is refused with the
static reason `service identity record unavailable`. That refusal is correct and is
pinned by `plan337_encrypted_control_server_without_a_persisted_identity_fails_closed`:
publishing an ordinary LeaseSet2 for a service the operator configured as encrypted
would look like success and hand clients an unencrypted service.

The consequence is that **no encrypted mode Plan 334 maps is usable through the
control surface at all**, even though the mapping, the validation, the persistence,
the redaction, and the record builder all work. This is the last gap between Plan
334's control surface and a real encrypted service.

### Gap 2 — a failed control transaction can leave a runtime with no durable definition

`TunnelControlState::rollback_state` rewrites the in-memory mirror
(`definitions`, `running`, `transitioning`) but **never reconciles the manager**. Every
caller of `commit_locked` follows the same shape:

```rust
match self.commit_locked(&guard, &[name.to_owned()], probe).await {
    Ok((generation, _)) => { /* record success */ }
    Err(error) => {
        self.rollback_state(name, None, &running_before);
        Err(error)
    }
}
```

When `commit_locked` fails *after* its `reconcile` has already staged and committed
the runtime, the mirror is rolled back but the manager is not, so the runtime stays
installed with no durable definition behind it. The reconcile-back inside
`commit_locked` does not help: it reconciles to `candidate_set()`, and the mirror has
not been rolled back yet, so the candidate still contains the failed transaction's
names.

This is **pre-existing** — `sync_names` and store-publish failures reach it too — but
Plan 337 made it reachable for an encrypted create, where it is now the visible
outcome of every refused encrypted service.

## Why the earlier verification missed it

- Plan 289's evidence is `create`/`get`/`stop`/`start`/`delete` lifecycle assertions
  over the **control state**. Every one of them inspects the mirror and the store
  generation, which the rollback path does restore. None inspects the manager after a
  *failed* transaction, because a successful transaction leaves the manager agreeing
  with the mirror and a failed one is only checked for the returned error.
- Plan 323's persistent-identity rows cover client tunnels, which is the only case
  its options admit, so the client-only restriction was never exercised from the
  server side.
- Nothing before Plan 337 could reach either path: no control-created service reached
  the publication path, so no ELS2 material was ever requested, and no shared manager
  meant a ghost runtime was invisible next to a sibling the product layer owned.

## Invariants this plan must not break

- No second service/destination/tunnel runtime. Plan 337's single manager stays single.
- The control transaction coordinator's ordering stays validate-mirror-stage-reconcile-
  publish-verify, immediate-release drain, 5 s reconcile-back, and reconcile-back under
  a hard deadline on publish failure.
- A persistent server identity is **reused across restarts**, never rotated. A rotated
  identity would invalidate every client's stored address.
- Fail closed, never downgrade: a service configured as encrypted must not publish an
  ordinary LeaseSet2 under any failure.
- No new `i2pr-*` production edge, no new dependency, no `unsafe`.
- No secret in a response, `Debug`, error, log, or the log ring.
- The listener and adapter surfaces stay loopback-only; I2CP/I2PControl stay disabled
  by default and non-advertised.
- M10/M11/M12 behavior, Plan 213/214/215 application qualification, and the Plan
  226/227/228 destination-group ownership model are untouched.

## Scope

In scope:

1. **Persisted identity for a control-created server.** Extend the supported option
   subset so a server can request persistent identity semantics, and make the resulting
   `ServiceDestinationStore` path the one `load_service_els2_material` reads. Note that a
   `KeyReference` policy currently stores at `for_key_reference`, which is a *different*
   path from `for_service`; the ELS2 loader and the ownership policy must agree, or the
   record is written where the publication cannot find it.
2. **A rollback that reaches the manager.** Every path that rolls the mirror back after
   a failed transaction must also reconcile the manager to the rolled-back candidate, so
   no runtime survives a transaction that did not publish.
3. **Regression evidence** for both, plus a row proving an encrypted control-created
   server publishes a type-5 record at the blinded key and exposes a resolving address.

Out of scope, explicitly:

- Any change to the ELS2 cryptography, the mode mapping, or client authorization policy
  (Plans 326/332/333/334).
- The i2pd/Java type-11 signature-transcript divergence (ADR 0005), which blocks Plan
  335 independently of this plan.
- The client-count ceiling divergence (ELS2 permits 65,535, `i2pr-netdb` caps at 255,
  this control surface at 24, Emissary at 99). No specification reconciles them.
- The pre-existing `parse_configured_destination` substring defect (`priv`), recorded in
  the Plan 333 and 334 closures and belonging to a service-tunnel parsing plan.
- Rewriting the Plan 337 pinning rows that document today's behavior. Plan 338 replaces
  them with positive assertions and records the change.

## Work packages

1. Decide the persistent-identity option shape for a server, and record it as an ADR
   candidate. Prefer the option the startup configuration already has, so a
   control-created server and a configured one produce the same ownership policy.
2. Make `load_service_els2_material` read whichever store the resolved ownership policy
   actually writes to. A record in the wrong place is worse than no record: the address
   would name an identity the runtime does not use.
3. Make the coordinator's rollback reconcile. Prefer one helper that rolls back the
   mirror and reconciles together, so no call site can grow a mirror-only rollback.
4. Add the evidence rows, and convert the two Plan 337 known-gap assertions into
   positive assertions.

## Failure, cancel, and restart semantics

- A create/edit that cannot obtain a publishable runtime fails the same transaction it
  does today, and now leaves neither a runtime nor a durable definition.
- A persistent server identity is loaded, never regenerated, on restart. A create that
  would rotate an existing identity is refused.
- A refused rollback must not itself leave a partially reconciled manager: if the
  reconcile-back fails, the failure is reported as such rather than swallowed.
- Publication failure keeps the existing bounded retry policy and still must not fall
  back to an ordinary LeaseSet2.

## Evidence required

| Requirement | Shape |
|---|---|
| A control-created server can hold a persisted identity | Positive row; the record exists at the path the ELS2 loader reads. |
| A refused encrypted create leaves no runtime | Positive row replacing the Plan 337 known-gap assertion. |
| A failed transaction of any kind leaves no ghost runtime | Row per failure class: supervisor start, store publish, ELS2 material. |
| An encrypted control-created server publishes type-5 at the blinded key | Positive row through `service_publication_store` with material from the real record. |
| The exposed address resolves to the published identity | The `.b32.i2p` decodes and names the destination the inner LeaseSet2 is signed by. |
| Restart does not rotate the identity | Reuse the Plan 337 restart row, extended to an encrypted service. |
| Secrets | No PSK, lookup secret, DH key, or signing seed in any response, `Debug`, error, or log. |
| The Plan 289/290/291 lifecycle rows stay green | Unmodified. |

## Acceptance

This plan passes when a server created through I2PControl with a Proposal 170
encrypted-LeaseSet mode publishes a real type-5 record at the day's blinded storage
key with a correct, resolving `.b32.i2p` address, and when no failed control
transaction leaves a runtime behind. At that point Plan 334's remaining scope is its
black-box I2PControl evidence and can be reclosed, and Plan 335 becomes the only
remaining item on the line.

This plan **must not** be counted as ELS2 capability on its own. It is persistence and
transaction correctness plus the wiring that lets a real identity exist; the
encrypted-LeaseSet2 support claim stays where Plans 332/333/334 put it.

## Stop conditions

Stop and re-plan if persisted identity for a server would require changing the
`ServiceDestinationStore` durable format or migrating existing records; if it would
require a second service/destination/tunnel runtime; if it would rotate an existing
persistent identity; or if the coordinator rollback change would require changing the
Plan 180 reconcile semantics rather than adding a rollback that uses them.

## Docs

- `docs/architecture/i2pr-daemon.md`: the service-tunnel ownership description, once a
  control-created server can be persistent.
- `specs/CONFORMANCE.md` and `specs/support.toml`: only after Plan 334 recloses.
- `plans/closure/i2pcontrol-proposal-170/337-status.md`: the two known gaps stay
  recorded there; this plan does not rewrite them.

## Related

- `plans/closure/i2pcontrol-proposal-170/337-status.md` — records both gaps.
- `plans/implementation/i2pcontrol-proposal-170/337-control-owned-service-tunnels-reach-the-product-layer.md`
  — the plan that found them.
- `plans/implementation/i2pcontrol-proposal-170/289-tunnelmanager-control-state-and-existing-service-adapter.md`
  — the coordinator whose rollback is incomplete.
- `plans/implementation/i2pcontrol-proposal-170/323-tunnelmanager-canonical-nondeep-parity.md`
  — whose persistent-identity options are client-only.
