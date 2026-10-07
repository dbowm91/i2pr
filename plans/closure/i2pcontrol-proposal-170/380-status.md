# Plan 380 — ELS2 authorized consumer production path (PSK/DH): status

Status: **passed-authorized-consumer-production-path-closed-with-two-defects-found-and-fixed**

Plan of record:
[`380-els2-authorized-consumer-production-path.md`](../../implementation/i2pcontrol-proposal-170/380-els2-authorized-consumer-production-path.md).

Classification: local capability + security hygiene. This plan changes
production behavior; it promotes **no** advertisement. Type 5 stays
`advertised = false` and `full-proposal-conformant` stays unset.

External router interoperability was **not** required and was not attempted.
That is Plan 374's scope, and it remains blocked.

## The stop condition that fired, and what it cost

WP2 asked for a control-surface option to carry the credential. It has none,
and the reason is structural rather than an oversight:

- `i2pr_i2pcontrol::proposal_wire::proposal_tunnel_value_type`
  (`crates/i2pr-i2pcontrol/src/proposal_wire.rs:481`) returns `None` for any
  field outside the frozen 75-name `PROPOSAL_TUNNEL_MANAGER_FIELDS`, after
  which `decode_tunnel_request` rejects it as `UnknownKey`
  (`tunnel_request.rs:329`).
- Every i2pr-local service option that exists today reached the daemon because
  **Proposal 170 defines a wire field for it**: `leaseset_password` ←
  `OptionalLookup`, `leaseset_client_auth` ← `LeaseSetClientAuths`, the Plan
  342 outproxy block ← `ProxyList` / `Outproxy*`. Proposal 170 defines no
  consumer-credential field, so there is none to map onto.
- The alternatives were checked rather than assumed. `CustomOptions` was
  refused outright ("no safe typed allowlist"); `control_sources` is
  inspection/metrics, not a configuration ingress; and the
  `[service_tunnels]` TOML layer has no ELS2 concept at all — a `.b33` target
  cannot be expressed there either.
- Adding the name to the frozen inventory was the one option ruled out on
  principle. `proposal_matrix_covers_every_canonical_option_for_all_twelve_types`
  (`proposal_tunnel_matrix.rs:394`) audits that inventory as *Proposal*
  conformance data, so a row for a field Proposal does not define would make
  `full-proposal-conformant` a false statement rather than an ambitious one.

This is the plan's own stop condition ("the credential source (WP1+WP2) and the
wiring (WP3+WP4) may be split into two plans"), escalated for a decision because
the remaining choice was a product/architecture boundary rather than an
implementation detail. The decision was taken in favour of a typed extension
seam on the existing `CustomOptions` field — see below.

## What was built

**A second sealed domain, not a reuse of the first.** `RouterSecretOwner`
(`crates/i2pr-service-tunnels/src/outbound_secret.rs`) is a supertrait of
`OutboundSecretStore` adding the ELS2 consumer-credential half. The
implementation (`crates/i2pr-daemon/src/outbound_secret.rs`) derives it under
`i2pr:els2-consumer-credential:secret-box:v1` and seals it under the marker
`$i2pr1e$`, with the marker as AEAD associated data. Sharing Plan 341's key
would have let a stored outproxy credential copied into the credential slot
open successfully there. Both domains come from one `Arc` and one derivation
from the router identity, so "derive once" stays literally true, and the outproxy
runtime keeps an ordinary `Arc<dyn OutboundSecretStore>` by trait upcast.

**A credential that is a type, not a string.**
`crates/i2pr-daemon/src/encrypted_target_credential.rs` holds a `PskClientKey`
or an X25519 key pair. For Diffie-Hellman the option carries the **private key
only** and the public key is derived from it: the record names the client's own
public key, so a configuration able to state a mismatched pair would be a
credential that can never authorize anything, for a reason the operator cannot
see. No `Debug`, no `Display`, no `Clone`, no serializer; every error reason is a
`&'static str`.

**A sealed stored form the manager holds.**
`SealedEncryptedTargetCredential` is ciphertext plus the owner that opens it.
`ServiceTunnelManager::install_encrypted_target_credential` takes *that type*,
so "the manager never holds the key" is a property of the signature rather than
a rule anyone has to remember.

**The typed extension seam.** `crates/i2pr-i2pcontrol/src/extension_options.rs`
carries i2pr's own options inside Proposal 170's `CustomOptions` field as
`{"i2pr": {"LeasesetClientCredential": "..."}}` — one namespace key, a closed
allowlist, string-only values, bounded in count, name length and value length.
It is deliberately **not** an escape hatch: a value that is not a JSON object is
the Proposal untyped pass-through and is refused with the historical
"no safe typed allowlist" error, and an unrecognised extension name is refused
rather than ignored. The frozen Proposal inventory is untouched.

**The production branch.** `resolve_encrypted_destination_for_service`
(`crates/i2pr-daemon/src/service_product.rs:3765`) selects `begin_authorized`
when a credential is held and `begin` otherwise. This is safe because
`EncryptedLeaseSet2Resolver::new_authorized` delegates to the same builder with
the address's own `requires_client_key` flag, so a redundant credential on a
`.b33` that does not demand one is presented and ignored rather than rejected for
being present — pinned by
`a_credential_on_an_unauthorized_address_is_harmless`.

**Three status arms replacing one wrong one.** The required-but-absent case was
mapped to `StorageKeyUnavailable`, which tells an operator to chase a network or
storage problem for a configuration omission — and no lookup is even issued.
`ClientCredentialRequired`, `ClientCredentialRejected` and
`ClientCredentialUnusable` now separate "configure a credential", "fix the
credential" and "fix the router/data directory".

## What this plan found

Two defects. The second is **pre-existing**, from Plan 342, and Plan 380's
closure record is the forward attribution.

1. **`edit` re-sealed both stored credential forms** *(high — one half
   pre-existing)*.
   `TunnelControlState::edit` merges the stored options with the request's, so
   `normalize_definition_with_filter_root` runs the seal step again over a value
   that is *already* a stored form. Two different failures, because the two seal
   steps were written at different times:
   - The **ELS2** credential's seal step tried to parse its own ciphertext as a
     plaintext credential, so **every** edit of a credential-bearing service
     failed outright. This one was introduced while writing Plan 380 and caught
     before commit.
   - The **Plan 342 outproxy** credential re-sealed *silently*. The stored form
     became sealed twice; opening it once returned the previous stored form **as
     text**, which `RouterOutproxyProvider` then presented to the outproxy as the
     HTTP proxy password. The outproxy answers 407 and no status surface says
     why.

   Demonstrated by direct probe before fixing: first seal 78 bytes, second 220,
   unequal, and `store.open(second)` returned `$i2pr1o$3030b0e2a97376b4…`.

   **Fixed**: both seal steps recognise their own marker and pass the form
   through, after validating its framing. Re-sealing would also be wrong on its
   own terms — a fresh nonce would change the stored bytes after an edit that
   changed nothing about the credential, and a generation round trip would stop
   being byte-stable.

   Plan 342's rows could not see the outproxy half because they exercised
   generation round trips, which never re-run the seal step, and **no Plan 342 or
   Plan 376 row ever edited an outproxy tunnel**.
   `plan342_sealed_block_survives_a_generation_round_trip` carried the comment
   "so an untouched credential survives an edit"; the first clause was true and
   the second was untested. Plan 342's closure record is **not** rewritten; this
   record is the correction.

2. **`edit` cannot remove a credential** *(medium — limitation, not a defect)*.
   The same merge semantics mean an edit that omits the credential keeps it. An
   operator who edits a service to stop presenting a credential would otherwise
   believe they had. This is the same limitation Plan 376 found and pinned for
   the outproxy block, with the same remedy: `delete` then `create`. Pinned as a
   property by `plan380_an_edit_cannot_remove_a_credential_but_a_delete_can`, and
   documented in the daemon deep-dive, so it is never the only thing a reader
   has.

## Requirement-to-evidence matrix

### Credential source (plan WP1)

| # | Requirement | Status | Evidence |
|---|---|---|---|
| 1 | PSK and DH both parse, and `parse → render` is the identity | pass | `both_schemes_parse_and_round_trip_exactly`, `the_scheme_is_reported_and_never_the_key` |
| 2 | a DH public key is derived, never configured | pass | `a_dh_credential_derives_its_public_key_from_the_private_one` |
| 3 | a malformed value is refused by shape and never echoed | pass | `malformed_values_are_refused_by_shape_and_never_echoed` (over-bound, no scheme, unknown scheme, short, long, non-hex, uppercase) |
| 4 | no `Debug`/`Display`/`Clone` on the credential | pass | guard §5b, mutation-tested; `the_credential_is_describable_without_being_printable` cannot use `unwrap_err` *because* `T: Debug` is unsatisfied |
| 5 | the sealed form is ciphertext and opens back exactly | pass | `a_sealed_credential_holds_ciphertext_and_not_its_value`, `a_credential_is_not_transferable_to_another_router` |
| 6 | the manager holds only sealed forms, per spec | pass | `the_manager_registry_is_per_spec_and_fail_closed_on_remove`, guard §5c |
| 7 | the two sealed domains cannot open each other | pass | `a_credential_is_not_transferable_to_another_router`; guard §5d pins the distinct label and marker |

### Control surface (plan WP2)

| # | Requirement | Status | Evidence |
|---|---|---|---|
| 1 | the option reaches the daemon through a typed, bounded seam | pass | `plan380_a_credential_creates_and_never_appears_on_the_wire`; contract rows in `plan289_tunnel_request_envelope_rules` |
| 2 | the untyped `CustomOptions` blob stays refused, same error | pass | `plan380_the_extension_seam_refuses_everything_it_does_not_type`; `custom_options_are_rejected_as_invalid_params_without_allocation` unchanged and green |
| 3 | an unknown extension name is refused, not ignored | pass | same row; `the_allowlist_is_the_whole_of_the_supported_surface` pins the allowlist's cardinality |
| 4 | the credential requires an encrypted target | pass | `plan380_a_credential_on_an_ordinary_target_is_refused` |
| 5 | a malformed credential is refused before storage | pass | `plan380_a_malformed_credential_is_refused_by_shape` (5 shapes) |
| 6 | the stored definition holds a sealed form, not the value | pass | `plan380_the_stored_definition_holds_a_sealed_credential` |
| 7 | no byte of the value crosses the wire | pass | `plan380_a_credential_creates_and_never_appears_on_the_wire` (whole response body, plus a run of the plaintext's bytes) |
| 8 | refused when no secret owner is installed | pass | `plan380_a_credential_needs_a_secret_owner_to_exist` |
| 9 | no advertisement change | pass | guard §5e: the name may not appear in `proposal_wire.rs` or `tunnel_options.rs`; `PROPOSAL_TUNNEL_MANAGER_FIELDS` is still 75 |

### Restart and removal (plan WP2)

| # | Requirement | Status | Evidence |
|---|---|---|---|
| 1 | a restart re-adopts the same bytes, not a fresh seal | pass | `plan380_a_credential_survives_a_restart_over_the_same_data_directory` (byte equality) |
| 2 | a data directory copied to another router cannot recover it | pass | `plan380_a_credential_is_unrecoverable_without_the_router_secret` |
| 3 | an unrelated edit preserves the sealed form byte-for-byte | pass | `plan380_an_unrelated_edit_preserves_the_sealed_credential`, `plan380_an_unrelated_edit_leaves_the_sealed_credential_unchanged` |
| 4 | a delete drops the installed credential | pass | `plan380_an_edit_cannot_remove_a_credential_but_a_delete_can` |
| 5 | `edit` cannot remove one — documented, not silently assumed | pass | same row |

### Wiring and resolution (plan WP3, WP4)

| # | Requirement | Status | Evidence |
|---|---|---|---|
| 1 | PSK client resolves the published record | pass | `an_authorized_psk_credential_resolves_through_the_owner` |
| 2 | DH client resolves the published record | pass | `an_authorized_dh_credential_resolves_through_the_owner` |
| 3 | the lookup secret and the credential each do their own job | pass | `a_lookup_secret_and_a_credential_each_do_their_own_job` — a wrong secret still addresses a different key even with a valid credential |
| 4 | required-but-absent fails **before a lookup is composed** | pass | `a_required_credential_that_is_absent_fails_before_a_lookup_is_composed` (`in_flight() == 0`) |
| 5 | a wrong credential is refused and releases the lease | pass | `a_wrong_credential_is_refused_and_releases_its_lease` |
| 6 | an unnecessary credential is harmless | pass | `a_credential_on_an_unauthorized_address_is_harmless` |
| 7 | the three status arms are distinct and static | pass | `the_credential_status_arms_are_distinct_and_static` |
| 8 | no decoded-LeaseSet injection anywhere | pass | every authorized row goes publisher → `DatabaseStoreMessage` → `ingest_store` → `inner_lease_set2()`; the record unwrapped is the one the publisher built |
| 9 | the eight pre-existing owner rows and the eight Plan 351 wiring rows stay green unchanged | pass | `encrypted_service_consumer.rs` 17/17, `encrypted_service_consumer_wiring.rs` 18/18 |

## Commands run (local)

```text
cargo fmt --all --check                                            OK
cargo check --locked --workspace --all-targets                     OK
cargo build --locked -p i2pr-app-fixture -p i2pr-apphost           OK
cargo test --locked --workspace --all-targets -- --test-threads=1  exit 0; 4 640 passed, 0 failed, 35 ignored (env-gated)
    # totals derived mechanically: 4 675 tests listed, 35 ignored
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings   OK
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps               OK
cargo test --locked --workspace --doc                                             OK
bash scripts/check-config-secret-hygiene.sh                          OK
bash scripts/check-encrypted-service-consumer-caller.sh              OK (8 pins, mutation-tested)
bash scripts/check-dependency-direction.sh                           OK
bash scripts/check-runtime-boundaries.sh                             OK
bash scripts/check-console-boundaries.sh                             OK
bash scripts/check-console-browser-security.sh                       OK
bash scripts/check-service-tunnel-boundaries.sh                      OK
bash scripts/check-outproxy-request-path.sh                          OK
bash scripts/check-outproxy-wire-lane-evidence.sh                    OK
python3 scripts/check-global-plan-number-uniqueness.py               OK
python3 scripts/check-adr-number-uniqueness.py                       OK
python3 -m unittest discover -s tests/planning -p 'test_*.py'       OK
```

**Not run in this pass:** the external interop lanes
(`tests/integration/els2/**`, `tests/integration/m6-interop/**`,
`tests/integration/ssu2/**`) and the macOS/bash-3.2 checkers listed in
`AGENTS.md`. No production wire changed, so the Proposal field inventory, the
`check-*-vectors.sh` fixture checks and `check-fixture-manifest.sh` are unaffected
by construction; they were last run green on the preceding commit of this branch.

**One pre-existing flake, unrelated to this plan.**
`i2pcontrol_tunnels::tests::plan292_idle_sweep_closes_quiet_tunnels` failed once
in a full-suite run and passed on three isolated re-runs. It is **not** caused by
this plan: the same test fails on an unmodified `HEAD` checkout in a separate
git worktree. The mechanism is a race in the test's own arithmetic — the
runtime's `last_activity_ms` is wall-clock at construction
(`service_tunnels.rs:4112`), the test reads `base` *after* `create`, and
`idle_ms = now_ms - last_activity_ms` (`i2pr-service-tunnels/src/idle.rs:43`),
so the "no early fire" assertion has only a 500 ms margin against a full
create-and-reconcile. Recorded as a finding below rather than fixed here.

## Invariant review (all seven)

| # | Invariant | Status | Note |
|---|---|---|---|
| 1 | a `.b33` alone still decrypts with no credential | held | the branch is selected by credential presence and `begin` is untouched; `a_credential_on_an_unauthorized_address_is_harmless` and all eight Plan 351 rows |
| 2 | required-but-absent fails closed at the boundary, never starts a lookup, never falls back | held | `a_required_credential_that_is_absent_fails_before_a_lookup_is_composed`; `begin` refuses such an address outright, so nothing is composed |
| 3 | credential material is secret and stored only via the router-bound owner | held | guard §5b/§5c; the plaintext exists only in `normalize_definition_with_filter_root` and in one resolution; `Debug` reports presence and a length only |
| 4 | the in-flight table stays bounded; leases release on every exit | held | `MAX_CONCURRENT_ENCRYPTED_RESOLVES` unchanged; wrong-credential and refused-reply rows both assert `in_flight() == 0` |
| 5 | no decoded-LeaseSet injection | held | every authorized row derives → looks up → ingests → unwraps |
| 6 | the `DelayOpen` Gate 1 rule is unchanged | held | untouched; the new pairing rule is a *stricter* addition that also applies when Gate 2 does not |
| 7 | no advertisement change | held | `PROPOSAL_TUNNEL_MANAGER_FIELDS` still 75; `SECRET_OPTIONS` still 5; type 5 `advertised = false`; `full-proposal-conformant` unset; guard §5e |

## Secrets review — where the plaintext exists, and for how long

| Location | Form | Lifetime |
|---|---|---|
| the JSON-RPC request body | `psk:`/`dh:` + 64 hex, in TLS memory | until the request is parsed |
| `decode_extension_options` | `serde_json::Value` string | until `decode_tunnel_request` returns |
| `normalize_definition_with_filter_root` → `seal_encrypted_target_credential` | `&str` | one call; the parsed `EncryptedTargetCredential` is dropped at end of function |
| the router-bound owner's AEAD | plaintext buffer inside `seal_with_rng` | zeroized by the `Zeroizing` frame |
| `ServiceTunnelManager::encrypted_target_credentials` | **ciphertext only** | life of the spec |
| `resolve_encrypted_destination_for_service` | opened `EncryptedTargetCredential` | one resolution; zeroized on drop |
| generation file on disk | **ciphertext only** (`$i2pr1e$…`) | life of the file |
| `get` / `rawConfig` / status / counters / logs | `clientCredentialConfigured: bool` | — |

Never held: `Config`, any `Raw*Config` struct, `DestinationRef`,
`SECRET_OPTIONS`, `TUNNEL_OPTIONS`, any `serde` projection, any `Debug` output,
any error reason, any evidence file.

## Findings by severity

| Severity | Finding | Disposition |
|---|---|---|
| high | `edit` re-sealed the Plan 342 outproxy credential (pre-existing) | **fixed**; `plan380_an_unrelated_edit_leaves_the_sealed_credential_unchanged`, guard §5d-bis |
| high | `edit` re-sealed/ mis-parsed the Plan 380 credential | **fixed**; same guard |
| medium | `edit` cannot remove a credential | documented limitation, pinned by a row, remedy `delete` + `create` |
| medium | `plan292_idle_sweep_closes_quiet_tunnels` is wall-clock racy, 500 ms margin | **not fixed** — pre-existing, unrelated, reproduced on unmodified `HEAD`. Needs its own plan-of-record |
| low | `ExtensionError::NotAnObject` initially conflated the untyped blob with a malformed namespace | fixed during this pass by splitting `Untyped` / `NamespaceNotAnObject`, so a malformed extension is not reported as an untyped one |
| low | `proposal_tunnel_matrix.rs`'s `CustomOptions` owner text said only "fail-closed rejection of untyped custom values" | updated to name both halves, so the row cannot later be read as "`CustomOptions` is refused" |
| info | the credential is i2pr-local configuration and adds no support surface | recorded in `specs/CONFORMANCE.md`; `support.toml` deliberately unchanged |

## Roadmap disposition and unblock audit

- **Plan 380 → passed.**
- **Plan 374 — still blocked**, and Plan 380 does not unblock it. Its blocker is
  the **external ELS2 live driver**, which is orchestration, not production code.
  What changed is that the *only production gap* behind its authorization rows is
  now closed, so Plan 381's matrix has something to drive against instead of
  rows that cannot be written.
- **Plan 375 — still blocked** on its own two gaps (no Java source proof at the
  pin for the ELS2 auth vocabulary, no Java-direction driver). Unchanged by this
  plan.
- **Plan 377 — still cannot converge**; both inputs remain absent.
- **Plan 378 — still blocked on 377.** §1/§2/§5 were executed live and green in
  its own closure; §3/§4 still await Plan 377.
- **Plan 381 — unblocked as the remaining external driver scope.** It should
  reference this plan as a closed hard dependency, exactly as Plan 380's handoff
  intended.
- No capability promotion, no support surface added, no frozen Proposal inventory
  touched.

## What remains unproven after this plan

- **No live cross-router `.b33` fetch.** Every row here is in-process: the
  publisher, the record and the owner are all driven directly, because what is
  under test is the wiring and the policy. That is Plan 374's job and it is still
  blocked. Nothing in this record substitutes for it.
- **No Java I2P direction**, and no i2pd direction. Plan 375 and Plan 381.
- **No daily rollover re-resolution.** The blinding rotates daily and there is
  no periodic re-resolution, so a resolution computed before a midnight boundary
  addresses the wrong DHT key. `a_record_from_another_day_is_refused` pins the
  failure that makes the limitation visible; it is not a fix.
- **`edit` cannot remove a credential.** `delete` + `create` is the only way, and
  an operator who assumes otherwise will be wrong.
- **The credential is not transferable.** A data directory copied to another
  router cannot be recovered. That is the intended property, and it is also why
  a backup is not a credential backup.

## Handoff

Plan 381 (Plan 374's external driver scope) may now assume the i2pr side can
present PSK and DH credentials to a `.b33` service. What it still has to build is
the `R` non-floodfill service role, the lane configuration profile that enables
SAM/I2CP/HTTP, loopback application payloads on both sides, the negative rows,
and the evidence artifact and its checker. `tests/integration/els2/reference-freeze.md`
is its input, not this plan's evidence.