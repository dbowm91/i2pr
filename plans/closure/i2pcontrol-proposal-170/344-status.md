# Plan 344 — Plan 326 re-audit against the delivered crypto and publication work

Status: **passed-reaudit-found-and-closed-one-mode-coverage-gap-326-still-blocked-on-external-transcript**

Implementation commit: the corrective evidence pass, `crates/i2pr-daemon/src/service_els2.rs`
(two rows extended). Plan document:
[`344-plan-326-reaudit-against-delivered-crypto-and-publication.md`](../../implementation/i2pcontrol-proposal-170/344-plan-326-reaudit-against-delivered-crypto-and-publication.md)

## The headline

**Plan 326's recorded blocker was wrong, and one of its acceptance criteria was
being met on parser acceptance alone.**

The recorded blocker was `Plan 325's Red25519 provider qualification`. That
question was superseded for forward architecture by Plan 331, which **passed**.
Separately, the audit found that of the ten canonical `EncryptLeaseSet`
spellings, nine had a row of the class Plan 326 demands and **one — `encrypted
with per-user key (psk)` — had only ever passed at the parser**. Plan 326 says
outright: *"No mode may pass from parser acceptance or inert storage."* That gap
is now closed and inversion-proven.

Plan 326 nevertheless **remains blocked**, because its acceptance criteria
require external end-to-end verification and that is gated on Plan 335.

## Requirement-to-evidence matrix

Plan 326's acceptance criteria, quoted from its plan of record:

> "Plan 326 closes only when every Proposal LeaseSet mode has its exact
> implemented semantics and the encrypted-LS2 path works end-to-end through real
> publication and lookup."

> "No mode may pass from parser acceptance or inert storage."

| Requirement | Evidence | Result |
|---|---|---|
| **(a)** Plan 325's Red25519 provider question. | Plan 331 closure, `331-status.md`; Plan 325's own successor note ("Plan 331 ... supersedes this record for forward architecture only") | **Closed.** The provider question is answered by i2pr's own implementation qualified in 331, and 325's record defers to it. No qualified third-party provider is needed for the forward architecture; adopting one remains out of scope. |
| **(b)** All ten canonical modes have exact implemented semantics. | Mechanical census below; `service_els2.rs` rows | **Closed, after one gap was found and fixed.** All ten spellings now have an evidence row of the correct class. |
| **(c)** Control-plane publication evidence. | Plan 337 and Plan 338 closures; `i2pcontrol_els2_black_box.rs` | **Closed.** 338 diagnosed the "missing capability" as a **store-path mismatch** (`for_group` vs `for_service`), not a missing capability. Re-verified here: 5/5 black-box rows green. |
| **Exact implemented semantics, not parser acceptance.** | `every_applied_mode_builds_material_from_the_stored_identity`; `every_type5_mode_produces_a_real_type5_record_at_its_blinded_key` | **Closed for all applied spellings.** Each builds material from the real stored identity and produces a real type-5 record at its blinded key, with the wire flag bits taken from the protocol owner rather than a local guess. |
| **`encrypted (aes)` handled, not mislabelled.** | `plan334_els2_refused_modes_stay_refused_over_jsonrpc`; `proposal_leaseset_mode.rs:939` | **Closed by correct refusal.** Plan 326 required legacy AES to be researched and frozen separately rather than mislabelled as encrypted LS2. It resolves to `LegacyAes`, is never published, and is refused **by name** at both the contract layer and over JSON-RPC, with the refusal naming the mode. |
| **End-to-end real publication and lookup.** | — | **NOT met.** The external half is gated on Plan 335; see below. |
| **External lookup/publication against at least one independent implementation.** | — | **NOT met, and not measurable today.** See below. |

## The mechanical coverage census, and the gap it found

Census run over the canonical inventory
`PROPOSAL_ENCRYPT_LEASE_SET_VALUES` in `crates/i2pr-i2pcontrol/src/proposal_wire.rs`,
asking which spellings appear in the ELS2 *publication-path* rows rather than
only in parser rows:

| Canonical spelling | Parser row | Publication-path row (before) | Publication-path row (after) |
|---|---|---|---|
| `disable` | yes | n/a — ordinary, no material | n/a (covered by `disabled_mode_registers_no_service_and_mutates_nothing`) |
| `encrypted (aes)` | yes | by-name refusal | by-name refusal (unchanged) |
| `blinded` | yes | yes | yes |
| `blinded with lookup password` | yes | yes | yes |
| `encrypted (psk)` | yes | yes | yes |
| `encrypted with lookup password (psk)` | yes | yes | yes |
| **`encrypted with per-user key (psk)`** | **yes** | **NO — gap** | **yes (corrective)** |
| `encrypted with lookup password and per-user key (psk)` | yes | yes | yes |
| `encrypted with per-user key (dh)` | yes | yes | yes |
| `encrypted with lookup password and per-user key (dh)` | yes | yes | yes |

**Why this is a real gap and not a bookkeeping quibble.** The frozen mapping in
[`proposal-170-encryptleaseset-mode-mapping.md`](../../../specs/references/proposal-170-encryptleaseset-mode-mapping.md)
determines that the `per-user key` axis is a *user-interface* matter carried by
authorization-block entry count, not a protocol difference. One could therefore
argue the spelling is behaviourally covered by its twin. That argument is
exactly what Plan 326 forbids: the mapping is the *design*, not the *evidence*,
and the evidence must be the publication path. A spelling whose only evidence is
"its twin behaves the same way" is a spelling that could break — a mapping
regression, a client-auth path that special-cases by spelling, or a
publication-path bug reached only through that string — with every existing row
still green.

The corrective adds the spelling to **both** ELS2 rows, with **three** client
authorizations so it cannot pass by accident against the single- and two-client
cases either side of it.

## Teeth

| Inversion | Edit | Result |
|---|---|---|
| Per-user PSK mapping broken | Split `"encrypted with per-user key (psk)"` out of the `PreSharedKey` arm in `resolve_encrypt_lease_set_mode` and mapped it to `Blinded` | **Failed closed, and precisely.** `every_applied_mode_builds_material_from_the_stored_identity` and `every_type5_mode_produces_a_real_type5_record_at_its_blinded_key` both FAILED; the other five rows in the file stayed green. The two rows the corrective touched are exactly the two that failed. |

The narrowness of that result is the point: the new coverage is sensitive to the
spelling and to nothing else, so it is evidence about that spelling rather than
about the file it lives in. The inversion was reverted and both rows re-run
green.

The inversion used an asserted anchor and the edit was confirmed applied before
the run, so the failure is attributable to the mapping change rather than to a
silently no-opped edit.

## Verification run

| Command | Outcome |
|---|---|
| `cargo fmt --all --check` | pass |
| `cargo test --locked -p i2pr-daemon --lib` | 550 passed, 0 failed |
| `cargo test --locked -p i2pr-daemon --lib service_els2` | 7 passed, 0 failed; 5/7 green under the inversion |
| `cargo test --locked -p i2pr-daemon --test i2pcontrol_els2_black_box -- --test-threads=1` | 5 passed, 0 failed |
| `cargo clippy --locked -p i2pr-daemon --all-targets --all-features -- -D warnings` | pass |

All local, Linux. No hosted CI run backs this record. No external reference lane
was run, because the requirement it would serve is the one that cannot currently
be met.

**No production behaviour changed.** The commit is test coverage only.

## What remains, precisely

Plan 326's remaining gap is a single external one, and it is not a code gap:

> "external lookup/publication against at least one independent implementation
> before capability claim"

The type-11 outer signature is verified by the **floodfill** that accepts a
type-5 publication, and it is produced by the **publisher**. So the external
requirement fails in both directions:

- **External publication** — i2pr signs with the transcript the Red25519
  specification mandates; the pinned Java I2P and i2pd verifiers reject it.
- **External lookup** — the pinned references cannot produce a type-5 record
  i2pr will accept, for the same reason.

That is Plan 335, and it is a measured result, not an untested guess. The
Red25519 specification states the construction unambiguously — `H(x) :=
SHA-512("I2P_Red25519H(x)" || x)` with a `len_u16` prefix, and gives the reason
(RedDSA requires a length-extension-resistant instantiation that bare SHA-512
does not provide) — and i2pr's 10-vector fixture is a byte-exact transcription of
the specification's own published vectors, all of which pass. The divergence is
isolated to the hash construction: key conversion, derivation, and
re-randomization agree with both references.

So Plan 326's true blocker is **Plan 335's external transcript divergence**, and
no amount of local work removes it.

## Findings by severity

Critical 0. High 1. Medium 0. Low 1.

- **High — Plan 326's status token named a blocker that had been superseded.**
  The record said `awaiting-qualified-red25519-provider`, which pointed at
  Plan 325, whose own successor note defers to Plan 331 (passed). A reader
  following the token would have chased a closed question. **Corrected** by dated
  amendment in `326-status.md`; the token now names the real gate.
- **Low — one canonical mode had no publication-path row.** Found and fixed here;
  see the census.

## Roadmap disposition and unblock audit

Plan 326 stays **blocked**, with its token corrected to name Plan 335. Plans 330,
332, 333, 337, 338 are passed and are not reopened by this record.

Plan 328 (the full-conformance gate) remains blocked on 326 and 327. This record
does not move 328: closing 326 needs external evidence that Plan 335 blocks, so
328 cannot be unblocked by any purely local plan. That is worth stating plainly
rather than discovering later — the gate is reachable, but not from inside this
repository alone.

No other registered plan lists Plan 344 as a hard dependency, so nothing else
moves to ready from this closure.
