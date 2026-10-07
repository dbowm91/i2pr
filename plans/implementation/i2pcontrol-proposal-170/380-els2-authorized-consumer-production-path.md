# Plan 380 — i2pr ELS2 authorized-consumer production path (PSK and DH)

Status: **registered-els2-authorized-consumer-production-path**

Classification: capability. This plan closes a deferral Plan 351 made on
purpose, and it removes the only production gap that blocks Plan 374's
authorization rows.

Hard dependencies:
- Plan 373 passed.
- Proposal 170/346 passed (deployed type-11 transcript profile).
- Proposal 170/349 passed (bounded encrypted-consumer owner).
- Proposal 170/350 passed (floodfill type-5 store and serve).
- Proposal 170/351 passed (production no-auth consumer path).

Historical source: [`374-status.md`](../../closure/i2pcontrol-proposal-170/374-status.md),
which recorded Plan 374 as blocked and identified this as the one production
gap behind that block.

## Objective

Give i2pr a **production** path to consume a PSK- or DH-authorized Encrypted
LeaseSet2 service.

Today the cryptography is complete and tested, but the wiring is not. This plan
makes `EncryptedServiceResolver::begin_authorized` reachable from the product's
own consumer path, and gives it a secret-safe source of client credential
material.

This plan does **not** attempt Plan 374's external lane. It closes the local
half so that Plan 374's remaining work is purely orchestration.

## Why this is ready

Every hard dependency is closed, and the interface dependency — the bounded
resolver owner — is a stable written contract that has not changed since Plan
349 and is exercised by eight passing rows.

More importantly, this plan is **independently verifiable with no external
router**. That is the whole reason it is sized this way: Plan 374 cannot close
locally, and registering a plan whose only evidence can come from an external
lane would repeat exactly the mistake this plan exists to avoid.

## Current implementation evidence

The publisher and the crypto are done. Only the consumer wiring is missing.

**Publisher side, complete:**
- `i2pr-client::encrypted_leaseset::EncryptedLeaseSet2Publisher::authorized_address`
  (`crates/i2pr-client/src/encrypted_leaseset.rs:204`) emits the client-auth b33.
- `::build_authorized_database_store` (`crates/i2pr-client/src/encrypted_leaseset.rs:662`)
  builds the signed, authorized type-5 record.
- `crates/i2pr-daemon/src/service_els2.rs:210-225` dispatches to the authorized
  builder when the service carries authorization.

**Consumer owner, complete and bounded:**
- `EncryptedServiceResolver::begin_authorized`
  (`crates/i2pr-daemon/src/encrypted_service_resolver.rs:309`) takes
  `Els2ClientAuth`, converts it to an owned non-`Clone` credential, and rejects
  a missing credential at `InvalidAddress` rather than starting a lookup that
  is certain to fail.
- `MAX_CONCURRENT_ENCRYPTED_RESOLVES` (`:72`) caps the in-flight table.
- `cancel` (`:377`) and `ingest_store` (`:416`) exist for every exit path.

**Consumer owner behaviour, already proven:**
`crates/i2pr-daemon/tests/encrypted_service_consumer.rs` drives the owner and
already covers `every_authorized_psk_client_resolves_through_the_owner` (`:228`),
`an_authorized_dh_client_resolves_through_the_owner` (`:278`),
`a_wrong_psk_is_refused_and_releases_its_lease` (`:365`), and
`a_secret_plus_authorized_service_resolves_when_both_match` (`:322`).

**The gap, precisely.** The production consumer is
`resolve_encrypted_destination_for_service`
(`crates/i2pr-daemon/src/service_product.rs:3745`). At `:3774` it calls plain
`begin()`, and at `:3778-3782` maps the authorized-path errors to
`UnwrapFailed` with the comment:

```rust
// a `b33` that requires per-client authorization cannot be
// resolved without a credential. Plan 351 defers consumer
// credentials (PSK/DH) on purpose
```

`grep -rn begin_authorized crates/` returns only the test file. There is no
production caller.

**The second gap: no credential source.** `manager.encrypted_target_secret`
(`crates/i2pr-daemon/src/service_tunnels.rs:2842`) returns a
`red25519::LookupSecret`, installed from the `leaseset_password` I2PControl
option by `sync_encrypted_target_secrets`
(`crates/i2pr-daemon/src/i2pcontrol_tunnels.rs:3722-3757`). That is the
*b33 lookup secret*, which is a different value from the per-client PSK or DH
credential. There is no configuration surface, manager field, or storage path
for a client auth credential.

## Invariants that must not regress

1. **A b33 address alone must still decrypt with no credential.** Client
   authorization is additive. The no-auth path must not start demanding a
   credential, or every existing working configuration breaks.
2. **An address that requires authorization and has no credential must fail
   closed at the boundary**, never start a lookup that is certain to fail, and
   never fall back to a no-auth attempt.
3. **Credential material must be secret.** No `Debug`, no `Display`, no
   unrestricted serialization, no `Clone`, zeroize where supported. It must be
   stored through the router-bound secret owner (Plan 341's discipline), never
   as a plaintext control option that reaches a status surface or a log line.
4. **The in-flight table stays bounded** and the lease is released on every
   exit path: success, wrong credential, refused reply, cancel, and shutdown.
5. **No decoded-LeaseSet injection.** Every row must go through derive →
   blinded-key lookup → `DatabaseStore` reply → owner ingest → unwrap. A row
   that hands the owner a pre-built record proves nothing and is refused by
   review.
6. **The `DelayOpen` Gate 1 rule is unchanged.** An encrypted target still
   requires a delay-open client, and a static alias still may not name an
   encrypted address.
7. **No advertisement change.** Type 5 stays `advertised = false`; this plan
   adds no support surface.

## Scope

**In:**
- A secret-safe source for per-client PSK / DH credential material on a service
  tunnel that carries an encrypted target.
- Wiring `begin_authorized` into `resolve_encrypted_destination_for_service`
  when a credential is present.
- The fail-closed branch when authorization is required and no credential is
  configured.
- Production-path tests that drive `resolve_encrypted_destination_for_service`,
  not only the owner.
- Documentation of the credential source and its lifecycle.

**Explicitly out:**
- **Plan 374's external lane.** No stock i2pd process, no controlled
  R/F/D topology, no SAM or I2CP reference configuration, no cross-router
  matrix, no evidence artifact. That is Plan 374's remaining scope.
- **Any new advertisement.** Type 5 remains `advertised = false`.
- **Transport, codec, or crypto changes.** Every primitive this plan calls
  already exists and is tested; this plan changes no wire format.
- **Enabling authorized consumption in the router's default configuration.**
  The credential is opt-in per service.
- **Plan 375's Java surface.**

## Required production changes

1. **A credential field on the manager**, beside the existing
   `encrypted_target_secrets` map: `install_encrypted_target_credential` /
   `remove_encrypted_target_credential` / `encrypted_target_credential`, holding
   an owned credential enum mirroring `OwnedClientCredential`. No `Debug` on the
   credential type; the map's owner accessors must not format one.
2. **A control-surface option**, resolved alongside `leaseset_password` in
   `sync_encrypted_target_secrets`. It must be parsed and length-bounded exactly
   like the lookup secret, must reject a credential on a service that names no
   encrypted target, and must drop both secret and credential on a definition
   that stops being one — the existing fail-closed rule
   (`i2pcontrol_tunnels.rs:3743-3747`) extended to the new field.
3. **Secret-owner sealing**, following Plan 341's pattern, so the credential
   is never held in plaintext in the control store. If a control client supplies
   it, the stored form must be sealed under the router identity and only opened
   inside the resolution path.
4. **The production branch** in `resolve_encrypted_destination_for_service`:
   select `begin_authorized` when a credential is present and `begin` otherwise,
   and make the "required but absent" case an explicit `EncryptedTargetStatus`
   arm rather than a generic unwrap failure, so an operator can tell *why* a
   service did not resolve.
5. **Ordering with the lookup secret.** A b33 may carry both a lookup secret and
   client authorization. `begin_authorized` already accepts the secret; the
   wiring must pass both, and a row must prove the combination.

## Work packages

- **WP1 — credential storage and accessors.** Manager field, install/remove/get,
  no-`Debug` discipline, bounded and non-`Clone`. Unit rows for install, remove,
  replace, and per-service isolation.
- **WP2 — control surface and sealing.** Option grammar, bounds, the
  encrypted-target pairing rule, secret-owner sealing and opening, and the
  fail-closed drop when a definition stops being an encrypted target. Rows for
  malformed, oversized, and orphaned credentials.
- **WP3 — the production branch.** `begin_authorized` selection, the
  required-but-absent status arm, secret+credential combination, and lease
  release on every exit path including shutdown.
- **WP4 — authorized rows in the existing production-wiring file.**
  `crates/i2pr-daemon/tests/encrypted_service_consumer_wiring.rs` already exists
  and holds Plan 351's wiring-and-policy rows — address parsing, the
  unblinded-destination binding, refusal of a foreign or unverifiable record,
  cross-day refusal, and the no-collapse projection. **Every one of those rows
  is no-auth only**, and its own header says so. Plan 380 adds the authorized
  counterparts to that same file rather than creating a new one: PSK success,
  DH success, missing credential, wrong credential, and secret + credential
  together. The existing rows must stay green unchanged — if an authorized row
  can only pass by weakening one of them, that is a stop condition.
- **WP5 — guards and documentation.** Extend
  `scripts/check-encrypted-service-consumer-caller.sh` (or add the narrowest
  correct guard) so a future removal of the production caller fails loudly.
  Update `specs/CONFORMANCE.md` to distinguish no-auth production consumption
  from authorized production consumption, and record that both are i2pr-internal
  and neither is interoperability.

## Failure, cancellation, restart, and contention semantics

- **Missing credential** — explicit status arm; no lookup is issued.
- **Wrong credential** — the unwrap fails, the lease is released, and the
  status distinguishes it from a missing credential.
- **Cancellation / shutdown** — the resolver is dropped with the task; the
  in-flight entry must not outlive it. `cancel` remains the explicit path.
- **Contention** — `MAX_CONCURRENT_ENCRYPTED_RESOLVES` (8) still bounds the
  table. A credential must not add an unbounded wait; it is read at `begin`
  time and then dropped.
- **Restart** — credentials are sealed in the control store, so a restart must
  recover them through the same secret owner the lookup secret uses. A copy of
  the data directory without the router secret must fail to open them, exactly
  as it must for the lookup secret.
- **Per-service isolation** — Plan 351's `provision_encrypted_service_target`
  contract is unchanged: a failure degrades that one service and never
  propagates.

## Compatibility and migration

No wire, codec, or storage-format change to any published protocol. The new
control option is additive and absent-by-default: a definition that does not set
it behaves identically to today. Existing stored definitions revalidate, because
`sync_encrypted_target_secrets` already re-parses on every commit. No migration
step and no rollback step beyond removing the option.

## Required tests

- Unit: manager credential install/remove/replace/isolation; credential type has
  no `Debug` that formats key bytes; option grammar and bounds; sealing and
  opening; the required-but-absent status arm.
- Authorized production-path rows, added to
  `crates/i2pr-daemon/tests/encrypted_service_consumer_wiring.rs`: PSK success;
  DH success; secret + credential together; missing credential refused with no
  lookup issued; wrong credential refused and the lease released; shutdown
  mid-resolve releases.
- Regression, all of which must be green before and after: the existing eight
  `encrypted_service_consumer.rs` owner rows, the existing Plan 351 wiring rows
  in `encrypted_service_consumer_wiring.rs`, and `service_els2.rs`'s publisher
  rows.
- Guard: a mutation that deletes the production `begin_authorized` call fails
  the checker.

## Exact verification commands

Run from the repository root. The full routine floor in `AGENTS.md` applies;
this plan changes production code, so it is not exempt.

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo build --locked -p i2pr-app-fixture -p i2pr-apphost
cargo test --locked -p i2pr-daemon --test encrypted_service_consumer -- --test-threads=1
cargo test --locked -p i2pr-daemon --test encrypted_service_consumer_wiring -- --test-threads=1
cargo test --locked -p i2pr-daemon --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
bash scripts/check-encrypted-service-consumer-caller.sh
bash scripts/check-els2-type11-transcript-boundary.sh
bash scripts/check-service-tunnel-boundaries.sh
bash scripts/check-config-secret-hygiene.sh
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
python3 scripts/check-global-plan-number-uniqueness.py
python3 -m unittest discover -s tests/planning -p 'test_*.py'
```

## Documentation updates

- `specs/CONFORMANCE.md`: separate no-auth from authorized ELS2 consumption and
  state plainly that both are i2pr-internal, that neither is interoperability,
  and that Plan 374 remains the owner of the live cross-router rows.
- `specs/support.toml`: `common.leaseset2-family` notes gain the authorized
  consumer status **without any change to `advertised`**.
- `docs/architecture/i2pr-service-tunnels.md`: the credential source, its
  sealing, and its lifecycle.
- `plans/subsystems/red25519-encrypted-leaseset-roadmap.md` and
  `plans/subsystems/i2pcontrol-proposal-170-roadmap.md`: register the plan and
  its position in the 374 → 380 → 381 sequence.

## Acceptance criteria

Plan 380 passes when a service tunnel configured with a client credential
resolves an **authorized** type-5 record through the product's own production
consumer path, for PSK and for DH, with wrong- and missing-credential rows
failing closed, with no regression to the eight existing no-auth/secret rows,
with the credential provably absent from status output, logs, counters, and the
control store in plaintext, and with a guard that fails if the production caller
is removed.

Passing Plan 380 does **not** unblock Plan 374 on its own. It removes one of
Plan 374's blockers; the external driver lane remains.

## Stop conditions

Stop and register a corrective rather than continuing if any of these occur:

- The credential cannot be stored without a plaintext value ever reaching the
  control store. That would be a secret-hygiene failure, and Plan 352's
  machinery applies.
- `begin_authorized`'s existing contract turns out to be wrong for the real
  lookup composition — for example if the credential must be chosen per
  attempt rather than per resolution. That is a contract question and needs a
  corrective, not a workaround.
- Correctness requires any change to the deployed type-11 transcript profile or
  the ELS2 wire format. ADR 0032 (Proposal 170) governs that and this plan may
  not touch it.
- Any row can only pass by injecting a decoded LeaseSet, sharing an in-process
  store where a real lookup is required, or modifying i2pd. All three are
  forbidden by Plan 374 and by this plan.
- The work stops being one coherent pass. The credential *source* (WP1+WP2) and
  the *wiring* (WP3+WP4) may be split into two plans; the driver must not be
  folded in.

## Closure evidence required

- A requirement-to-evidence matrix covering every acceptance criterion.
- Commands run with outcomes, labelled local or CI truthfully.
- A statement of what remains unproven after this plan, naming Plan 374's
  driver lane explicitly.
- A secrets review: where the credential exists in plaintext, and for how long.
- An invariant review against all seven items above.
- Findings by severity, and the roadmap disposition with an unblock audit.

## Handoff notes

- **Do not start this plan by touching the external lane.** The entire point of
  the split is that this half closes locally.
- Read `374-status.md` first; it carries the source-level proof that both
  directions are protocol-feasible and that i2pd implements all three auth
  modes, which is why Direction B's PSK/DH rows are unsatisfiable until this
  plan lands.
- `tests/integration/els2/reference-freeze.md` records the reference builds and
  the controlled mesh. It is not needed for this plan and is not evidence for
  it.
- The natural successor is **Plan 381**: Plan 374's remaining external driver
  scope — the `R` service role, the lane configuration profile with SAM/I2CP/HTTP
  enabled, loopback application payloads on both sides, the negative rows, and
  the evidence artifact and checker. Plan 374's closure already names that
  scope; 381 should reference this plan as its closed hard dependency.