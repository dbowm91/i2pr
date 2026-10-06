# Plan 364 — correct the stale Plan-281 floor in `check-m12-floodfill-boundaries.sh`

Status: **registered-silent-failing-guard-in-neither-floor-nor-ci**

Classification: **invariant corrective** (a CI-enforced guard that no longer describes the tree).

Subsystem: `floodfill` — the script enforces an M12 boundary, and the drift was introduced by
Proposal 170 Plans 332/333/334.

## The defect

`scripts/check-m12-floodfill-boundaries.sh` **currently exits 1**. Verified 2026-10-06:

```text
crates/i2pr-netdb/src/lookup_engine.rs:660:        DatabaseStoreData::EncryptedLeaseSet(record) => {
crates/i2pr-netdb/src/lib.rs:65:        OwnerBlinding, ValidatedEncryptedLeaseSet2, day_bound_expiry_offset,
crates/i2pr-netdb/src/store_message.rs:84:        | DatabaseStoreData::EncryptedLeaseSet(_)
EncryptedLeaseSet type 5 is deferred and cannot enter server-authority NetDB storage
exit=1
```

The script still enforces Plan 281's "type 5 is deferred" floor. That floor was correct when
written and is now **false**: Plans 332/333/334 legitimately populate
`DatabaseStoreData::EncryptedLeaseSet`, and Plan 346 passed the corrected ELS2 transcript policy
(ADR 0032).

The guard is in **neither** the `AGENTS.md` routine floor **nor** `ci.yml`. So it fails silently —
nobody sees it, and its failure looks like content drift rather than a stale rule.

Two distinct harms, and the second is the serious one:

1. The rule is stale and wrong.
2. **The script's silence trains the reader to distrust the M12 boundary.** A guard that always
   fails is worse than an absent guard, because it converts "this boundary is enforced" into
   "this boundary reports something is wrong" — and the honest answer is that the *guard* is wrong.

`AGENTS.md` records this gap explicitly and instructs that it must **not** be added to the floor
until a plan corrects it. This is that plan.

## Why ready

- **No hard dependency.** The drift is already present; nothing must land first.
- **No production code change is expected.** If correcting the rule requires changing `i2pr-netdb`
  or `i2pr-daemon` to satisfy it, that is a finding for a separate corrective — not something to
  accommodate here.
- **No new dependency, no protocol change, no capability change.**

## Objective

`check-m12-floodfill-boundaries.sh` exits 0 on the current tree, and still exits 1 on the classes of
violation it was written to catch. Then it goes into the routine floor and `ci.yml` so it is no
longer silent.

## In scope

1. **Reconcile the type-5 rule with what the tree now legitimately contains.** Establish from the
   Proposal 170 closure records (332, 333, 334, 346) and `i2pr-netdb` what the *current* type-5
   boundary is, then assert **that** — not the superseded Plan-281 wording.
2. **Keep the guard meaningful, not merely green.** "Exit 0" is not the goal. The corrected rule
   must still forbid the things the M12 boundary forbids: unsolicited type-5 publication, server
   authority without the reviewed `fR` record, advertising a type-5 capability that is not qualified.
3. **Move it into the `AGENTS.md` routine floor** so it can no longer fail silently.
4. **Move it into `ci.yml`** (or state explicitly, with evidence, why it is deliberately excluded —
   but the current "in neither" state must end).
5. **A negative test per rule**, using temp fixtures, proving each corrected assertion still fails
   when its condition is deliberately broken.

## Out of scope

- **Reopening M12.** It stays stopped at Plan 306's bandwidth-tier boundary. Correcting a guard is
  not a capability claim.
- **Any change to type-5 wire behaviour, the ELS2 transcript, or publication policy.** Plans
  332/333/334/346 own those and they passed.
- **The M12 bandwidth-class design** that `registry.md` names as the reopen condition.
- **Adding the script to the floor before it is corrected** — the `AGENTS.md` prohibition is
  explicit, and adding it first would make the floor red for a reason unrelated to any change.

## Invariants

- **The corrected rule is derived from closure records and source, never from what makes the script
  pass.** If the honest rule is "type 5 is permitted in server-authority storage only under
  condition X", the assertion must encode X. Erasing the rule to get exit 0 is the failure mode this
  plan exists to avoid, and it is forbidden.
- **Type 5 stays non-advertised.** Guard correction must not become a capability promotion.
- **Every corrected assertion is negative-tested** with a fixture that fails for the right reason.
- No new dependency, no wire change, no advertisement change, no `ci.yml` weakening.

## Required evidence

- The script exits 0 on the current tree, with the corrected rule set listed and each rule traced
  to the closure record that establishes it.
- Each corrected assertion is shown to fail when broken.
- A deliberate re-introduction of the Plan-281 violation class (unsolicited type-5 publication)
  still fails the corrected guard.
- The script passes in the routine floor and in `ci.yml`.
- `check-floodfill-type5-serve.sh` (already in the floor) is unaffected and still green.

## Production changes

`scripts/check-m12-floodfill-boundaries.sh`; one added line in the `AGENTS.md` floor; one added job
or step in `.github/workflows/ci.yml`. **No `crates/` change** — if one is needed, stop and register
a corrective.

## Documentation updates

- `AGENTS.md` — remove the "currently exits 1 / not in floor / not in ci.yml" bullet from Known
  checker gaps, and record the corrected rule set in the floor note.
- `docs/architecture/tooling.md` — the M12 checker entry.

## Acceptance criteria

Plan 364 passes only when:

1. the script exits 0 on the current tree;
2. **it is still meaningfully strict** — every corrected assertion has a recorded negative test, and
   the Plan-281 violation class is still detected;
3. it is in the `AGENTS.md` routine floor **and** `ci.yml`, so it can no longer fail silently;
4. no `crates/` code changed;
5. type 5 remains non-advertised and M12 remains stopped;
6. exact-head routine CI is green.

## Stop conditions

Stop and record a classified boundary if the honest current type-5 boundary cannot be expressed as
a static rule over the source, or if making the script pass would require changing production
behaviour rather than correcting a stale assertion.

## Closure evidence required

Commits; requirement-to-evidence matrix; commands with local/CI outcomes labelled truthfully; the
before/after rule inventory with each change traced to its authority; the negative-test transcripts;
known limitations; findings by severity; roadmap disposition.

**Note on verification batching.** Executed alongside Plans 360, 361, and 362. Targeted verification
runs per plan; the full routine floor runs once on the combined tree, and the closure record must
say so rather than implying a per-plan floor run.