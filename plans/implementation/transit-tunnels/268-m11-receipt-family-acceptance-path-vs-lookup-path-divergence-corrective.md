# Plan 268 — M11 receipt-family acceptance-path vs lookup-path divergence corrective

Status at registration:
**registered-m11-receipt-family-acceptance-path-vs-lookup-path-divergence-corrective-ready**

Corrects:

- `plans/closure/transit-tunnels/267-status.md`: the `receipt`
  scenario family closed 0/8 against a required 2 on qualification
  SHA `315fb0d124ce6475139ce08d22086a51e4d2bb9c`, with **zero**
  i2pr semantic failures in all 8 Plan 267 attempts (40
  opportunity evaluations across Plans 265–267 with none
  contradicting i2pr).
- Plan 267 §7 / WP C: the "fewer than two qualified successes"
  branch, whose stated disposition is that the next scope derives
  from the disposition distribution rather than from a guess.

Retains in full:

- the Plan 265 fixed-budget contract, the Plan 266 ladder, and
  the Plan 267 disposition surface: manifest v5, the closed
  17-token terminal vocabulary, the three input-side opportunity
  predicates plus the ladder predicate, the quarantined
  diagnostic rows, exactly eight retained fresh-mesh attempts on
  one qualification SHA, no success-based early stop, no attempt
  nine, the composition gate with its fixtures, and the
  `--receipt-only` mode that refuses fresh `ibgw-data` /
  `participant-lifecycle` sets (those families stay closed on
  the Plan 265 evidence and are not re-executed);
- Plan 262 production semantics and authority: byte-identical
  `crates/*/src` tree at
  `514bf1237e86fde21e17fc98c743eb52852edd99`;
- the Plan 264 five-epoch retained evidence and its integrity
  index; the Plan 265 24-attempt evidence; the Plan 266 and Plan
  267 8-attempt evidence sets;
- exact-pinned, unmodified i2pd 2.61.0 at
  `635b013a612ff47278ef02acf8580a28e10e26c5`;
- loopback-only controlled qualification, fresh datadirs/ports/
  evidence roots, no public reseed/network fallback, no reference
  patching.

## 1. Objective

Close the one remaining M11 experimental-qualification row — two
tuple-bound exact-once creator-owned receipt completions — by
first recording which install path each acceptance took and then
executing the frozen ladder budget against it, without changing
production routing, without relaxing any frozen counting rule,
and without converting reference nondeterminism into
retry-until-green.

## 2. Why this scope derives from the Plan 267 distribution

The Plan 267 disposition distribution over 427 retained drop rows
is 208 `gateway-not-found` with a prior gate-passing acceptance,
210 `gateway-not-found` with none, and 9 `local-ibgw-refused`
with a prior acceptance. Three candidate scopes were falsified
or disfavored, leaving one:

1. *B-side snapshot staleness as primary scope* — falsified as
   primary: the fastest accepted-linked drops (+29 s, +57 s on
   fresh snapshots, e.g. 96 ingresses starting ~1 min after
   acceptance) cannot be stale snapshots, and only 1 row in 217
   falls under 30 s. Staleness explains the never-accepted
   phantoms, not the accepted population.
2. *Lookup-key divergence as primary scope* — disfavored but
   not falsified: a wrong key would drop immediately as well as
   late, yet fast drops are nearly absent (delays run minutes,
   p50 ~5.4 min). Divergence stays inside this plan as the
   not-found branch to eliminate.
3. *Registration lapse between acceptance and send* —
   consistent with the delay shape but not directly measured
   (production sweeps are silent to the harness).

The discriminating fact available *without production
instrumentation* is the acceptance install path: the driver
already sees `LiveBuildOutcome::CreatorBypass` vs
`::Dispatched` per build (it matches on both today; only the
dispatched arm records a build observation). Creator-bypassed
acceptances that never resolve for data would localize the
window to the bypass path — a production routing fact,
correctly escalated out of harness-only scope with a
production-scope plan required, never guessed. Dispatched
acceptances that lapse would localize it to lifetime
sequencing. This plan records the path and re-executes the
frozen budget against it.

## 3. Frozen invariants

### 3.1 Product

Identical to Plan 267 §3.1 (which inherits Plan 266 §3.1 and
Plan 265 §3.1): no production `crates/*/src` changes; ordinary
i2pr transit stays disabled; no RouterInfo capability or
`router.version` change; no public transit config option; no
wire-format change; no task/channel/queue addition; no quota,
timeout, message-size, retry-round, or admission-ceiling
inflation; no M12 implementation. Production internals may be
read for forensics, never modified for qualification.

### 3.2 Reference/network

Identical to Plan 267 §3.2. The reference stays **unmodified**:
no source patch, no vendoring, no synthetic LeaseSet, no
injected tunnel, no fake peer, no larger stimulus to widen the
window, no public network fallback. Sequencing observations are
not stimulus; adding rounds, cells, bytes, or attempts beyond
the frozen budget is.

### 3.3 Evidence and budget

- The Plan 265/266/267 contract is inherited unchanged,
  including the eight-attempt budget and the "no post-result
  adjustment of the counting rule" rule.
- The `ibgw-data` and `participant-lifecycle` families are
  **not** re-executed.
- Every dispatched `receipt` attempt is retained in the
  denominator with the same vocabulary, the same input-side-only
  opportunity rule, and the same receipt-only composition gate.
- The closed vocabulary gains **no** new terminal. The install
  path is a diagnostic row, never a terminal: the vocabulary
  stays 17 and no existing terminal's meaning changes.
- Opportunity classification remains decided from an input-side
  predicate, never from the downstream result. The install-path
  rows are downstream observations and must never feed any
  opportunity, semantic, or composition predicate (same static
  quarantine as the disposition rows).
- Raw reference logs remain diagnostic only.

## 4. Work packages

### WP A — acceptance install-path rows (harness only)

- Record the per-acceptance install path with the same
  remember-store pattern as the disposition store (bounded at
  the observation ceiling, fail-closed assert): for each
  gate-passing IBGW acceptance, whether the build outcome was
  gateway-`Dispatched` or `CreatorBypass` (plus the remaining
  outcome classes verbatim if any appear — currently only the
  catch-all `build-not-dispatched` exists downstream of the
  match).
- Emit sanitized `plan268-acceptance-path` rows at the receipt
  pre-gate fold (`path=<dispatched|creator-bypass|…>`,
  `receive`, `accepted-t`; counts and logical-ms only), joined
  against the disposition/timing rows by receive id.
- Extend the static quarantine with the new row symbols on the
  same terms (all ten predicate/classifier bodies plus the
  composer body). No new terminal; the composer vocabulary is
  untouched.

### WP B — fixed-budget execution and composition

- Freeze one new qualification SHA after the local floor.
- Execute exactly eight `receipt` attempts, ordinals 1..8, fresh
  mesh each, in the frozen order, on that SHA, with the ladder,
  disposition, and install-path rows active.
- Compose once with `--compose-265 --receipt-only` against the
  retained Plan 264 index. No composer change is expected; if
  the install-path rows require any composer change, that change
  is diagnostic-plumbing only and needs its own fixtures (it
  must not alter any accept/reject rule).

### WP C — closure

- Close only if the `receipt` family reaches ≥2 qualified
  successes with zero semantic failures, all eight attempts
  retained and classified, the retained Plan 264/265/266/267
  evidence still integrity-checked, and the local floor green.
- If it does not, the path-joined distribution is the evidence:
  bypass-accepted ids that never resolve escalate to a
  production-scope plan (stated explicitly, with the
  harness-measured correlation as its requirement); dispatched
  acceptances that lapse scope lifetime sequencing; either way
  the next plan acts on a measured path correlation, not on
  another blind budget.

## 5. Required focused tests

At minimum:

1. all 27 retained `plan265_*` + `plan266_*` + `plan267_*`
   tests still pass unchanged;
2. each install-path outcome maps to exactly one declared path
   value and never to a terminal;
3. a dispatched acceptance records `path=dispatched`, a
   bypassed one `path=creator-bypass`;
4. no install-path symbol appears in any opportunity or
   semantic predicate body (static test via the shared guard
   quarantine);
5. the composer still rejects a manifest carrying a path value
   as a terminal (unclassified-terminal rule);
6. the production-diff guard still reports an empty
   `crates/*/src` diff;
7. ordinary product transit remains disabled.

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
- the install path would require a production change to become
  observable (the driver already matches on both outcome
  classes; if the distinction collapses upstream, stop — do
  not instrument production for qualification);
- the path correlation proves the window is a production
  routing fact (bypass path): stop the harness-only sequence
  and register a production-scope plan instead of a further
  harness corrective;
- an attempt cannot be classified by the frozen vocabulary;
- reference patching, public fallback, timeout/quota/
  message-size enlargement, or a new dependency would be
  required.

If fewer than two qualified successes occur within eight
attempts and every opportunity-present input still passes
i2pr's semantics, this plan does not license a second budget
increase. The path-joined distribution across the eight
retained attempts is then the evidence, and the next scope
must be derived from it.

## 8. Acceptance criteria

1. `receipt` has ≥2 tuple-bound exact-once SAM completions on
   one qualification SHA.
2. Exactly eight retained attempts, ordinals 1..8, all
   classified from the frozen vocabulary.
3. Zero i2pr semantic failures.
4. The `ibgw-data`, `participant-lifecycle`, and Plan 265/266/
   267 receipt evidence is still integrity-checked and is not
   re-executed.
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

1. implement the install-path remember-store, rows, and
   quarantine extension;
2. add the focused tests;
3. run the complete local floor;
4. freeze one qualification SHA;
5. execute exactly eight `receipt` attempts;
6. compose once;
7. obtain exact-head ordinary CI;
8. close only if §8 passes exactly.

No production routing changes and no post-result adjustment of
the counting rule.
