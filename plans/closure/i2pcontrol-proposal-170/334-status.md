# Plan 334 status — passed: Proposal 170 encrypted-LeaseSet mode mapping

> **Reclosed 2026-10-05 as `passed-mode-mapping-and-control-surface-complete`.** This record was
> written as `blocked` on 2026-10-04. Every blocker it named has since been removed by Plan 337
> (one shared service manager) and Plan 338 (one owner for the service identity store, and a
> rollback that reaches the manager), and this plan's own black-box, rollback, and restart evidence
> landed in `db63bc0`. The sections below are **kept as written at closure time**, with the
> reclosure recorded here and in the requirement-to-evidence matrix, rather than rewritten. A closure
> record that hid the blocked state would erase the reason the three corrective plans existed.

- Plan: [`plans/implementation/i2pcontrol-proposal-170/334-prop170-encrypted-leaseset-mode-mapping.md`](../../implementation/i2pcontrol-proposal-170/334-prop170-encrypted-leaseset-mode-mapping.md)
- Status: **`passed-mode-mapping-and-control-surface-complete`**
- Decision date: 2026-10-04 (blocked) / 2026-10-05 (passed, reclosure)
- Reclosure commits: **`bdaa48c`** (Plan 338, the identity store and the rollback),
  **`db63bc0`** (the wire posture/address and the black-box evidence)
- Classification: control-plane contract plus control-surface wiring. This plan promotes **no**
  capability advertisement and **no** live interoperability.
- Normative freeze commit: **`ffb1f02`** — `specs/references/proposal-170-encryptleaseset-mode-mapping.md`
  and the byte-pinned Proposal 170 manifest, committed *before* any implementation change could
  narrow or widen the mapping.
- Implementation commits: **`e381170`**, **`586f371`**, **`0b84949`**; foundation: **`7842c93`** (Plan 337), **`bdaa48c`** (Plan 338), **`db63bc0`** (this reclosure)
- Conformance: [`specs/CONFORMANCE.md`](../../../specs/CONFORMANCE.md)
  §"Proposal 170 LeaseSet mode mapping status (Plan 334)"
- Support inventory: [`specs/support.toml`](../../../specs/support.toml) surface
  `control.i2pcontrol-leaseset-modes`

## Reclosure — 2026-10-05

Plan 334 closed `blocked` on three obligations, all of which are now met.

| Obligation at the blocked closure | How it was met |
|---|---|
| A control-created tunnel publishes no LeaseSet2, because it is reconciled onto a manager instance that is not the product layer's and carries no router delivery capability | **Plan 337** (`7842c93`, ADR 0031). The composition root builds the **one** `ServiceTunnelManager` in `build_shared_service_manager` and injects the same `Arc` into the control state and the product. `i2pcontrol` declares `depends_on("ssu2-router")` because the graph's order is lexical, so the product prepares the manager and installs the delivery backend before any control reconcile. The publication sweep reads the committed generation, so a later control reconcile is visible to it. |
| `publish_service_ls2_for_service` does not consume the ELS2 material | **Plan 337** (`6d06c73`). `service_publication_store` selects the record's **own** storage key — the day's blinded key for an encrypted service, the destination hash otherwise — and the floodfill is chosen for whichever it is. Absent material leaves the ordinary publication byte-identical. |
| The `.b32.i2p` address is not exposed | **Plan 337** added it to the control-state response; **`db63bc0`** carried it onto the **JSON-RPC wire** (`info.encryptedAddress` plus `info.lease_set_security`). Without the wire step a JSON-RPC client could never discover the address, so the whole mode mapping was unobservable to a real client — a gap this plan's own black-box rows found. |
| **Black-box evidence through I2PControl** | **NOT MET → MET** (`db63bc0`). Five rows over a real TLS listener and real JSON-RPC `TunnelManager` calls, in `crates/i2pr-daemon/tests/i2pcontrol_els2_black_box.rs`. No private bridge, LeaseSet2, driver, pump, or manager API is touched. |
| **Create/edit rollback and restart evidence** | **NOT MET → MET** (`db63bc0`). A rejected `edit` leaves the stored options, the posture, and the address byte-identical; a restart over the same data directory restores a secret-bearing encrypted definition with the address **unchanged**. |
| A type-5 record needs a persisted service identity | **Plan 338** (`bdaa48c`). Every server group is already persistent, so a control-created server always had a `ServiceDestinationRecord`; the ELS2 loader had been reading `for_service` while the runtime writes `for_group`. `ServiceTunnelManager` is now the single owner of that resolution. This also **corrects this record's** gap-1 diagnosis, which wrongly concluded the capability was missing. |

Two things this reclosure does **not** claim:

- **No live interoperability.** The Java I2P and i2pd differential is unexecuted — no runnable Java
  I2P build is provisioned — and the type-11 signature transcript is unverifiable by both
  (ADR 0005, Plan 336). That is Plan 335's obligation and it remains open.
- **The client-count ceiling divergence is untouched.** ELS2 permits 65,535 entries, the protocol
  owner accepts 255, this control surface accepts 24, and Emissary parses at most 99. No
  specification reconciles them.

The pre-existing, unrelated `parse_configured_destination` substring defect is still deliberately
unfixed and is recorded in the Plan 333 and 338 closures as well.


## Why this plan is blocked rather than passed

The plan's acceptance criterion has two halves. The first — "Proposal 170 can configure every
supported encrypted-LeaseSet mode" — is met. The second, which the plan also requires, is the
matrix-promotion rule: *"no inert promotion … every cell must have live protocol effect."* That half
is **not** met.

### Correction: the real blocker is upstream of ELS2

This record was first written naming `publish_service_ls2_for_service` as the blocker, because that
function still composes an ordinary `DatabaseStoreData::LeaseSet2`. **That was the wrong blocker**,
and the correction is recorded here rather than quietly edited away.

A `ServiceTunnelManager` reached through I2PControl is reconciled onto a **separate manager
instance**, not the one the product layer publishes through:

- `TunnelControlState::for_config` builds a fresh `ServiceTunnelManager` over an **empty**
  `ServiceTunnelSet` (`i2pcontrol_tunnels.rs`). That is the only production construction in the
  module.
- `ServiceProduct::new` builds a *different* manager over `config.service_tunnels.tunnels`
  (`service_product.rs`), installs the executable router delivery backend on it, and that is the
  only manager `publish_service_ls2_for_service` is ever handed.
- No production call site installs a delivery capability on the control-owned manager.
  `install_router_delivery_handle`, the free-function installer, is referenced only from integration
  tests.

The consequence is larger than an ELS2 gap: **a service tunnel created through TunnelManager is not
on the publication path at all.** A control-created *server* tunnel publishes no LeaseSet2 —
encrypted or ordinary. No ELS2 mode can change what it publishes, because there is nothing there to
change, and the `.b32.i2p` address Plan 334 wanted to expose would hand an operator an address for
a service no client could look up.

Two executable rows pin this:

- `plan334_control_manager_is_separate_from_the_product_manager` — the control-owned manager holds
  no delivery capability and is not `Arc`-equal to a product-style manager, while a manager *can*
  hold a capability. The asymmetry is in the construction, not the type.
- `plan334_control_created_server_is_validated_but_not_published` — a control-created encrypted
  server tunnel is a real, validated definition whose posture resolves to a type-5 publisher, and
  the `ServiceTunnelSpec` it builds carries **no** LeaseSet publication intent at all.

### Doc-versus-source drift, recorded rather than corrected

The module documentation at the top of `i2pcontrol_tunnels.rs` states:

> This module implements durable administrative ownership over **the one existing M10
> [`ServiceTunnelManager`]**. It never creates a second service/destination/tunnel runtime.

The source contradicts both halves: there are two manager instances, and a control-created runtime
is a second runtime.

This is not only a stale comment. Plan 289's own plan-of-record states the same invariant as a
requirement, not a description — "It must not create a second service/destination/tunnel runtime"
(line 13) and "Plan 289 closes when the seven lifecycle actions are real over **one** M10
`ServiceTunnelManager`" (line 128). By `plans/README.md`'s authority order the executable tests and
source outrank the prose, so the source wins and both the module documentation and the plan-of-record
invariant do not hold in the code.

Rewriting another plan's invariant is not Plan 334's call, so the drift is recorded here and the
correct fix is a plan of its own for control-owned service tunnels reaching the product layer.

### What Plan 334 would still owe once that is fixed

The ELS2 work that already landed is correct: `i2pr-daemon::service_els2` builds real type-5
records at the day's blinded storage key from the service's own Ed25519 seed, with no new stored
secret and no new `i2pr-*` edge. Once a control-owned server can publish, that material needs one
call site to consume it, and a plan would own exposing the `.b32.i2p` address.

The matrix `owner` strings name the control owner that exists today rather than a publication driver
that does not, so the repository does not over-claim.

## Requirement-to-evidence matrix

Evidence: `crates/i2pr-i2pcontrol/src/proposal_leaseset_mode.rs` (12 unit rows),
`crates/i2pr-i2pcontrol/tests/contract.rs` (the envelope row extended to all nine applied modes
plus negative rows for the refused mode, each missing companion, an empty secret, an unused
secret, a malformed list, and secret non-disclosure),
`crates/i2pr-daemon/src/i2pcontrol_tunnels.rs`
(`plan293_deep_primitives_rejected_with_named_limitation` rewritten to cover 9 modes × 4 publishing
kinds, plus the refused mode, five bad mode spellings, and the retired duplicate slot;
`plan289_unsupported_options_rejected_before_storage` extended),
`crates/i2pr-daemon/src/service_els2.rs` (7 rows).

| Requirement | Evidence | Result |
|---|---|---|
| Verify the Proposal 170 pin directly rather than trusting a reference | `docs/provenance/proposal-170-manifest.md` | PASS. Retrieved read-only, 19,010 bytes, SHA-256 `f13ae00b886c5e72131bc5d5b138a371148d1faa6899a119a1dacb65a555e7dc`, computed locally and matching the reference project's independently recorded M161 pin. The manifest now anchors to bytes, not to a URL. |
| Freeze the ten-value mapping before implementing | `ffb1f02` | PASS. The mapping document, including the two argued dispositions and the control-surface prohibitions, is committed and pushed before the first line of implementation. |
| Each of the ten values resolves to exactly one behavior | `every_ten_proposal_values_resolve_to_a_documented_behavior` | PASS. The assertion pins the ordered ten-value → eight-behavior mapping and every value's exact spelling round-trip. |
| `encrypted (aes)` is refused, not aliased | `legacy_aes_is_recognized_then_refused_by_name` | PASS. It resolves to `LegacyAes`, publishes no type-5 record, and `resolve_lease_set_security` refuses it naming the Proposal 121 supersession and the modern alternative. |
| Mode names are identifiers, never normalized | `mode_names_are_identifiers_and_are_never_normalized` | PASS. Eleven near-misses rejected, including `Disable`, `encrypted(psk)`, and `encrypted  (psk)`. |
| The two `per-user` PSK spellings are one behavior, both recorded | `per_user_spellings_are_recorded_and_change_no_behavior` | PASS. Same behavior, different reported spelling. |
| A mode implies the right address flags | `every_applied_mode_resolves_with_its_own_companions` | PASS. Four cases: none, blinding secret, client key, both. |
| A supplied-but-unused secret is an error, never a no-op | `a_supplied_but_unused_secret_is_an_error_not_a_no_op` | PASS, for the lookup secret across four modes and for the client list across three, including a mode with no `EncryptLeaseSet` at all. |
| A required secret that is absent or empty is an error | `a_required_secret_must_be_present_and_non_empty` | PASS, for all four lookup-secret modes including four whitespace-only values, and for all four authorization modes. |
| Every illegal combination is rejected before any mutation | `the_every_illegal_combination_is_rejected_before_any_mutation` | PASS, and a plan can never come back describing a required-but-missing secret. |
| Bounded client list, validated per entry | `client_entries_are_bounded_charset_checked_and_redacted`, `the_wire_array_decodes_with_both_proposal_spellings_and_is_bounded` | PASS. The count ceiling is enforced before the entry vector is built; name length, name charset, key length, and key alphabet are each checked with a distinct error. |
| An unrecognised `LeaseSetClientAuths` field is rejected | the same wire-array row | PASS, by a real implementation fix. The first version of the decoder accepted `{"Name":…,"Key":…,"Extra":1}` because the frozen envelope validator already checked shape elsewhere. The decoder is public and now closed on its own; the test caught it. |
| The durable encoding round-trips and fits the option-value ceiling | `the_durable_encoding_round_trips_and_fits_the_option_value_ceiling` | PASS. Round-trips at capacity 24 with a bound assertion; five malformed stored values fail closed rather than publishing a block. |
| `encrypt_lease_set` wire type corrected | `tunnel_options.rs`; Plan 293 divergence item 1 | PASS. `Boolean` → `String`, the Proposal's actual type. |
| `OptionalLookup` and `LeaseSetClientAuths` reach the control plane | `contract.rs::plan289_tunnel_request_envelope_rules` | PASS. All nine applied modes decode, carry the mode verbatim, and carry each companion only when the mode uses it. |
| The block is validated as a unit, not per key | `normalize_definition`; `SUPPORTED_334_OPTIONS` | PASS. The mask check inside `build_control_spec` runs first, so "this kind has no LeaseSet" stays more fundamental than "this mode does not use this secret". |
| A merged `create`+stored-`edit` candidate obeys the same law | `normalize_definition` | PASS, by re-running the identical rule on the merged options. |
| A reloaded stored definition obeys the same law | store load path | PASS. A generation that no longer satisfies the frozen mapping fails the load closed as `StoreError::Corrupt`, rather than starting a service in a posture the operator never asked for. |
| The derived posture cannot drift from the options | `ControlDefinition::lease_set_security` | PASS. It is a method that re-derives on every call, not a field stored beside the options. A stored field would have needed 64 test constructions updated and would still have been a second source of truth. |
| `get` and `rawConfig` never carry a secret byte | `lease_set_security_projection`, `lease_set_secret_presence` | PASS. Presence, the lookup secret's length, and the client count. No error, no `Debug`, and no response was shown to contain a key or a secret by `no_error_message_can_contain_a_secret`. |
| Matrix cells name a real owner, not a parser | `tunnel_matrix.rs`; `proposal_tunnel_matrix.rs` | PASS **for the control owner**, and the owner strings name the seam that is not closed. The Plan 326 deep prerequisite is retired; the `leaseset_blinding_secret` duplicate is refused by name. |
| A real type-5 record is built from the service's own identity | `every_type5_mode_builds_material_from_the_stored_identity`, `every_type5_mode_produces_a_real_type5_record_at_its_blinded_key` | PASS. Six type-5 modes produce a real `DatabaseStoreData::EncryptedLeaseSet` at the day's blinded storage key, carrying the day's blinded public key, a non-zero outer salt, and a non-empty outer ciphertext. |
| No new stored secret is needed for encryption | `unblinded_scalar_from_ed25519_seed` | PASS. The blinding identity is the Red25519 conversion of the service's existing Ed25519 signing seed, which the v1/v2 identity file format already stores. The format is untouched. |
| An authorization mismatch is refused, not published unauthenticated | `an_authorization_mismatch_is_refused_rather_than_published_unauthenticated` | PASS, for all three mismatch shapes. |
| A lookup secret changes the daily key but not the address | `a_lookup_secret_changes_the_daily_key_but_not_the_address` | PASS. Two secrets give two different daily keys and the same address; an absent secret gives a third. A client with the wrong secret finds the service and cannot read it. |
| **A control-created tunnel reaches the product layer's publication path** | `plan334_control_manager_is_separate_from_the_product_manager` | **NOT MET — pre-existing, not this plan's.** `TunnelControlState::for_config` builds its own manager over an empty `ServiceTunnelSet`; `ServiceProduct::new` builds another over the startup config and is the only one handed to `publish_service_ls2_for_service`. No production call site installs a delivery capability on the control-owned manager. |
| **A control-created server publishes a LeaseSet2 at all** | `plan337_control_created_server_reaches_the_product_publication_sweep`; `plan338_control_created_encrypted_server_publishes_type5_at_its_blinded_key` | **MET (Plan 337/338).** The definition is real and validated; the definition's publication intent is installed as ELS2 material on the shared manager at the end of a committed transaction, and the product's publication sweep — which reads the manager's **committed** generation — sees the control-created server and files a record for it. Replaces the negative pin, which asserted the opposite. |
| **Plan 289's "the one existing M10 `ServiceTunnelManager`" invariant holds** | `plan337_the_composition_root_builds_exactly_one_service_manager`; `plan337_control_reconciles_onto_the_product_manager` | **MET (Plan 337, ADR 0031).** The composition root builds the one manager and injects the same `Arc` into both owners, so the source now matches the invariant. The drift is resolved in the source rather than by rewriting another plan's text. |
| No new `i2pr-*` production edge | `scripts/check-dependency-direction.sh` | PASS. The mode contract lives in `i2pr-i2pcontrol`, which has no `i2pr-*` edge and therefore cannot reach the ELS2 owners; `i2pr-daemon` is the only layer that sees both. |
| The ELS2 material holds no long-lived secret beyond the identity it must | `ServiceEls2Material` | PASS. Not `Clone`; `Debug` prints presence and counts only. |
| **Live protocol effect for the configured mode** | `plan338_control_created_encrypted_server_publishes_type5_at_its_blinded_key`; `an_encrypted_service_publishes_type5_at_its_blinded_storage_key`; `plan334_els2_create_get_rawconfig_round_trip_over_jsonrpc` | **MET (Plan 337/338, this reclosure).** A control-created encrypted server publishes a type-5 record at the record's own blinded storage key — the store key is `blinded_storage_key(record.blinded_public_key())` and is **not** the destination hash — and the posture and address are observable over the JSON-RPC wire. The publication selector row drives the production function, not a reimplementation. |
| **The `.b32.i2p` address exposed through the control surface** | `plan338_control_created_authorized_server_address_carries_both_flags`; `plan334_els2_create_get_rawconfig_round_trip_over_jsonrpc` | **MET (Plan 337, wire step in this reclosure).** `get` reports `info.encryptedAddress` and `info.lease_set_security` on the JSON-RPC wire; the address decodes with the protocol's own decoder and its flag bits follow the mode. |
| **Black-box evidence through I2PControl** | `crates/i2pr-daemon/tests/i2pcontrol_els2_black_box.rs` (5 rows) | **MET (this reclosure).** Real TLS listener, real JSON-RPC `TunnelManager` calls, canonical Proposal 170 wire shape, no private API touched. |
| **Create/edit rollback and restart evidence** | `plan334_els2_rejected_edit_leaves_the_tunnel_unchanged_over_jsonrpc`; `plan334_els2_restart_restores_the_definition_without_rotating_the_address`; `plan338_a_rolled_back_transition_leaves_no_ghost_runtime` | **MET (this reclosure, plus Plan 338).** A rejected `edit` leaves options, posture, and address byte-identical; a restart restores a secret-bearing encrypted definition with the address unchanged; and a rolled-back transition leaves no ghost runtime — a row verified to fail when the reconcile half of the rollback is removed. |
| **Cross-implementation check against Java I2P or i2pd** | — | **NOT EXECUTED, and out of this plan's scope.** No Java I2P build is provisioned, and the type-11 signature transcript is unverifiable by both (ADR 0005, Plan 336). **This does not block this plan's closure**: the obligation is a live-interoperability obligation, and Plan 335 owns it. |

## The central finding

Proposal 170 revision 2026-05-20 lists ten `EncryptLeaseSet` values plus `OptionalLookup` and
`LeaseSetClientAuths`, and **defines nothing about them** — no per-mode property table, no wire
types, no schemas, and no precedence rule between the three parameters. Every behavior this plan
attaches to a mode comes from the referenced ELS2 specifications, not from Proposal 170. That is
recorded as a finding rather than papered over with an invented rule.

## Independent and reference evidence

| Lane | Source | Result |
|---|---|---|
| Proposal 170 source bytes | `https://i2p.net/proposals/170-i2pcontrol-expansion.txt` | Retrieved read-only and hashed locally. No Emissary, i2pd, or Java bytes were used to derive the mapping. |
| Emissary as a behavioral oracle | — | **Not invoked.** The mapping is derived from the specifications; no fixture was read out of Emissary's internals. |
| Java I2P / i2pd as ambiguity references | — | **Not consulted** for this plan. The mapping rests on the ELS2 specifications and Proposal 170's own text. |

## Security review

- **Secret hygiene.** The lookup secret and the client keys exist as plain `String`s inside
  `ControlDefinition.options`. That is the pre-existing posture for every secret option in this
  control surface and matches the per-service Ed25519 seed files. At-rest encryption remains
  `i2pr-storage`'s responsibility and is **not** claimed. The one new secret-typed value,
  `ServiceEls2Material`, is not `Clone` and has a redacting `Debug`.
- **No read-back channel.** `get` returns `[redacted]` for the two secret keys and adds a
  projection carrying presence, the lookup secret's length, and the client count. `rawConfig`
  projects `OptionalLookup` and `LeaseSetClientAuths` as presence markers. The lookup secret's
  length is bounded by the option-value ceiling and the client count by the count ceiling, so
  neither can carry arbitrary bytes, and both are derivable from the operator's own input.
- **Ownership.** No new private identity is minted. The ELS2 identity is derived from the service's
  existing Ed25519 seed rather than a second key, so there is no second private identity to
  protect and no chance of two identities for one service.
- **Failure direction.** Every illegal mode/secret/list combination is rejected *before* the
  definition is mirrored, persisted, or reconciled. Nothing falls back to a weaker posture.
- **Deprecation is named, not silent.** `encrypted (aes)` cites Proposal 121 and the modern
  alternative, so the refusal is actionable.

## Migration and compatibility review

- **No file format changed.** The v1/v2 per-service identity format is untouched.
- **No new dependency.** The plan adds no crate, no feature, and no `unsafe`.
- **Two rows of a frozen inventory changed meaning.** `encrypt_lease_set` moved from `Boolean` to
  `String`, and the matrix census moved 12 cells from incompatible to apply. A stored definition
  written before this change cannot carry `encrypt_lease_set` at all — the old type check rejected
  every value — so there is no stored-generation migration to perform.
- **One i2pr-invented name retired.** `leaseset_blinding_secret` keeps its refusal. It was never
  settable through Proposal 170, so no operator could have depended on it; its refusal message
  changed and now says why.

## Limitations and named divergences

1. **Client-count ceiling: 24 vs 255 vs 65,535.** The ELS2 authorization block format permits
   65,535 entries and `i2pr-netdb` accepts 255. This control surface accepts 24, because the list
   must survive a round trip through one durable option value bounded by `MAX_OPTION_VALUE_LEN`; the
   worst case is asserted to fit. This is a control-surface bound, not a protocol limit, and the
   Emissary oracle caps at 99 — a third number that no specification reconciles.
2. **`EncryptedServiceAddress` flags vs the string spelling.** The control surface reports
   `requiresBlindingSecret`/`requiresClientKey` booleans, which is what the frozen
   `EncryptedServiceAddress` encodes. It is not inventing a wire form for the Proposal field.
3. **Both PSK and DH client keys are 32 bytes.** i2pr cannot validate which of the two an operator
   *meant*; the mode decides how the bytes are interpreted, and the protocol owner's constructors
   do the type-specific checks. i2pr does not pretend to check operator intent.
4. **The address is deterministic from the identity.** It names the unblinded public key, so it does
   not change when the mode or the lookup secret changes. A mode change is therefore not visible in
   the address, only in the flags and the record shape.
5. **Not advertised, not live-verified, not interoperable.** Nothing in this plan changes what
   i2pr claims on the wire.

## Routine floor

Run from the repository root at the closure commit. All local; the CI labels in `AGENTS.md` do not
apply to a single-host run.

| Check | Result |
|---|---|
| `cargo fmt --all --check` | PASS |
| `cargo check --locked --workspace --all-targets` | PASS |
| `cargo test --locked --workspace --all-targets -- --test-threads=1` | **PASS — 3,941 passed, 0 failed, 35 ignored across 143 suites.** Exactly +21 over Plan 333's 3,920: the 12 new contract rows, the 7 new `service_els2` rows, and the 2 new manager-drift pinning rows. No pre-existing row was removed or weakened. |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | PASS |
| `RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps` | PASS |
| `cargo test --locked --workspace --doc` | PASS |
| `bash scripts/check-dependency-direction.sh` | PASS |
| `bash scripts/check-runtime-boundaries.sh` | PASS |
| `bash scripts/check-service-tunnel-boundaries.sh` | PASS |
| `bash scripts/check-fixture-manifest.sh` | PASS |
| `bash scripts/check-global-plan-number-uniqueness.py` | PASS |
| `python3 -m unittest discover -s tests/planning -p 'test_*.py'` | PASS — 6 tests |
| `bash scripts/check-ntcp2-vectors.sh` | PASS |
| `bash scripts/check-ssu2-vectors.sh` | PASS |
| `bash scripts/check-i2cp-vectors.sh` | PASS |
| `bash scripts/check-ntcp2-interoperability.sh` | PASS — Plan 099 static check |
| `bash scripts/check-constrained-host-lane-boundary.sh` | PASS |
| `bash scripts/check-m11-transit-boundaries.sh` | PASS |
| `bash scripts/check-m11-transit-qualification-evidence.sh` | PASS |
| `bash scripts/check-sam-acceptance-evidence.sh` | PASS |
| `bash scripts/check-ssu2-acceptance-evidence.sh` | PASS |
| `bash scripts/check-i2cp-acceptance-evidence.sh` | PASS |
| `bash scripts/check-i2pcontrol-acceptance-evidence.sh` | PASS |
| `bash scripts/check-service-tunnel-acceptance-evidence.sh` | PASS |
| `bash scripts/check-exploratory-tunnel-evidence.sh` | PASS |
| `bash scripts/check-netdb-tunnel-evidence.sh` | PASS |
| `bash scripts/check-destination-tunnel-evidence.sh` | PASS |
| `bash scripts/check-streaming-tunnel-evidence.sh` | PASS |
| `bash scripts/check-m6-mixed-router-acceptance-evidence.sh` | PASS |
| `bash scripts/check-m12-floodfill-qualification-evidence.sh --self-test` | PASS |
| `python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py'` | PASS — 18 tests |
| `cargo deny check advisories bans sources` | PASS |

No committed fixture bytes changed, so no fixture manifest or vector regeneration was required.

## Unblock audit

Run per the planning process. The plan's own blocker is upstream of the ELS2 work and is
pre-existing; the audit surfaced two further items.

| Item | Disposition |
|---|---|
| A control-created service tunnel is reconciled onto a manager instance that is not the product layer's and carries no delivery capability, so it publishes no LeaseSet2 at all | **Blocks closure of Plan 334, and is upstream of ELS2.** Pre-existing, in Plan 289's subsystem, and not fixable inside Plan 334. **Registered as Plan 337** (`plans/implementation/i2pcontrol-proposal-170/337-control-owned-service-tunnels-reach-the-product-layer.md`), a corrective pass on Plan 289, `registered` and not blocked on anything: it unifies the two managers, gives the unified one its delivery backend, wires publication for a control-owned server, and exposes the address. Until it lands no ELS2 mode can have a publication effect. |
| `publish_service_ls2_for_service` does not consume the ELS2 material | **Blocks closure of Plan 334**, but only *after* the manager unification above. `service_els2` already builds the correct record; one call site would consume it. A drafted `spec_id_for_destination` accessor was written and reverted rather than left half-landed. |
| Plan 289's "one existing `ServiceTunnelManager`" invariant is contradicted by the source | **Recorded as doc-versus-source drift.** Not corrected in place, because rewriting another plan's architectural invariant is not this plan's call. The correct fix is the same unification plan. |
| `.b32.i2p` address is not exposed | **Blocks closure of Plan 334**, after the manager unification. Exposing it earlier would publish an unreachable address. |
| No black-box I2PControl evidence | **Blocks closure.** The control-surface rows are lib-level and must be re-established as JSON-RPC round trips. |
| `parse_configured_destination` rejects any destination whose lowercased text contains `priv` | **Pre-existing, unrelated, deliberately not fixed here.** A legitimate random base64 I2P destination spells it by chance about once in 5,500 draws (measured 1.8×10⁻⁴ over 200k draws). Real availability bug in a different subsystem; the suggested fix is to decode-and-check-structure or to match an explicit `priv:` scheme prefix. Recorded again from Plan 333 so it is not lost. |
| Java I2P and i2pd authorization lanes | **Deferred to Plan 335.** Unexecuted. No runnable Java I2P build is provisioned, and the type-11 transcript divergence blocks a meaningful comparison regardless. |
| Plan 335 | **Stays blocked** on Plan 334, and additionally carries the unexecuted Java and i2pd lanes and the client-count ceiling divergence above. |

Plan 334's own registry and roadmap entries are `blocked on 337`, and Plan 337 is registered as the
corrective pass that owns the defect. Per the planning process a corrective pass is a new plan in the
same subsystem referencing the original plan and this record, and Plan 337 does that, explains why
Plan 289's verification missed the defect, and carries the regression rows.
The plan is **not** closed and **not** counted as passed in any census, milestone, or advertisement.
