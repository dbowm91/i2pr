# Plan 267 — M11 receipt-family accepted-id drop-disposition corrective: status

Status:
**retained-m11-receipt-family-drop-disposition-surfaced-no-family-success-accepted-id-not-found-dominates-path-divergence-scope-derived-corrective-required-via-plan268**

Plan 267 executed the contract it registered and delivered its
instrument: the drop disposition is now a measured fact on every
retained attempt. The `receipt` family closed 0/8 against the
required 2 with zero i2pr semantic failures. Per Plan 267 §7/WP C
this plan does not license a second budget increase: the
disposition distribution across the eight retained attempts is
the evidence, and Plan 268's scope derives from it. This record
is that evidence.

## Disposition

- The drop-disposition diagnostic surface landed with **zero
  production `crates/*/src` diff** from Plan 262 `514bf12`
  (re-proved mechanically on every one of the 8 runs): a bounded
  fail-closed disposition store on the qualification ledger, the
  closed 2-value disposition vocabulary, per-id accepted-vs-
  addressed timing rows, and the static quarantine that keeps
  every disposition symbol out of all ten predicates/classifiers
  and the composer (proven by the new self-test fixture and a
  throwaway mutation probe). No new terminal; vocabulary stays
  17.
- 8 retained `receipt` attempts, ordinals exactly 1..8, one
  qualification SHA `315fb0d124ce6475139ce08d22086a51e4d2bb9c`,
  every attempt classified from the closed vocabulary, no
  attempt beyond ordinal 8.
- **Zero i2pr semantic failures anywhere** (40 opportunity
  evaluations across Plans 265–267 with none contradicting
  i2pr).
- `receipt` **not closed**: 0 qualified successes against a
  required 2. First-unsatisfied-rung distribution: rung 4 ×3 (B
  never acted), rung 6 ×5 (B acted but never on a live counted
  id). No setup stops; rung 5 unobservable throughout.
- Disposition distribution over 427 retained drop rows: 208
  `gateway-not-found` with a prior gate-passing acceptance, 210
  `gateway-not-found` with none, 9 `local-ibgw-refused`
  (`unknown-id-or-expired`) with a prior acceptance. Accept-to-
  drop delays run minutes (not-found/accepted: min 29 s, p50
  ~5.4 min, max ~9.75 min; only 1 row under 30 s), which is
  inconsistent with a lookup-key divergence (that would drop
  immediately too) and consistent with lapse — except the
  fastest rows (+29 s, +57 s on fresh snapshots), which are not.
- The receipt-only composition gate was run **once** against the
  complete retained set plus the integrity-checked Plan 264
  index and failed closed on exactly one violation
  (`family receipt: requires at least two retained successes,
  found 0`).
- Derived scope (§WP-C-derivation): the dominant population is
  accepted-yet-not-found, so Plan 268 owns the
  acceptance-path vs lookup-path divergence (per-acceptance
  install path is already driver-visible in the outcome enums;
  no production instrumentation). ADR 0026 stays unpassed; M12
  stays deferred; no public transit.

## Commits

- `315fb0d124ce6475139ce08d22086a51e4d2bb9c` — **qualification
  SHA**: the disposition store + remember/filter methods, the
  two recording arms, the pre-gate diagnostic fold, the label
  and accepted-t pure functions, three new focused unit tests,
  the quarantine (predicate + composer-body scans), the
  disposition-as-terminal self-test fixture, and the boundary
  locks. Full local floor green before freezing (3156 passed,
  clippy/doc/deny clean, all 20 checkers).
- this commit — this record + Plan 268 registration + registry /
  roadmap / support / conformance / dossier reconciliation.

Pre-implementation HEAD: `ddc3106b45f5a3bd06112a618553566b698bc64`.

Reference: unmodified i2pd 2.61.0 @ `635b013a612ff47278ef02acf8580a28e10e26c5`
throughout.

## Implementation contents (WP A)

- `DropDisposition` (`epoch`, `receive_tunnel`, `refused`,
  `reason`, `logical_ms`) + `TypedLedger::drop_dispositions`
  (bounded at the observation ceiling, fail-closed assert) +
  `remember_drop_disposition` / `drop_dispositions_of`.
- Recording arms: `LocalIbgwDropped{receive_id, reason}` →
  `refused=true` with the verbatim secret-free reason class;
  gateway `Dropped{tunnel_id, ..}` → `refused=false`,
  `reason="not-found"`. The catch-all `Gateway(_)` arm records
  nothing (CreatorOwned/Disabled are not data drops and carry
  no addressed id).
- `plan267-drop-disposition` rows
  (`disposition/reason/receive/accepted-t/t`, counts and
  logical-ms only) and `plan267-addressed-id` rows
  (`receive/accepted-t/first-addressed-t/delivered/dropped`)
  folded at the receipt pre-gate from ledger facts, on every
  attempt that reaches the input boundary.
- `plan267_disposition_label` (closed 2-value vocabulary,
  unit-proven never-a-terminal in all five terminal sets) and
  `plan267_accepted_at_ms` (the exact counted-set gate plus a
  time bound; `None` claims nothing beyond "no acceptance
  existed then").
- Quarantine: `QUARANTINE_TOKENS` over 4 method predicates + 6
  free predicates/classifiers in the shared guard, plus a
  marker-delimited scan proving the composer body never names
  the disposition. A disposition can therefore never promote a
  drop into an opportunity, demote a success, or reach a
  count.

## Plan 267 §8 acceptance criteria — requirement-to-evidence matrix

1. **`receipt` has ≥2 completions** — FAIL (0/8). Stop branch
   §7 fires instead; see §WP-C-derivation below.
2. **Exactly eight retained attempts 1..8, all classified** —
   PASS (mechanically re-verified; composer ordinals check).
3. **Zero i2pr semantic failures** — PASS (no `present/fail`
   verdict in any manifest; composer semantic gate clean).
4. **Prior evidence still integrity-checked, not re-executed**
   — PASS (receipt-only mode; retained Plan 264 digests
   verified inside the composition run; Plan 265/266 families
   untouched).
5. **Empty production diff** — PASS (every manifest carries
   `production_source_diff: []` against baseline `514bf12`).
6. **Complete local verification** — PASS (§Verification below).
7. **Exact-head ordinary CI** — PASS (§CI below: run `36806382962` on `3fc1747`, all four jobs success).
8. **Registry/roadmap/support/conformance/dossier agree** — PASS
   (this commit).
9. **No product/capability/version/public-network change** — PASS.
10. **No critical/high finding open** — PASS (highest is MEDIUM,
    owned by Plan 268).

Because criterion 1 fails, the §8 authority transition does
**not** fire. ADR 0026 stays unpassed and M12 planning stays
deferred.

## Executions

All on qualification SHA `315fb0d…`, exact-pinned i2pd 2.61.0,
fresh mesh per attempt, disjoint ports/datadirs/roots
(`/tmp/m11-267-receipt-a1` … `-a8`).

| att | rung | terminal | dispositions (refused/not-found×accepted?) |
| --- | --- | --- | --- |
| 1 | 6 (4 addressed, 3 accepted) | `receipt-no-live-counted-ibgw-target` | 3 refused/yes + 46 not-found/no |
| 2 | 6 (94 addressed, 2 accepted) | `receipt-no-live-counted-ibgw-target` | 3 refused/yes + 98 not-found/yes + 74 not-found/no |
| 3 | 4 (B silent, accepted=4) | `receipt-no-b-originated-self-action` | 1 not-found/yes (non-B-attributed whisper ingress; see note) |
| 4 | 4 (B silent, accepted=1) | `receipt-no-b-originated-self-action` | 1 not-found/yes (non-B-attributed whisper ingress; see note) |
| 5 | 6 (48 addressed, 3 accepted) | `receipt-no-live-counted-ibgw-target` | 3 refused/yes + 95 not-found/yes |
| 6 | 4 (B silent, accepted=1) | `receipt-no-b-originated-self-action` | — |
| 7 | 6 (93 addressed, 4 accepted) | `receipt-no-live-counted-ibgw-target` | 13 not-found/yes + 90 not-found/no |
| 8 | 6 (13 addressed, 2 accepted) | `receipt-no-live-counted-ibgw-target` | no drop rows: 30 seam-delivered / 0 dropped (see note) |

Note: the rung-4 attempts emitted no counted addressed set, so
their disposition folds are near-empty by construction. a3 and a4
each carry one not-found/accepted whisper row from a
non-B-attributed ingress (A-side maintenance traffic hitting
the seam on an accepted id and resolving nothing — a3's id had
been accepted ~58 s earlier) — consistent with the rung-6
population, not with silence. a8 is the third
sub-shape: its 13 addressed ingresses (2 accepted-linked, 3
stale) all *delivered* at the seam (30 delivered / 0 dropped)
but never counted-large, so rung 6 fired with zero drop rows.
Delivery without large-live inputs is a distinct, equally
honest classification from drops, and Plan 268's per-id timing
rows already separate the two mechanically.

The per-id timing rows on every rung-6 attempt bind each
addressed id to its earliest gate-passing acceptance (or
`none`), first-addressed time, and delivered/dropped counts —
the facts the Plan 266 forensics reconstructed by hand, now
emitted mechanically.

## Composition (WP B, run once)

```text
bash scripts/check-m11-per-epoch-composition.sh --compose-265 --receipt-only \
  --retained /tmp/m11-264f-obep-p1 ... /tmp/m11-264f-obepdata-p2 \
  -- /tmp/m11-267-receipt-a1 ... /tmp/m11-267-receipt-a8
```

Output: `family receipt: requires at least two retained successes,
found 0` plus `1 violation(s)`, rc=1. No retained-digest
complaint: the Plan 264 index verified. No second composition was
or will be run against this set.

## Verification (local, `315fb0d…`)

- Full floor before freezing: `cargo fmt --all --check`,
  `cargo check --locked --workspace --all-targets`,
  `cargo test --locked --workspace --all-targets --
  --test-threads=1` (3156 passed: 3153 retained + 3 new
  disposition tests), `cargo clippy ... -- -D warnings`,
  `RUSTDOCFLAGS="-D warnings" cargo doc`, workspace doc tests,
  all 20 boundary/evidence/checker scripts PASS (including the
  quarantine and the new fixture), ntcp2 lane unittest,
  `cargo deny` clean.
- Focused: all 24 retained `plan265_*` + `plan266_*` tests pass
  unchanged; the 3 new `plan267_*` tests pass by exact name.
- `--check-input-side` passes (4 predicates + 10
  quarantine-scanned functions).
- `--self-test` passes (20 retained + 1 new fixture).
- `git diff --name-only
  514bf1237e86fde21e17fc98c743eb52852edd99..315fb0d --
  'crates/*/src'` is empty.

## Production-equivalence proof

Same mechanism as Plans 265–266: the baseline is carried
in-tree, every manifest re-proves the empty diff, and the
composer rejects a non-empty diff. The Plan 267 harness commit
touches 3 files (driver test, 2 scripts); production sources
are untouched. Production internals cited in §WP-C-derivation
were read, never modified.

## CI

Ordinary push CI was run on the **exact closure head**
`3fc174783cfd916c6887c3f78a51fe67d61beffa`. All four required
jobs pass:

| Run | Head | Required jobs | Result |
| --- | --- | --- | --- |
| `36806382962` | `3fc1747` | Quality (ubuntu-latest), Quality (macos-latest), MSRV (Ubuntu), Dependency policy | **success** on all four |

`https://github.com/dbowm91/i2pr/actions/runs/36806382962`

Recorded in the follow-up commit after the GitHub credential was
restored. The credential expiry that blocked retrieval was a
local hygiene gap only: it changed no executed evidence, and the
closure was already final and pushed. The manually-dispatched
`m11-transit-external.yml` lane records a pre-existing zero-job
push-triggered failure on every head in this series and is not a
required ordinary-CI job.

## Security / resource / concurrency / migration review

- No user migration; no config/CLI/SAM/I2CP/RouterInfo/version/
  default change. Transit stays disabled and non-advertised.
- No secret retention: disposition rows carry a reason class,
  counts, ids and logical-ms only. The reason classes are
  fixed secret-free vocabulary from the production outcome
  enums; tunnel ids are protocol-visible numbers already
  precedented in rows. No payload/key material anywhere.
- No new task/channel/queue/dependency; the disposition store
  is bounded at the observation ceiling with a fail-closed
  assert; frozen Plan 262 numerics untouched; no
  timeout/quota/size change; no reference patching; no public
  fallback.

## Findings by severity

- MEDIUM (owned by Plan 268): accepted-yet-not-found drops
  dominate (208/427 rows, plus 9 refused/accepted): gate-passing
  acceptances exist minutes before sends, yet gateway lookup
  resolves nothing. Zero semantic contradictions in 40
  opportunity evaluations across Plans 265–267, so this is a
  resolvability window, not an i2pr correctness verdict.
- LOW: refused/accepted rows (9, reason
  `unknown-id-or-expired`) — the registration resolved but
  forwarded nothing; lifetime/supersession signal for Plan
  268's secondary analysis.
- LOW: stale-id drops (210 not-found/never-accepted) — B's
  snapshot contains phantoms; B never re-resolves within an
  attempt. Secondary signal, not the primary scope.
- LOW: rung-4 silences (3/8) carry at most whisper rows; no
  i2pr-side question.

## Roadmap disposition

Plan 267 is retained-blocked with the disposition instrument
proven (427 drop rows with accepted-t timing on 8 retained
attempts, zero semantic failures, composition failed closed on
exactly the count rule). Plan 268 is registered in the same
commit and owns the derived scope: acceptance-path vs
lookup-path divergence. M12 stays deferred; ADR 0026 stays
unpassed.

## WP-C derivation (why Plan 268 is scoped this way)

Three candidate scopes were falsified by the measured
distribution, leaving one:

1. *B-side snapshot staleness as primary scope* — falsified as
   primary: the fastest accepted-linked drops (+29 s, +57 s on
   fresh snapshots, e.g. Plan 266 a6's 96-row series starting
   ~1 min after acceptance) cannot be stale snapshots, and
   only 1 row in 217 falls under 30 s. Staleness explains the
   never-accepted phantoms, not the accepted population.
2. *Lookup-key divergence as primary scope* — disfavored but
   not falsified: a wrong key would drop immediately as well
   as late, yet fast drops are nearly absent (delays run
   minutes, p50 ~5.4 min). Divergence remains possible for the
   fastest rows and stays inside Plan 268 as the not-found
   branch to eliminate.
3. *Registration lapse (expiry/supersession between acceptance
   and send)* — consistent with the delay shape but not
   directly measured (production sweeps are silent to the
   harness).

The discriminating fact available without production
instrumentation is the *acceptance install path*: the driver
already sees `LiveBuildOutcome::CreatorBypass` vs
`::Dispatched` per build. Creator-bypassed acceptances that
never resolve for data would localize the window to the
bypass path (a production routing fact, correctly escalated
out of harness-only scope with a production-scope plan
required — never guessed); dispatched acceptances that lapse
would localize it to lifetime sequencing. Plan 268 therefore
records the per-acceptance install path as sanitized rows
(same remember-store pattern, same quarantine, no new
terminal, vocabulary stays 17) and re-executes the frozen
ladder budget against it. No other scope is derivable from
this distribution without guessing.

## Unblock audit

Audited `plans/registry.md` blocked work plus the transit-tunnels
roadmap dependency graph.

- Plan 268 (registered in this commit) lists Plan 267 as its sole
  hard dependency. Plan 267 now has an authoritative closure
  record and an integrity-checkable evidence set (8 manifests on
  `315fb0d`, retained index, fail-closed composition output), so
  that dependency is closed. Plan 268 has no other hard
  dependency and its interface dependency — manifest v5 +
  closed 17-token vocabulary + ladder/disposition rows +
  receipt-only composition gate — is a stable written contract
  in the tree. Plan 268 is therefore **dependency-ready** and
  moves to `ready` in this commit.
- Every other registry row naming an M11 plan as a dependency is
  either `retained` with a named corrective that is now closed
  (Plans 258→…→267 chain) or names Plan 267 only as M12's
  deferral condition. M12 has no registered plan, so nothing
  becomes dependency-ready from it.
- No plan is unblocked that depends on the unmet `receipt`
  criterion. Nothing downstream of "M11 experimental
  qualification passed" moves; ADR 0026 is not marked passed.

## Correction cross-reference, 2026-10-05

The phrase "**zero production `crates/*/src` diff** from Plan 262
`514bf12` (re-proved mechanically on every one of the 8 runs)" above is
corrected by the dated correction in [`265-status.md`](265-status.md): the
guard's pathspec matched nothing, so it could not fail. The substantive
claim survives — the retained evidence SHA `6ab9dc2d` is re-proved
byte-identical to `514bf12` over crate production sources with a working
pathspec. Nothing in this plan's disposition changes.
