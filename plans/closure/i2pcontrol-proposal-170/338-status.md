# Plan 338 status — passed: one owner for the service identity store, and a rollback that reaches the manager

- Plan: [`plans/implementation/i2pcontrol-proposal-170/338-persisted-control-server-identity-and-transaction-rollback.md`](../../implementation/i2pcontrol-proposal-170/338-persisted-control-server-identity-and-transaction-rollback.md)
- Status: **`passed-els2-material-reads-the-writers-store-and-rollback-reconciles`**
- Decision date: 2026-10-05
- Classification: corrective pass against Plan 289 (the coordinator) and Plan 334 (the ELS2 material
  resolution). Infrastructure and transaction correctness only: **no ELS2 capability advertisement and
  no live interoperability is claimed here.**
- Implementation commit: **`bdaa48c`**
- Corrects the gap-1 diagnosis in [`337-status.md`](337-status.md) and in its own plan of record.

## The headline: Plan 337's gap 1 was diagnosed wrongly

Plan 337 closed with the finding that **a control-created server cannot hold a persisted identity**,
because Plan 323's persistent-identity options are validated against client tunnels only and a
non-persistent destination group generates its identity in memory. Both premises are false.

`ServiceTunnelSet::destination_groups` sets:

```rust
group.persistent |= service.policy.persists_client_identity()
    || service.kind.is_server()
    || matches!(service.kind, ServiceTunnelKind::HttpBidirServer);
```

So **every server group is persistent**, and `ServiceTunnelManager::create_bridge_for_group` has always
written a `ServiceDestinationRecord` for a control-created server. The real cause was a **store-path
mismatch**:

| | Path |
|---|---|
| Where the runtime wrote the record | `ServiceDestinationStore::for_group(data_dir, spec_id)` |
| Where `load_service_els2_material` read | `ServiceDestinationStore::for_service(data_dir, service_id)` |

A third path exists too — a `KeyReference` policy writes at `for_key_reference`. Three store paths for
one concept, and the ELS2 loader had duplicated the resolution instead of asking the owner, so it found
nothing and the create was refused.

The refusal itself was **correct and fail-closed**: publishing an ordinary LeaseSet2 for a service the
operator configured as encrypted would look like success and hand clients an unencrypted service. But
treating a fail-closed outcome as a missing capability, without first asking what the manager actually
writes, produced a false diagnosis. Both the Plan 337 closure and this plan's own plan of record carried
it; both now carry a dated correction rather than a silent edit, because a closure record that states a
false diagnosis is itself a defect in the record.

**No capability was missing.** The capability was always there and looked in the wrong place.

## What changed

### One owner for "where does this service's identity live"

`ServiceTunnelManager` now owns the store resolution outright:

- `create_bridge_for_group` and the new `service_identity_record` share one
  `load_or_create_identity_record` path, so the runtime and the ELS2 material cannot disagree about
  which store holds a service's identity.
- `load_service_els2_material` takes the manager rather than a data directory.
- The store is read through the **committed generation**, so the material is derived from the spec the
  runtime is actually running.
- Generating on first use and loading thereafter is unchanged, so a restart never rotates an existing
  identity.

The value of one owner is specific: a record written where the publication cannot find it publishes an
address that names an identity the service is **not using**. That is a worse failure than publishing
nothing, and it is exactly the class of bug the duplicated path allowed.

### The rollback reaches the manager

`rollback_state` now reconciles the shared manager to the rolled-back candidate as well as restoring the
in-memory mirror, and all five call sites await it. A rollback that cannot reconcile is logged rather
than swallowed, so a ghost runtime is a visible defect rather than a silent one.

The mirror-only half is kept as a separate private function, `rollback_mirror`, so the reconciling
wrapper is the only entry point a failure path can use.

## Evidence

| Requirement | Row | Result |
|---|---|---|
| A control-created encrypted server is accepted and publishes | `plan338_control_created_encrypted_server_publishes_type5_at_its_blinded_key` | PASS. Material installed from the service's own record; the address decodes with the protocol's own decoder, carries the Ed25519 unblinded type and the blinded signature type, and its flags follow the mode. |
| An authorized mode carries both address flags, and no secret | `plan338_control_created_authorized_server_address_carries_both_flags` | PASS. `requires_blinding_secret` and `requires_client_key` both set; one authorized client; the lookup secret and client key are absent from the response. |
| The ELS2 wiring weakens no Plan 334 rule | `plan338_els2_wiring_does_not_weaken_the_block_validation` | PASS. A per-user-key mode with no keys and a supplied-but-unused secret are both still errors, with no runtime and no material. |
| A rolled-back transition leaves no ghost runtime | `plan338_a_rolled_back_transition_leaves_no_ghost_runtime` | PASS, **and verified to have teeth**: disabling the reconcile half makes it fail. |
| Secrets on the new surfaces | `plan338_no_lease_set_secret_reaches_a_control_response_or_a_debug_rendering` | PASS. The lookup secret and client key are absent from the inventory, from a single-service `get`, and from the manager's `Debug` — while `encrypted_address` is genuinely present, so the redaction is not achieved by omission. |
| Restart does not rotate the identity | `plan337_a_control_owned_server_survives_a_restart_without_rotating_its_identity` | PASS, unchanged. |
| The Plan 289/290/291 lifecycle rows stay green | Plan 289/290/291 rows | PASS, unmodified. |

### What the rollback row does and does not cover

The row drives the rollback path **directly** instead of waiting for a natural post-reconcile failure.
None of the three failure classes can be triggered deterministically from an input: `sync_names` needs a
supervisor that fails to start, the store publish needs a write that fails after the generation is
staged, and the ELS2 material now resolves for every mode the control surface can express. The row was
checked for teeth by disabling the reconcile and confirming the failure.

This is narrower coverage than the row's name suggests, and the row says so. The three real call sites
are covered structurally: all of them `await` the one helper, so none can grow a mirror-only rollback.

## Routine floor

- `cargo fmt --all --check`; `cargo check --locked --workspace --all-targets`;
  `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`;
  `RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps`; all clean.
- `cargo test --locked --workspace --all-targets -- --test-threads=1`: **3,953 passed / 0 failed /
  35 ignored / 144 suites**.
- Boundary scripts (`check-dependency-direction`, `check-runtime-boundaries`,
  `check-service-tunnel-boundaries`), fixture manifest, the NTCP2/SSU2/I2CP vector scripts, all fifteen
  acceptance-evidence scripts, the planning tests, the plan-number uniqueness check, and the NTCP2
  harness lane: clean.
- `cargo deny check advisories bans sources`: clean. No dependency changed, no edge changed.

## Invariants held

- **One** `ServiceTunnelManager`; no second service/destination/tunnel runtime. ADR 0031 is untouched by
  this plan.
- A persisted service identity is loaded, never rotated. Generating on first use is unchanged.
- Fail closed, never downgrade: a service configured as encrypted is never published as an ordinary
  LeaseSet2, and every Plan 334 block validation rule still applies (pinned above).
- The coordinator's transaction ordering, drain policy, and deadlines are unchanged; only the rollback
  gained a reconcile it should always have had.
- No secret in a response, `Debug`, error, log, or the log ring.
- No new `i2pr-*` production edge, no new dependency, no `unsafe`, no change to the durable
  `ServiceDestinationStore` format or to any existing record.
- Listeners stay loopback-only; I2CP/I2PControl stay disabled by default and non-advertised.

## Not claimed

- **No ELS2 capability is claimed by this plan.** The support claim stays where Plans 332/333/334 put
  it. What this plan removes is one obstacle underneath that claim, not the claim itself.
- **No live interoperability.** Plan 335 remains blocked behind Plan 334.
- **Plan 334 is not reclosed by this plan.** Its remaining scope is its own black-box I2PControl
  evidence, which is a separate obligation.
- The i2pd/Java type-11 signature-transcript divergence (ADR 0005) and the client-count ceiling
  divergence are untouched and still block Plan 335 independently.

## Effect on the line

With Plan 337 and Plan 338 both passed, every obstacle this repository found on the Proposal 170 control
path has been removed: one manager, a publication path that a control-created service reaches, a type-5
record at the record's own blinded storage key, a resolving `.b32.i2p` on the control surface, and
transactions that leave nothing behind when they fail.

**Plan 334's remaining scope is its black-box I2PControl evidence** — a JSON-RPC create/get/rawConfig/edit
round trip, a create/edit rollback row, and a restart row — and that is now the only thing between this
line and Plan 335.

## Docs

- `docs/architecture/i2pr-daemon.md`: `src/service_els2.rs` and the `i2pcontrol_tunnels.rs` rows
  describe the manager-owned store resolution and the reconciling rollback.
- `docs/adr/0031-one-shared-service-tunnel-manager.md`: its "consequences" section now points at this
  plan for both gaps, with the corrected diagnosis.
- `specs/CONFORMANCE.md` and `specs/support.toml`: **not** changed here. They describe Plan 334's
  control surface, which Plan 334's reclosure owns.
- `plans/closure/i2pcontrol-proposal-170/337-status.md`: carries the dated correction, not a rewrite.
