# Plan 267 — M11 receipt-family accepted-id drop-disposition corrective

Status at registration:
**registered-m11-receipt-family-accepted-id-drop-disposition-corrective-ready**

Corrects:

- `plans/closure/transit-tunnels/266-status.md`: the `receipt`
  scenario family closed 1/8 against a required 2 on qualification
  SHA `a9803ca2195221144d77282a058b6cfe9dabf3de`, with **zero**
  i2pr semantic failures in all 8 Plan 266 attempts (32
  opportunity evaluations across Plans 265–266 with none
  contradicting i2pr).
- Plan 266 §8 / WP D: the "fewer than two qualified successes"
  branch, whose stated disposition is that the next scope derives
  from the first-unsatisfied-rung distribution rather than from a
  guess.

Retains in full:

- the Plan 265 fixed-budget contract and the Plan 266 ladder:
  manifest v5, the closed 17-token terminal vocabulary, the three
  input-side opportunity predicates plus the ladder predicate,
  exactly eight retained fresh-mesh attempts on one qualification
  SHA, no success-based early stop, no attempt nine, the
  composition gate with its fixtures, and the `--receipt-only`
  mode that refuses fresh `ibgw-data` / `participant-lifecycle`
  sets (those families stay closed on the Plan 265 evidence and
  are not re-executed);
- Plan 262 production semantics and authority: byte-identical
  `crates/*/src` tree at
  `514bf1237e86fde21e17fc98c743eb52852edd99`;
- the Plan 264 five-epoch retained evidence and its integrity
  index; the Plan 265 24-attempt evidence; the Plan 266 8-attempt
  evidence;
- exact-pinned, unmodified i2pd 2.61.0 at
  `635b013a612ff47278ef02acf8580a28e10e26c5`;
- loopback-only controlled qualification, fresh datadirs/ports/
  evidence roots, no public reseed/network fallback, no reference
  patching.

## 1. Objective

Close the one remaining M11 experimental-qualification row — two
tuple-bound exact-once creator-owned receipt completions — by
first making the drop disposition observable and then executing
the frozen ladder budget against it, without changing production
routing, without relaxing any frozen counting rule, and without
converting reference nondeterminism into retry-until-green.

## 2. Why this scope derives from the Plan 266 distribution

The Plan 266 first-unsatisfied-rung distribution is rung 1 ×2
(setup: A accepted nothing), rung 4 ×2 (B never acted), rung 6
×3, rung 7 ×1 (success). Rung 5 is unobservable by lane
construction. Rungs 1 and 4 carry no i2pr-side question. The
actionable signal is rung 6, and anchored per-attempt forensics
show every rung-6 attempt is an *accepted-id* drop population:

- a3: B addressed A-accepted gate-passing `0x6350f66c`; every
  ingress on it dropped as unresolvable (no next router/tunnel).
- a6: B addressed A-accepted gate-passing `0xa2f61e0a` 96 times
  across 4 send rounds / 8.4 min, starting ~1 min after
  acceptance; all 96 dropped as unresolvable; sibling A-accepted
  ids never addressed.
- a7: B addressed A-accepted gate-passing `0x4b0863b` among
  others; all ingresses on it dropped as unresolvable.
- a2: the identical shape (B addresses the one A-accepted id)
  delivered 27 ingresses over ~3.5 min and completed exactly
  once at the creator socket.

The retained ledgers cannot separate "registration found but
refused" (`ObepDeliveryOutcome::LocalIbgwDropped`) from
"registration not found" (gateway `Dropped`): both render the
same TSV shape (no next router/tunnel). That missing disposition
is the single fact that would separate an i2pr-side
lifetime/supersession window from a B-side staleness window, and
no other scope is derivable from this distribution without
guessing.

## 3. Frozen invariants

### 3.1 Product

Identical to Plan 266 §3.1 (which inherits Plan 265 §3.1): no
production `crates/*/src` changes; ordinary i2pr transit stays
disabled; no RouterInfo capability or `router.version` change; no
public transit config option; no wire-format change; no
task/channel/queue addition; no quota, timeout, message-size,
retry-round, or admission-ceiling inflation; no M12
implementation.

### 3.2 Reference/network

Identical to Plan 266 §3.2. The reference stays **unmodified**:
no source patch, no vendoring, no synthetic LeaseSet, no
injected tunnel, no fake peer, no larger stimulus to widen the
window, no public network fallback. Sequencing observations
(waiting, re-resolving, reordering existing rounds) are not
stimulus; adding rounds, cells, bytes, or attempts beyond the
frozen budget is.

### 3.3 Evidence and budget

- The Plan 265/266 contract is inherited unchanged, including the
  eight-attempt budget and the "no post-result adjustment of the
  counting rule" rule.
- The `ibgw-data` and `participant-lifecycle` families are **not**
  re-executed.
- Every dispatched `receipt` attempt is retained in the
  denominator with the same vocabulary, the same input-side-only
  opportunity rule, and the same receipt-only composition gate.
- The closed vocabulary gains **no** new terminal. The drop
  disposition is a diagnostic row, never a terminal: the
  vocabulary stays 17 and no existing terminal's meaning
  changes.
- Opportunity classification remains decided from an input-side
  predicate, never from the downstream result. The disposition
  rows are downstream observations and must never feed any
  opportunity, semantic, or composition predicate (locked
  statically: no disposition-row symbol may appear in a
  predicate body or the composer).
- Raw reference logs remain diagnostic only.

## 4. Work packages

### WP A — drop-disposition diagnostic rows (harness only)

- Surface which drop variant fired per gateway ingress as a
  sanitized typed row: `plan267-drop-disposition` with
  `disposition=<local-ibgw-refused|gateway-not-found>` plus the
  addressed id's accepted-and-gate-passing membership at drop
  time (`accepted-at-drop=true|false`, counts only). The outcome
  enums already distinguish the variants; only the row is new.
  No payload, key, or timing beyond the existing logical-ms
  clock.
- Record per-id accepted-vs-addressed timing as sanitized rows:
  for each addressed id, `accepted-t`, `first-addressed-t`, and
  the delivered/dropped counts on it. These are the facts the
  Plan 266 forensics reconstructed by hand; the lane must emit
  them mechanically.
- The disposition rows are diagnostic-only: statically forbid
  their symbols in every opportunity/semantic predicate body and
  in the composer (a disposition must never promote a drop into
  an opportunity or demote a success).

### WP B — fixed-budget execution and composition

- Freeze one new qualification SHA after the local floor.
- Execute exactly eight `receipt` attempts, ordinals 1..8, fresh
  mesh each, in the frozen order, on that SHA, with the ladder
  and disposition rows active.
- Compose once with `--compose-265 --receipt-only` against the
  retained Plan 264 index. No composer change is expected; if
  the disposition rows require any composer change, that change
  is diagnostic-plumbing only and needs its own fixtures (it
  must not alter any accept/reject rule).

### WP C — closure

- Close only if the `receipt` family reaches ≥2 qualified
  successes with zero semantic failures, all eight attempts
  retained and classified, the retained Plan 265/264/266
  evidence still integrity-checked, and the local floor green.
- If it does not, the disposition distribution across the eight
  retained attempts (found-but-refused vs not-found on
  accepted ids, with timing) is the evidence, and the next
  scope must derive from it: a refused-disposition points at
  registration lifetime/supersession instrumentation; a
  not-found-disposition points at acceptance-vs-lookup key
  divergence. Either way the next plan acts on a measured
  disposition, not on another blind budget.

## 5. Required focused tests

At minimum:

1. all 24 retained `plan265_*` + `plan266_*` tests still pass
   unchanged;
2. each drop variant maps to exactly one declared disposition
   value and never to a terminal;
3. a refused disposition on a gate-passing accepted id records
   `accepted-at-drop=true`;
4. a not-found disposition on a never-accepted id records
   `accepted-at-drop=false`;
5. no disposition-row symbol appears in any opportunity or
   semantic predicate body (static test via the shared guard
   vocabulary);
6. the composer still rejects a manifest carrying a
   disposition value as a terminal (unclassified-terminal
   rule);
7. the production-diff guard still reports an empty
   `crates/*/src` diff;
8. ordinary product transit remains disabled.

## 6. Exact verification floor

Plan 265 §13 unchanged, plus:

```text
bash scripts/check-m11-per-epoch-composition.sh --self-test
bash scripts/check-m11-per-epoch-composition.sh --compose-265 --receipt-only <evidence>
```

Every new focused test must be run by exact name.

## 7. Stop conditions

Stop and register a further narrow corrective only if:

- a `receipt` opportunity-present attempt produces any i2pr
  semantic contradiction;
- the drop disposition would require a production change to
  become observable (the outcome enums already carry it; if
  they do not, stop — do not instrument production for
  qualification);
- an attempt cannot be classified by the frozen vocabulary;
- reference patching, public fallback, timeout/quota/
  message-size enlargement, or a new dependency would be
  required.

If fewer than two qualified successes occur within eight
attempts and every opportunity-present input still passes
i2pr's semantics, this plan does not license a second budget
increase. The disposition distribution across the eight
retained attempts is then the evidence, and the next scope
must be derived from it.

## 8. Acceptance criteria

1. `receipt` has ≥2 tuple-bound exact-once SAM completions on
   one qualification SHA.
2. Exactly eight retained attempts, ordinals 1..8, all
   classified from the frozen vocabulary.
3. Zero i2pr semantic failures.
4. The `ibgw-data`, `participant-lifecycle`, Plan 265 receipt,
   and Plan 266 receipt evidence is still integrity-checked
   and is not re-executed.
5. `git diff --name-only 514bf1237e86fde21e17fc98c743eb52852edd99..HEAD -- 'crates/*/src'`
   is empty.
6. Complete local verification passes, including every new
   focused test.
7. Exact-head ordinary CI passes all four jobs.
8. Registry, roadmap, support inventory, conformance and the
   tunnel dossier agree on the final M11 disposition.
9. No product default, capability, version, or public-network
   change.
10. No critical/high finding remains open.

Only then may the unblock audit mark ADR 0026's one-family M11
experimental qualification passed and make M12 planning
dependency-ready. Public transit participation remains a
separate future decision.

## 9. Handoff order

1. implement the disposition diagnostic rows with their static
   predicate/composer quarantine;
2. add the focused tests;
3. run the complete local floor;
4. freeze one qualification SHA;
5. execute exactly eight `receipt` attempts;
6. compose once;
7. obtain exact-head ordinary CI;
8. close only if §8 passes exactly.

No production routing changes and no post-result adjustment of
the counting rule.
