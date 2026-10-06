# Plan 361 — ADR-number uniqueness guard

Status: **registered-known-planning-coverage-gap**

Classification: **invariant** (a static guard for a property that must always hold).

Subsystem: `workspace-foundation` (planning and tooling authority). Parallel to Plan 319, which
established the plan-number side of this.

## The gap

`plans/global-number-collision-ledger.md` records it in its own words:

> Known coverage gap: no equivalent guard exists for ADR numbers.

Plan numbers are guarded by `scripts/check-global-plan-number-uniqueness.py`, exercised by
`tests/planning/test_global_plan_number_uniqueness.py`. **ADR numbers are not guarded at all.**

This is not hypothetical. `docs/adr/` currently holds **two `0030-*` records, both `Accepted`**,
which makes ADR 0029's "partially superseded by ADR 0030" ambiguous — a reader cannot tell which
document is meant. `docs/adr/` also holds duplicate `0032-*` and `0033-*` pairs, documented in the
collision ledger's ADR section.

So the gap has already produced a real, live ambiguity. This is the "verified, do not rediscover"
category.

## Why ready

- **No hard dependency.** The ADR tree is stable; this only reads it.
- **No interface dependency, no new dependency, no production change.** A Python script under
  `scripts/` plus a unittest under `tests/planning/`.
- It runs in the existing floor next to the plan-number check, so it costs one fast step.

## Objective

An ADR number is claimed by exactly one file in `docs/adr/`. Adding an ADR whose number is already
taken fails the floor before it can be merged, naming both files.

## In scope

1. **`scripts/check-adr-number-uniqueness.py`**, modelled on the existing plan-number checker: same
   exit-code discipline, same failure message style, run with `python3` (not `bash` — `bash`
   garbles these scripts and exits 2).
2. **Add it to the `AGENTS.md` routine floor.**
3. **A `tests/planning/test_*.py` suite** proving the negative case fails *for the right reason*,
   with fixture files in a temp dir — never by mutating the real `docs/adr/`.
4. **Encode the existing, already-known exceptions.** `0030`, `0032`, and `0033` each have two
   documents today. The guard must fail closed on a **new** duplicate while tolerating exactly these
   known ones, and the tolerated set must be visible in the script and cross-referenced to
   `plans/global-number-collision-ledger.md`. A silent skip-list is how a guard rots; a
   documented, enumerated, ledger-linked one is not.
5. **Close the ledger's own gap note** so the document stops advertising a hole that no longer
   exists.

## Out of scope

- **Renumbering any existing ADR.** Accepted ADRs are never renumbered without a plan-of-record, and
  renaming would break every reference across `plans/`, `specs/`, and code comments. The duplicate
  `0030` pair in particular has live supersession semantics.
- **Resolving the `0030` ambiguity itself.** That is a content question about what 0029 meant, and
  belongs to an ADR-level decision, not a tooling plan.
- **Checking plan-number uniqueness** — already guarded, and Plan 361 must not weaken it.
- **Validating ADR content, status, or supersession chains.** Out of scope; this guards identity
  only.

## Invariants

- **Fails closed.** An unparseable or unexpected ADR filename must be an error, not a silent skip.
- **No ADR is renumbered, moved, or deleted by this plan.**
- **The tolerated-duplicate set is explicit, enumerable, and ledger-linked** — never a glob, never a
  silent continue, never an unlogged exemption.
- **Adding an exemption requires editing the script**, so it shows up in review.
- No new dependency. Python 3 standard library only, matching the existing checker.

## Required evidence

- The guard passes on the real `docs/adr/` today (with the three known duplicates encoded).
- Introducing a duplicate number in a temp fixture **fails**, naming both files.
- A malformed filename **fails**, rather than being skipped.
- Removing an entry from the tolerated set makes the real tree fail, proving the set is load-bearing
  and not decorative.
- `python3 scripts/check-global-plan-number-uniqueness.py` still passes.
- `tests/planning` still green.

## Production changes

New `scripts/check-adr-number-uniqueness.py`; new `tests/planning/test_adr_number_uniqueness.py`;
one new line in the `AGENTS.md` floor; one edited line in `plans/global-number-collision-ledger.md`.

## Documentation updates

- `plans/global-number-collision-ledger.md` — replace the "known coverage gap" note with the fact
  that the gap is closed and how the three known duplicates are handled.
- `plans/README.md` — if it describes the plan-number check as the only uniqueness guard.

## Acceptance criteria

Plan 361 passes only when:

1. the guard exists, is in the routine floor, and passes on the real tree;
2. it is **negative-tested**, with recorded transcripts showing a duplicate, a malformed name, and a
   removed exemption each failing;
3. the three known duplicates are enumerated explicitly and traceable to the ledger;
4. `check-global-plan-number-uniqueness.py` is untouched and still green;
5. no ADR file is renamed, moved, or deleted;
6. exact-head routine CI is green.

## Stop conditions

Stop and record a classified boundary if satisfying "fails closed" would require renaming an
existing ADR, or if the real `docs/adr/` contains a naming convention this guard cannot parse
without guessing.

## Closure evidence required

Commits; requirement-to-evidence matrix; commands with local/CI outcomes labelled truthfully; the
negative-test transcripts; an explicit statement of what is **not** checked (content, status,
supersession); known limitations; findings by severity; roadmap disposition.

**Note on verification batching.** Executed alongside Plans 360, 362, and 364. Targeted verification
runs per plan; the full routine floor runs once on the combined tree, and the closure record must
say so rather than implying a per-plan floor run.