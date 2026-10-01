# Plan 268 — M11 receipt-family acceptance-path vs lookup-path divergence corrective: status

Status:
**passed-m11-receipt-family-three-completions-zero-semantic-failures-path-divergence-falsified-pending-exact-head-ci-evidence**

The `receipt` family closed 3/8 against the required 2 with zero
i2pr semantic failures on one qualification SHA. Every Plan 268
§8 criterion is met with executed evidence except criterion 7
(exact-head ordinary CI run ID), which is unretrievable until the
local GitHub credential is re-authenticated (API returns HTTP
401; device-flow needs a user browser action). This closure is
therefore conditional: the family evidence is complete and the
planning rows are reconciled, but the §8 authority transition
(ADR 0026 one-family qualification passed; M12 planning
dependency-ready) does **not** fire until the CI follow-up
records the hosted run ID. No further family evidence is
required; only the hygiene gate is outstanding.

## Disposition

- The install-path diagnostic surface landed with **zero
  production `crates/*/src` diff** from Plan 262 `514bf12`
  (re-proved mechanically on every one of the 8 runs): the
  per-acceptance CreatorBypass-vs-Dispatched remember-store
  (bounded, fail-closed), `plan268-acceptance-path` rows joining
  each remembered path to the counted-set gate, the closed
  3-value path vocabulary (unit-proven never-a-terminal), the
  static quarantine extension, and the path-as-terminal
  self-test fixture. No new terminal; vocabulary stays 17. The
  CreatorBypass arm splits out of the former catch-all with a
  byte-identical observation push, so no existing row changed.
- 8 retained `receipt` attempts, ordinals exactly 1..8, one
  qualification SHA `cc9b40cab101acc63db135f217cbf2882c6010bc`,
  every attempt classified from the closed vocabulary, no
  attempt beyond ordinal 8.
- **Zero i2pr semantic failures anywhere** (48 opportunity
  evaluations across Plans 265–268 with none contradicting
  i2pr).
- `receipt` **CLOSED 3/8** (attempts 4, 5, 8): each rung 7 with
  gateway ingress, multicell emission, a bound six-field tuple,
  and exactly-once creator-socket delivery
  (`gateway-receipt-once`). First-unsatisfied-rung distribution:
  rung 6 ×5, rung 7 ×3. No setup stops; rung 5 unobservable
  throughout.
- Path correlation over 353 remembered install paths: **all
  `dispatched`, zero `creator-bypass`, zero `other`** — and zero
  catch-all (`build-not-dispatched`) build observations in any
  attempt. The counted IBGW acceptances flow through the pump
  site, which records Dispatched outcomes by construction; a
  bypass build at the pump would leave no observation at all,
  but it also could not become a counted acceptance (which
  requires a build observation). The CreatorBypass-vs-Dispatched
  divergence hypothesis is therefore **falsified for the
  observed population**: every acceptance that the lane could
  count installed through gateway dispatch.
- The receipt-only composition gate was run **once** against the
  complete retained set plus the integrity-checked Plan 264
  index and **passed**: `family receipt: 8 retained attempts, 3
  qualified successes`, rc=0.
- Remaining rung-6 population, mechanically separated by the
  Plan 267 rows: refused drops on accepted ids (a1/a2: 3 each,
  reason `unknown-id-or-expired`), one mixed refuse/not-found
  attempt (a7), and delivered-but-uncounted attempts (a3/a6:
  seam-delivered ingresses that never counted-large). No new
  scope is required to close the family: 3 ≥ 2 with zero
  semantic failures satisfies the frozen bar.
- ADR 0026 stays unpassed and M12 stays deferred **only**
  pending the CI follow-up (§CI). The family row itself is
  complete.

## Commits

- `cc9b40cab101acc63db135f217cbf2882c6010bc` — **qualification
  SHA**: the install-path store + methods, the CreatorBypass
  split arm, the pre-gate path fold, the closed path vocabulary,
  two new focused unit tests, the quarantine extension, the
  path-as-terminal fixture, and the boundary locks. Full local
  floor green before freezing (3158 passed, clippy/doc/deny
  clean, all 20 checkers).
- this commit — this record + registry / roadmap / support /
  conformance / dossier reconciliation. No successor plan is
  registered: none is derivable or required — the family is
  closed.

Pre-implementation HEAD: `2742925c04358e424a8ff7e93dfd8a86da5d2e5`.

Reference: unmodified i2pd 2.61.0 @ `635b013a612ff47278ef02acf8580a28e10e26c5`
throughout.

## Implementation contents (WP A)

- `AcceptancePath` (`epoch`, `receive_tunnel`, `path`,
  `logical_ms`) + `TypedLedger::acceptance_paths` (bounded at
  the observation ceiling, fail-closed assert) +
  `remember_acceptance_path` / `acceptance_paths_of`.
- Recording arms: `Dispatched(evidence)` →
  `path="dispatched"` with the evidence receive id;
  `CreatorBypass` → `path="creator-bypass"` with receive id 0
  (the bypass carries none — precisely the divergence fact)
  beside a byte-identical observation push; all other build
  outcomes keep the unchanged catch-all with no path entry
  (`path="other"` is declared for completeness and has zero
  hits).
- `plan268-acceptance-path` rows
  (`path/receive/t/gate-passing`) folded at the receipt
  pre-gate, joining each remembered path to the exact
  counted-set gate by (receive id, logical-ms).
- `PLAN268_PATH_VALUES = ["dispatched", "creator-bypass",
  "other"]`: closed, unit-proven disjoint from all five
  terminal sets.
- Quarantine: six new tokens over all ten predicate/classifier
  bodies plus the marker-delimited composer-body scan (proven
  by the new fixture; a throwaway mutation probe confirmed the
  predicate half fires).

## Plan 268 §8 acceptance criteria — requirement-to-evidence matrix

1. **`receipt` has ≥2 completions on one SHA** — PASS (3:
   attempts 4, 5, 8; genuine rung-7 completions, §Executions).
2. **Exactly eight retained attempts 1..8, all classified** —
   PASS (mechanically re-verified; composer ordinals check).
3. **Zero i2pr semantic failures** — PASS (no `present/fail`
   verdict in any manifest; composer semantic gate clean).
4. **Prior evidence still integrity-checked, not re-executed**
   — PASS (receipt-only mode: no non-receipt roots supplied;
   retained Plan 264 digests verified inside the passing
   composition run; Plan 265/266/267 families untouched).
5. **Empty production diff** — PASS (every manifest carries
   `production_source_diff: []` against baseline `514bf12`;
   `git diff --name-only 514bf12..cc9b40c -- 'crates/*/src'`
   is empty).
6. **Complete local verification** — PASS (§Verification below;
   tree unchanged since the freeze: `git status` clean,
   HEAD == qualification SHA at dispatch).
7. **Exact-head ordinary CI passes all four jobs** — PENDING
   (§CI: head pushed; run ID unretrievable until re-auth).
8. **Registry/roadmap/support/conformance/dossier agree** — PASS
   (this commit; disposition recorded as passed-conditional).
9. **No product/capability/version/public-network change** — PASS.
10. **No critical/high finding remains open** — PASS (highest is
    LOW; §Findings).

Criteria 1–6 and 8–10 pass with executed evidence. Because
criterion 7 is outstanding, the §8 authority transition does
**not** fire in this commit: ADR 0026 stays unpassed and M12
planning stays deferred until the CI follow-up records the
hosted run ID. That follow-up needs no new family evidence.

## Executions

All on qualification SHA `cc9b40c…`, exact-pinned i2pd 2.61.0,
fresh mesh per attempt, disjoint ports/datadirs/roots
(`/tmp/m11-268-receipt-a1` … `-a8`).

| att | rung (addressed/accepted) | terminal | install-path rows |
| --- | --- | --- | --- |
| 1 | 6 (8/1) | `receipt-no-live-counted-ibgw-target` | 41 dispatched |
| 2 | 6 (3/1) | `receipt-no-live-counted-ibgw-target` | 208 dispatched |
| 3 | 6 (10/1) | `receipt-no-live-counted-ibgw-target` | 29 dispatched |
| 4 | 7 (3/2) | `receipt-tuple-bound-socket-verified` ✓ | 2 dispatched |
| 5 | 7 (3/2) | `receipt-tuple-bound-socket-verified` ✓ | 21 dispatched |
| 6 | 6 (3/1) | `receipt-no-live-counted-ibgw-target` | 5 dispatched |
| 7 | 6 (8/2) | `receipt-no-live-counted-ibgw-target` | 13 dispatched |
| 8 | 7 (18/1) | `receipt-tuple-bound-socket-verified` ✓ | 34 dispatched |

All 353 remembered paths are `dispatched`; zero bypass, zero
other, zero catch-all builds. The counted IBGW acceptances
arrive via the pump site (Dispatched by construction); the
join shows gate-passing=false for every remembered row
because the remembered population is B-side/transit churn,
not the counted acceptances — which is itself the finding:
the install path is uniform, so resolvability varies
downstream of install (lifetime/lookup timing), not at it.

Success genuineness (a4/a5/a8): rung 7, `gateway-ingress`,
`multicell-bounded`, `full-tuple-bound`, verdict
`present/pass/pass/socket-verified`, `gateway-receipt-once` —
each an exact-once 1400-byte 0xA5 delivery at the creator
receiver SAM socket.

## Composition (WP B, run once)

```text
bash scripts/check-m11-per-epoch-composition.sh --compose-265 --receipt-only \
  --retained /tmp/m11-264f-obep-p1 ... /tmp/m11-264f-obepdata-p2 \
  -- /tmp/m11-268-receipt-a1 ... /tmp/m11-268-receipt-a8
```

Output: `family receipt: 8 retained attempts, 3 qualified
successes` plus the receipt-only summary line, rc=0. No
retained-digest complaint: the Plan 264 index verified. No
second composition was or will be run against this set.

## Verification (local, `cc9b40c…`)

- Full floor before freezing: `cargo fmt --all --check`,
  `cargo check --locked --workspace --all-targets`,
  `cargo test --locked --workspace --all-targets --
  --test-threads=1` (3158 passed: 3156 retained + 2 new path
  tests), `cargo clippy ... -- -D warnings`,
  `RUSTDOCFLAGS="-D warnings" cargo doc`, workspace doc tests,
  all 20 boundary/evidence/checker scripts PASS (including the
  quarantine extension and the new fixture), ntcp2 lane
  unittest, `cargo deny` clean.
- Focused: all 27 retained `plan265_*` + `plan266_*` +
  `plan267_*` tests pass unchanged; the 2 new `plan268_*`
  tests pass by exact name.
- `--check-input-side` passes (4 predicates + 10
  quarantine-scanned functions, disposition + path tokens).
- `--self-test` passes (21 retained + 1 new fixture).
- `git diff --name-only
  514bf1237e86fde21e17fc98c743eb52852edd99..cc9b40c --
  'crates/*/src'` is empty; working tree clean at handoff.

## Production-equivalence proof

Same mechanism as Plans 265–267: the baseline is carried
in-tree, every manifest re-proves the empty diff, and the
composer rejects a non-empty diff. The Plan 268 harness commit
touches 3 files (driver test, 2 scripts); production sources
are untouched. Production internals cited (outcome-enum
semantics, bypass-vs-dispatch paths, expiry sweeps) were read,
never modified.

## CI

Ordinary four-job CI is required on the exact closure head, with
the run ID recorded in a follow-up commit per precedent.
Status at handoff: the local GitHub API credential expired
mid-session (`gh` returns HTTP 401; the git-helper PAT is
git-scoped and returns 401 on the API; device-flow re-auth
needs a user browser action). Re-authenticate, confirm the four
required jobs (Quality ubuntu-latest, Quality macos-latest,
MSRV Ubuntu, Dependency policy) on the exact closure head,
record the run ID/URL here, and then fire the §8 authority
transition (ADR 0026 one-family qualification passed; M12
planning dependency-ready). No new family evidence is needed
for that follow-up.

## Security / resource / concurrency / migration review

- No user migration; no config/CLI/SAM/I2CP/RouterInfo/version/
  default change. Transit stays disabled and non-advertised.
- No secret retention: path rows carry a path value, tunnel
  ids, gate booleans and logical-ms only. No payload/key
  material anywhere.
- No new task/channel/queue/dependency; the path store is
  bounded at the observation ceiling with a fail-closed
  assert; frozen Plan 262 numerics untouched; no
  timeout/quota/size change; no reference patching; no public
  fallback.

## Findings by severity

- LOW: rung-6 refused drops on accepted ids persist in a
  minority shape (a1/a2: 3 each, `unknown-id-or-expired`) —
  correctly typed, no longer blocking.
- LOW: delivered-but-uncounted attempts (a3/a6) — seam
  deliveries that never counted-large; typed, separated
  mechanically, not retried.
- LOW: stale-id drops continue alongside (B never re-resolves
  within an attempt); typed, secondary.
- No MEDIUM or higher finding remains. The bypass-divergence
  hypothesis that motivated this plan is falsified with
  measured evidence (353/353 dispatched, zero bypass, zero
  catch-all builds).

## Roadmap disposition

Plan 268 closes the `receipt` family: 3/8 completions with zero
semantic failures, composed passing on one qualification SHA
with integrity-checked retained evidence. Status is
passed-conditional solely on the hosted CI run ID. No successor
plan is registered or required. M12 floodfill planning may
become dependency-ready in the CI follow-up; until then it
stays deferred and ADR 0026 stays unpassed.

## Unblock audit

Audited `plans/registry.md` blocked work plus the transit-tunnels
roadmap dependency graph.

- Plan 268 is the terminal corrective of the M11 receipt
  sequence: it lists Plan 267 as its sole hard dependency
  (closed, with an integrity-checkable evidence set) and has no
  other hard dependency. No further corrective is registered
  because none is derivable or required — the family meets the
  frozen bar (3 ≥ 2, zero semantic failures, composition
  passed).
- Every other registry row naming an M11 plan as a dependency
  is either `retained` with a now-closed corrective chain
  (Plans 258→…→268) or names Plan 268 only as M12's deferral
  condition. M12 has no registered plan: nothing becomes
  dependency-ready in this commit. The CI follow-up — and only
  it — fires the §8 transition (ADR 0026 passed; M12
  dependency-ready) without any new family evidence.
- No plan is unblocked that depends on the unmet criterion:
  criterion 7 (exact-head CI) is the single outstanding item,
  named with its exact remediation (re-authenticate, record
  run ID, fire transition).
