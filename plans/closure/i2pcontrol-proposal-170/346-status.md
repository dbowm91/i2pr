# Plan 346 — ELS2 type-11 transcript deployed-compatibility corrective: status

Status: **passed-els2-type11-deployed-profile-corrected-strict-primitive-unchanged-live-cross-router-still-plan347**

Implementation commit: `8386d22`. ADR:
[`0032-els2-type11-signature-profile-boundary.md`](../../../docs/adr/0032-els2-type11-signature-profile-boundary.md).
Plan of record:
[`346-els2-type11-transcript-deployed-compatibility-corrective.md`](../../implementation/i2pcontrol-proposal-170/346-els2-type11-transcript-deployed-compatibility-corrective.md).

## The headline

The policy error Plan 335 measured is corrected, and the independently implemented
Proposal-146 primitive is untouched. I2P has **two** written definitions of a signature
type 11 and **no wire discriminator** between them:

| | Proposal 146 / standalone Red25519 | Encrypted LeaseSet2 specification (deployed) |
|---|---|---|
| nonce/challenge | `HStar(m) = SHA-512("I2P_Red25519H(x)" ‖ T ‖ A ‖ len_u16(m) ‖ m)` | `r = SHA-512(T ‖ A ‖ m)`, `c = SHA-512(R ‖ A ‖ m)` |
| domain separator | yes | no |
| prefix-free length frame | yes | no |
| implemented by | i2pr (strict) | Java I2P, i2pd, since 2019 |

Java I2P and i2pd derive the same blinded public key as each other and as i2pr, and they
verify **each other's** type-11 signatures. They rejected i2pr's, and i2pr rejected theirs.
The disagreement was the transcript alone — which is exactly what this plan corrects.

**Measured result, both directions, against executed reference output** (not a source
reading):

| verifier \ signature | i2pr strict | i2pr deployed | i2pd | Java I2P |
|---|---|---|---|---|
| i2pr strict | ACCEPT | REJECT | REJECT | REJECT |
| i2pr deployed (ELS2) | REJECT | ACCEPT | **ACCEPT** | **ACCEPT** |
| Java / i2pd (plain Ed25519) | REJECT | **ACCEPT** | ACCEPT | ACCEPT |

The two cells marked ACCEPT in bold are what did not exist before this plan.

## Requirement-to-evidence matrix

| Plan 346 requirement | Evidence | Result |
|---|---|---|
| **1. Proposal-146 strict Red25519 byte-for-byte unchanged.** | `red25519_official_vectors.rs` (8 rows, all ten vectors, deterministic fields compare byte-exactly and both signature rows of every vector verify); `red25519_adversarial.rs`; `red25519_emissary_differential.rs`; `red25519_plain_ed25519_divergence.rs` (3); `red25519_java_reddsa_differential.rs` (4); `red25519_reference_differential.rs` (1). `scripts/check-els2-type11-transcript-boundary.sh` greps `HSTAR_PREFIX` and the length-frame `to_le_bytes` call so a future edit fails the guard. | **Closed.** No official-vector byte changed. |
| **2. A separate ELS2 deployed profile cross-verifies against both Java I2P and i2pd harnesses.** | `red25519_deployed_els2_profile.rs`: `the_deployed_profile_verifies_the_pinned_java_signature` (Java fixture, pinned `i2p/i2p.i2p@93eef5db87fae48025de00c0eb9b669e97b92149`), `the_deployed_profile_verifies_every_pinned_i2pd_signature` (all four i2pd cases, pinned `PurpleI2P/i2pd@2c694149fa6996eaeb23e378d5f83c9d3232c22f`), `an_i2pr_deployed_signature_verifies_under_a_plain_ed25519_verifier` (all four cases), `the_references_still_agree_with_each_other`. | **Closed.** |
| **3. Outbound type-5 records use the deployed profile.** | `els2_type11_profile.rs`: `a_locally_built_type5_record_uses_the_deployed_transcript_and_is_named_deployed`. Production path: `i2pr-client/src/encrypted_leaseset.rs` signs both the no-auth and authorized record builders through `sign_type11_deployed`; the strict guard in the boundary checker forbids `red25519::sign` in that file. | **Closed.** |
| **4. Inbound type-5 records accept deployed and strict only within the bounded ELS2 owner.** | `els2_type11_profile.rs`: `a_frozen_strict_form_record_remains_accepted_and_is_named_strict`, `the_two_profiles_are_reported_distinctly`. The owner is `i2pr_netdb::els2_transcript`; `i2pr_netdb::els2` records the outcome in `ValidatedEncryptedLeaseSet2::signature_profile`. The static checker asserts `red25519_deployed` has exactly one non-test production consumer. | **Closed.** |
| **5. Security / provenance / static guards prevent generic dual-transcript use.** | `scripts/check-els2-type11-transcript-boundary.sh` (added to the `AGENTS.md` routine floor) asserts: `verify_signature` keeps its non-type-7 guard; the crypto root does not re-export or call the deployed transcript; the strict module does not depend on the deployed one; the deployed module does not use `HSTAR_PREFIX`; the ELS2 publisher cannot use `red25519::sign`; `Els2SignedRegion` has no byte-slice constructor, no mutable access, and a private inner field; the profile API takes a typed region and never a `message: &[u8]`; only `Deployed` and `Strict` are accepting states. Runtime half: `the_common_signature_layer_has_no_type11_path`, `the_deployed_profile_rejects_a_strict_signature`, `the_strict_primitive_rejects_every_deployed_signature`. The checker was itself negative-tested: widening `is_accepted` to include `Ambiguous` makes it exit 1. | **Closed.** |
| **6. All no-auth / lookup-secret / PSK / DH publication rows remain green.** | `els2_foundation.rs` (25 rows) still green after moving its builders to the deployed profile; `els2_type11_profile.rs`: `every_authorization_mode_publishes_the_same_outer_profile`, `an_offline_delegation_follows_the_same_els2_type11_policy`, `a_strict_form_offline_delegation_remains_accepted`, `type7_offline_transient_ed25519_is_unaffected`. Client publish/resolve suites (`els2_publish_resolve`, `els2_authorized_publish_resolve`) and the daemon black-box suite green in the full workspace run. | **Closed.** |
| **7. Exact-head routine CI is green.** | Full `AGENTS.md` routine floor run locally on this branch at `8386d22`; see "Commands run" below. **No CI run was performed for this commit** — the repository has no CI trigger available in this environment, so every result below is labelled local, not CI. | **Closed locally; not claimed as CI.** |
| Inversion rows: routing the deployed helper through the generic strict API fails the intended compatibility rows. | `the_deployed_profile_rejects_a_strict_signature`, `the_strict_primitive_rejects_every_deployed_signature`, `the_common_signature_layer_has_no_type11_path`. | **Closed.** |
| Bounded security downgrade (plan §4). | See "Security review" below. Every listed compensating constraint is structural, not documentary. | **Closed.** |
| Clean-room provenance preserved (plan §5). | No Java I2P or i2pd source was copied, vendored, or patched. The deployed transcript is written from the Encrypted LeaseSet2 specification text. Reference material is limited to the already-committed signed-output fixtures. `git diff --stat` touches no third-party tree. | **Closed.** |
| "Describe type 5 as implemented but still non-advertised pending Plan 347." | `specs/support.toml` `common.leaseset2-family` and the Proposal-170 surface notes rewritten; `m12_type11_red25519` no longer says `type5-deferred`; `specs/CONFORMANCE.md` §"Red25519 (signature type 11) status" gained a two-transcript subsection and §"Encrypted LeaseSet2" status corrected. `advertised = false` unchanged. | **Closed.** |

## Commands run (all local, on this branch, at `8386d22`)

`cargo fmt --all --check`; `cargo check --locked --workspace --all-targets`;
`cargo test --locked --workspace --all-targets -- --test-threads=1` (exit 0);
`cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`;
`RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps`;
`cargo test --locked --workspace --doc`;
`bash scripts/check-dependency-direction.sh`;
`python3 scripts/check-global-plan-number-uniqueness.py`;
`python3 -m unittest discover -s tests/planning -p 'test_*.py'`;
`bash scripts/check-runtime-boundaries.sh`; `check-service-tunnel-boundaries.sh`;
`check-fixture-manifest.sh`; `check-ntcp2-vectors.sh`; `check-ssu2-vectors.sh`;
`check-i2cp-vectors.sh`; `check-ntcp2-interoperability.sh`;
`check-constrained-host-lane-boundary.sh`; `check-m11-transit-boundaries.sh`;
`check-m11-transit-qualification-evidence.sh`; `check-sam-acceptance-evidence.sh`;
`check-ssu2-acceptance-evidence.sh`; `check-i2cp-acceptance-evidence.sh`;
`check-i2pcontrol-acceptance-evidence.sh`; `check-service-tunnel-acceptance-evidence.sh`;
`check-exploratory-tunnel-evidence.sh`; `check-netdb-tunnel-evidence.sh`;
`check-destination-tunnel-evidence.sh`; `check-m6-mixed-router-acceptance-evidence.sh`;
`check-m12-floodfill-qualification-evidence.sh --self-test`;
`python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py'`;
`bash scripts/check-els2-type11-transcript-boundary.sh` (new);
`cargo deny check advisories bans sources` (`advisories ok, bans ok, sources ok`).

All green. Fixture bytes were not modified, so no `check-*-vectors.sh` re-measurement was
required beyond the runs above.

## Security review

Plan 346 §4 listed seven compensating constraints for the deployed transcript's missing
domain separator and length framing. Each is structural:

| Constraint | How it is enforced |
|---|---|
| The deployed signer/verifier accepts only the exact typed ELS2 signed region. | `i2pr_proto::Els2SignedRegion`, constructible only via `of_record` / `of_offline_keys`. The static checker greps the impl block for `new` / `from_bytes` / `from_slice` / `from_raw` / `as_mut`, for mutable access, and for construction outside `i2pr-proto`. |
| Callers cannot supply arbitrary application messages. | Same: the profile API signature is `region: Els2SignedRegion<'_>`, and the checker fails if a `message: &[u8]` parameter ever appears in it. |
| The type-5 store-type byte remains included. | `the_signed_region_is_exactly_the_record_region` asserts `region.as_bytes()[0] == 5`. |
| Message length is bounded before hashing. | `sign_type11_deployed` refuses a region above `MAX_ELS2_RECORD_LENGTH`; `red25519_deployed` independently caps at 65 534. Covered by `the_signer_enforces_the_els2_record_ceiling` and `the_deployed_profile_bounds_its_own_message_length`. |
| No transcript-selection input comes from the network or I2PControl. | There is no selector in the API. `sign_type11_deployed` takes no profile argument; `classify_type11` reports an outcome, it does not choose one. The Proposal-170 mode block never gained a transcript option. |
| No automatic downgrade/retry outside ELS2 verification. | The deployed module has exactly one production consumer, enforced statically; `verify_signature` has no type-11 path at all. |
| No logging of signatures with secret material; randomized signing still needs a CSPRNG. | `Red25519Signature`'s `Debug` prints only a length; `sign` takes an injected `TryCryptoRng` and reports `RandomnessUnavailable` rather than falling back, pinned by `a_failed_random_source_is_reported_rather_than_downgraded`. |

Additional security properties: no new dependency was added, so ADR 0005's table and the
`cargo deny` result are unchanged. The deployed module is `#![forbid(unsafe_code)]`-covered
by the crate root. Secret types are untouched — no `Clone` was added to any secret, and no
new `Debug`/`Display`/serde surface exists. An ambiguous transcript match is **rejected**,
not resolved, so a signature satisfying both equations can never be accepted.

## Migration and compatibility evidence

- **Records published under the previous strict policy remain readable.**
  `a_frozen_strict_form_record_remains_accepted_and_is_named_strict`, and the same for an
  offline delegation (`a_strict_form_offline_delegation_remains_accepted`).
- **i2pr does not publish strict records.** There is no strict signing path reachable from
  the ELS2 publisher; the checker fails if `red25519::sign` reappears in it.
- **The strict primitive is unchanged for every other consumer.** Destination, RouterInfo,
  SAM, and I2CP verification all route through `verify_signature`, which has no type-11 path
  and never had one.
- **No wire format changed.** The signed region, the type-5 framing, the offline block, the
  storage key, the address codec, and the layer cryptography are all byte-identical. Only the
  64 signature bytes differ, and only for records i2pr itself produces.
- **Persisted data is unaffected.** No key, seed, or stored record layout changed; a restart
  restores an unchanged identity, as before.

## Documentation and operational evidence

- ADR 0032 created; ADR 0005 and Plan 336 amended for the ELS2 use of type 11 only.
- `specs/CONFORMANCE.md`: §"Red25519 (signature type 11) status" gained a two-transcript
  subsection; §"Encrypted LeaseSet2" and §Proposal-170 EncryptLeaseSet corrected. Plan 335's
  measured-negative record is referenced as history, not rewritten.
- `specs/support.toml`: the `common.leaseset2-family` and Proposal-170 surface notes, and
  `m12_type11_red25519`, no longer describe type 5 as i2pr-only or provider-deferred.
  `advertised = false` unchanged everywhere.
- `docs/architecture/i2pr-crypto.md`, `i2pr-netdb.md`, `overview.md` updated; ADR 0032 linked
  from the netdb deep-dive.
- `crates/i2pr-daemon/src/service_els2.rs` module docs corrected so the code no longer
  claims the references cannot verify the transcript.
- `AGENTS.md` routine floor includes the new checker.

## Known limitations

1. **No live interoperability.** The evidence here is a **cryptographic-boundary** result.
   No stock router has been observed publishing, storing, looking up, decrypting, and using a
   type-5 record end to end in either direction. Plan 347 owns that.
2. **Type 5 remains non-advertised**, and no I2PControl capability, version, or RouterInfo
   claim was added.
3. **The deployed profile is a compatibility tradeoff, not equivalent security semantics.**
   It omits the explicit domain separator and the prefix-free length frame. The compensating
   constraints are recorded above and in ADR 0032; they bound the exposure but do not remove
   it.
4. **Recorded overlap, not a defect.** The deployed transcript's challenge hash
   `SHA-512(R ‖ A ‖ M)` is the plain Ed25519 challenge, so a deployed signature is also a
   valid Ed25519 signature and vice versa. What differs between the two type-11 transcripts
   is the **signing** transcript, not the verification equation. The deployed verifier is
   therefore *not* a stricter check than type 7 and no such claim is made anywhere. The
   protection that actually holds is that the ELS2 owner dispatches on the record's own
   `sigtype` field and refuses a type-5 record declaring a non-11 blinded sigtype before any
   transcript is consulted — pinned by
   `the_deployed_equation_overlaps_type7_and_the_boundary_is_the_sigtype_dispatch`. This is
   a property of the algorithms, not something this plan introduced, and it is why the
   sigtype dispatch rather than the equation is the named boundary.
5. **No CI run.** Every result above is local. The plan's "exact-head routine CI is green"
   criterion is satisfied in substance (the full floor passes) but not as a CI run; the
   environment exposes no CI trigger.

## Findings by severity

- **Critical:** none.
- **High:** none.
- **Medium:** none.
- **Low (recorded, accepted):** the deployed/strict verification-equation overlap described in
  limitation 4. Accepted with the sigtype dispatch as the named compensating boundary and a
  test pinning both halves.
- **Low (informational):** the `Ambiguous` state is currently unreachable — constructing a
  signature that satisfies both transcripts would require breaking SHA-512. It exists so the
  policy is fail-closed by construction rather than by the current absence of a counterexample.
  A test pins that it is never an accepting state.

No corrective pass is required.

## Historical records

Nothing was rewritten. Plan 335 remains the authoritative measured-negative result; this
record supersedes only its **forward interpretation** for ELS2 execution. Plan 336 remains
authoritative for standalone Proposal-146 Red25519. Plan 326 remains an immutable blocked
closure until Plan 347 supplies external end-to-end evidence. Plan 344's finding (one mode
that had only ever passed at the parser) is unaffected.

## Roadmap disposition and unblock audit

`plans/subsystems/red25519-encrypted-leaseset-roadmap.md` §8 and its §3 dependency graph are
updated: Plan 346 is `passed`, Plan 347 moves `blocked on 346` → `ready`.

**Unblock audit**, run per `plans/README.md` against every registered plan listing Plan 346 as
a hard or interface dependency:

| Plan | Declared dependency on 346 | Other hard deps | Interface contract stable? | Disposition |
|---|---|---|---|---|
| 347 | hard | none beyond 346 and its own reference freeze | yes — 346's output surface (`Els2Type11Profile`, `sign_type11_deployed`, `verify_type11`, ADR 0032) is written and enforced | **`ready`** |
| 348 | indirect, via 347 | **Plan 342 (not passed)**; Plans 339/340 passed | n/a | **stays blocked.** 346 passing discharges only the 347 leg; 348's own hard dependency on 342 is unmet and 342 is outside this plan's scope |
| 326 | via 347 (external evidence) | Plan 325 resolved by 331/346 | yes | **stays blocked**, correctly: 326 requires end-to-end external evidence, which is 347 |
| 348's Red25519/ELS2 rows | — | — | — | unchanged |

No other registered plan lists 346 as a dependency. No plan was silently unblocked.

**Passing Plan 346 does not close Plan 326 and does not claim cross-router Encrypted LeaseSet
interoperability.** It removes the policy defect that made interoperability impossible and
leaves Plan 347 as the sole remaining owner of the external evidence.
