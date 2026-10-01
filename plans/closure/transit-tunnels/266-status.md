# Plan 266 — M11 receipt-family reference-completion opportunity generation corrective: status

Status:
**retained-m11-receipt-family-opportunity-ladder-landed-one-family-success-short-of-two-accepted-id-drop-scope-derived-corrective-required-via-plan267**

Plan 266 executed the contract it registered and proved its own
mechanism, but the `receipt` family closed 1/8 against the required 2
with zero i2pr semantic failures. Per Plan 266 §8 this plan does not
license a second budget increase: the first-unsatisfied-rung
distribution across the eight retained attempts is the evidence, and
Plan 267's scope derives from it. This record is that evidence.

## Disposition

- The ten-rung opportunity ladder, the one new declared terminal,
  the `--receipt-only` composer and its fixtures all landed with
  **zero production `crates/*/src` diff** from Plan 262 `514bf12`
  (re-proved mechanically on every one of the 8 runs).
- 8 retained `receipt` attempts, ordinals exactly 1..8, one
  qualification SHA `a9803ca2195221144d77282a058b6cfe9dabf3de`,
  every attempt classified from the closed 17-token vocabulary, no
  attempt beyond ordinal 8.
- **Zero i2pr semantic failures anywhere** (32 opportunity
  evaluations across Plans 265–266 with none contradicting i2pr).
- `receipt` **not closed**: 1 qualified success (attempt 2) against
  a required 2. First-unsatisfied-rung distribution: rung 1 ×2
  (setup stops), rung 4 ×2 (B never acted), rung 6 ×3 (B acted but
  never on a live counted id), rung 7 ×1 (success). Rung 5 fired
  nowhere: the creator advertisement stayed unobservable on all 8
  attempts and rung 6 decided throughout, exactly the specified
  fallback.
- The receipt-only composition gate was run **once** against the
  complete retained set plus the integrity-checked Plan 264 index
  and failed closed on exactly one violation
  (`family receipt: requires at least two retained successes,
  found 1`).
- The sharp derived fact (§Executions, anchored per attempt): in all
  three rung-6 attempts B addressed receive ids that were live in
  the accepted set, and every ingress on them dropped as
  unresolvable — while attempt 2's identical shape delivered and
  completed. The drop disposition (registration found-but-refused
  vs not-found) is not separable in the retained evidence, so
  Plan 267 owns it. ADR 0026 stays unpassed; M12 stays deferred; no
  public transit.

## Commits

- `a9803ca2195221144d77282a058b6cfe9dabf3de` — **qualification
  SHA**: the ladder predicate, the rung-5 terminal, the ladder
  evidence row, the receipt-only composer mode with seven new
  fixtures plus the static-guard mutation fixture, four new focused
  unit tests, and the boundary locks. Full local floor green before
  freezing (3153 passed, clippy/doc/deny clean, all 20 checkers).
- this commit — this record + Plan 267 registration + registry /
  roadmap / support / conformance / dossier reconciliation.

Pre-implementation HEAD: `0b0a0aecfd3d1c198430ecd0d9520cdcd81b780c9d`. A
diagnostic probe on the pre-qualification tree
(`/tmp/m11-266-probe-a1`, unmodified Plan 265 code, debug reference
logs) determined the rung-5 observability finding below; it is
diagnostic only and is not composed (different SHA, no verdict row
from the new code).

Reference: unmodified i2pd 2.61.0 @ `635b013a612ff47278ef02acf8580a28e10e26c5`
throughout.

## Implementation contents (WP A–C)

- `plan266_receipt_ladder_rung` (driver): rungs 1–3 setup stops
  with distinct reasons (`creator-counted-ibgw-id-unaccepted`,
  `b-outbound-i2pr-unproven`, `b-leaseset-resolution-unproven`),
  rung 4 the missing B action, rung 5 creator-side churn, rung 6
  the Plan 265 conjunction, rung 7 opportunity-present. Rung 5
  fires only when no addressed id is counted-live, so a
  counted-live action keeps its opportunity even if the creator
  rotated its advertisement. Covered by the shared static
  input-side guard on the same terms (matcher accepts wrapped
  signatures; proven by the new mutation fixture).
- `PLAN266_RECEIPT_CREATOR_LOST_ID =
  "receipt-creator-leaseset-lost-ibgw-id"`: vocabulary 16 → 17,
  member of the no-opportunity set, no existing terminal
  repurposed. Zero live hits in 8 attempts (recorded, not hidden).
- `plan266-ladder` sanitized row on every attempt:
  `first-unsatisfied=<rung>/creator-advertised-observable=<bool>/
  addressed=<n>/accepted=<m>` (counts only, no ids, no keys).
- `--compose-265 --receipt-only`: composes exactly eight fresh
  `receipt` manifests against the retained Plan 264 index and
  refuses every fresh `ibgw-data` / `participant-lifecycle` set.
- Rung-5 observability finding (WP B, empirical): A's log carries
  only A's local inbound ids (`Inbound tunnel <id> has been
  created`), never the gateway-side lease ids it advertises; B's
  log carries addressed ids (`Gateway of N bytes for tunnel <id>`)
  but no lease contents; no disk or API surface exposes the
  published LeaseSet. The rung therefore records unobservable on
  every attempt and rung 6 decides, per the plan's specified
  fallback — no production change was or is required.

## Plan 266 §9 acceptance criteria — requirement-to-evidence matrix

1. **`receipt` has ≥2 completions** — FAIL (1/8: attempt 2 only).
   Stop branch §8 fires instead; see §WP-D-derivation below.
2. **Exactly eight retained attempts 1..8, all classified** —
   PASS (mechanically re-verified above; composer ordinals check).
3. **Zero i2pr semantic failures** — PASS (no `present/fail`
   verdict in any manifest; composer semantic gate clean).
4. **Plan 265 evidence still integrity-checked, not re-executed**
   — PASS (receipt-only composer refused no non-receipt set was
   even supplied; retained Plan 264 digests verified inside the
   composition run; `ibgw-data`/`participant-lifecycle` untouched).
5. **Empty production diff** — PASS (every manifest carries
   `production_source_diff: []` against baseline `514bf12`).
6. **Complete local verification** — PASS (§Verification below).
7. **Exact-head ordinary CI** — see §CI below.
8. **Registry/roadmap/support/conformance/dossier agree** — PASS
   (this commit).
9. **No product/capability/version/public-network change** — PASS.
10. **No critical/high finding open** — PASS (highest is MEDIUM,
    owned by Plan 267).

Because criterion 1 fails, the §9 authority transition does **not**
fire. ADR 0026 stays unpassed and M12 planning stays deferred.

## Executions

All on qualification SHA `a9803ca…`, exact-pinned i2pd 2.61.0,
fresh mesh per attempt, disjoint ports/datadirs/roots
(`/tmp/m11-266-receipt-a1` … `-a8`).

| att | first-unsatisfied rung | terminal | semantic/external |
| --- | --- | --- | --- |
| 1 | 4 (B never acted; accepted=2) | `receipt-no-b-originated-self-action` | n/a / n/a |
| 2 | 7 (12 addressed, 1 accepted, ingress=27) | `receipt-tuple-bound-socket-verified` | pass / pass |
| 3 | 6 (54 addressed, 1 accepted, ingress=0) | `receipt-no-live-counted-ibgw-target` | n/a / n/a |
| 4 | 4 (B never acted; accepted=1) | `receipt-no-b-originated-self-action` | n/a / n/a |
| 5 | 1 (A accepted nothing; accepted=0) | `receipt-setup-stop` (`creator-counted-ibgw-id-unaccepted`) | n/a / n/a |
| 6 | 6 (99 addressed, 3 accepted, ingress=0) | `receipt-no-live-counted-ibgw-target` | n/a / n/a |
| 7 | 6 (12 addressed, 1 accepted, ingress=0) | `receipt-no-live-counted-ibgw-target` | n/a / n/a |
| 8 | 1 (A accepted nothing; accepted=0) | `receipt-setup-stop` (`creator-counted-ibgw-id-unaccepted`) | n/a / n/a |

Anchored forensics (B = the datagram-sending peer: only B sends in
this leg; A = the other peer; accepted = passing the driver's
delivery + active-count gates):

- a2 (success): B addressed exactly the one A-accepted id
  (`0x25a0e8bd`); 27 ingresses delivered over ~3.5 min; socket
  completed exactly once.
- a3: B addressed {`0x6350f66c`, `0xaaf95e7b`}; `0x6350f66c` was
  A-accepted and gate-passing, yet every ingress on it dropped as
  unresolvable (no next router/tunnel); `0xaaf95e7b` was never
  A-accepted (stale-id drops).
- a6: B addressed {`0xa2f61e0a`, `0xb561dc78`}; `0xa2f61e0a` was
  A-accepted ~1 min before sends began, yet all 96 ingresses on it
  across 4 send rounds / 8.4 min dropped as unresolvable; sibling
  A-accepted ids were never addressed; `0xb561dc78` was never
  accepted (stale-id drops).
- a7: B addressed 4 ids including A-accepted `0x4b0863b`
  (gate-passing); all ingresses on it dropped as unresolvable; the
  rest were never accepted (stale-id drops). (A accepted one
  further id that failed the active-count gate and was correctly
  never counted.)
- a5/a8: A accepted zero IBGW registrations all attempt, so no
  input boundary existed (rung 1, honest setup stops — Plan 265
  would have typed a5 as a no-opportunity absence).
- a1/a4: B emitted nothing addressable (a4 additionally shows one
  A-side seam observation that is correctly not B-attributed).

The retained ledgers cannot separate "registration found but
refused" (`LocalIbgwDropped`) from "registration not found"
(gateway `Dropped`): both render the same TSV shape. That missing
disposition is Plan 267's scope.

## Composition (WP C, run once)

```text
bash scripts/check-m11-per-epoch-composition.sh --compose-265 --receipt-only \
  --retained /tmp/m11-264f-obep-p1 /tmp/m11-264f-obep-p2 \
  /tmp/m11-264f-ibgw-p1 /tmp/m11-264f-ibgw-p2 \
  /tmp/m11-264f-part-p1 /tmp/m11-264f-part-p2 \
  /tmp/m11-264f-reject-p1 /tmp/m11-264f-reject-p2 \
  /tmp/m11-264f-obepdata-p1 /tmp/m11-264f-obepdata-p2 \
  -- /tmp/m11-266-receipt-a1 ... /tmp/m11-266-receipt-a8
```

Output: `family receipt: requires at least two retained successes,
found 1` plus `1 violation(s)`, rc=1. No retained-digest
complaint: the Plan 264 index verified. No second composition was
or will be run against this set.

## Verification (local, `a9803ca…`)

- Full floor before freezing: `cargo fmt --all --check`,
  `cargo check --locked --workspace --all-targets`,
  `cargo test --locked --workspace --all-targets --
  --test-threads=1` (3153 passed: 3149 retained + 4 new ladder
  tests), `cargo clippy ... -- -D warnings`,
  `RUSTDOCFLAGS="-D warnings" cargo doc`, workspace doc tests,
  all 20 boundary/evidence/checker scripts PASS (including the
  three M11 scripts with the new symbols, fixtures and
  receipt-only mode), ntcp2 lane unittest, `cargo deny` clean.
- Focused: all 20 retained `plan265_*` tests pass unchanged; the 4
  new `plan266_*` tests pass by exact name
  (`plan266_receipt_ladder_names_each_unsatisfied_rung`,
  `plan266_receipt_creator_lost_id_is_typed_no_opportunity`,
  `plan266_receipt_counted_live_action_keeps_opportunity_without_advertisement`,
  `plan266_receipt_unobservable_advertisement_falls_back_to_plan265`).
- `--check-input-side` passes (4 predicates, including the ladder).
- `--self-test` passes (13 retained + 8 new fixtures: receipt-only
  accept/refusals, miss-labeled-pass, rung-5-as-opportunity, nine
  attempts, mixed SHA, ladder mutation).
- `git diff --name-only
  514bf1237e86fde21e17fc98c743eb52852edd99..a9803ca --
  'crates/*/src'` is empty.

## Production-equivalence proof

Same mechanism as Plan 265 §8: the baseline is carried in-tree,
every manifest re-proves the empty diff, and the composer rejects
a non-empty diff. No production file was touched by the Plan 266
harness commit (4 files: driver test, 3 scripts).

## CI

See §CI evidence below (recorded in the follow-up commit, per the
Plan 258–266 precedent). Required: Quality (ubuntu-latest), Quality
(macos-latest), MSRV (Ubuntu), Dependency policy, on the exact
closure head.

## Security / resource / concurrency / migration review

- No user migration; no config/CLI/SAM/I2CP/RouterInfo/version/
  default change. Transit stays disabled and non-advertised.
- No secret retention: ladder rows carry counts only; the composer
  reads manifests only; no payload/key/id bytes added anywhere
  (tunnel ids already appear in precedented rows and are
  protocol-visible numbers, and the new row carries none).
- No new task/channel/queue/dependency; frozen Plan 262 numerics
  untouched (third lock still holds); no timeout/quota/size
  change; no reference patching; no public fallback.

## Findings by severity

- MEDIUM (owned by Plan 267): receipt accepted-id drops —
  live accepted ids addressed by B drop as unresolvable in 3/8
  attempts while the identical shape completes in 1/8. No i2pr
  semantic contradiction in 32 opportunity evaluations across
  Plans 265–266; the drop disposition is unobservable in retained
  evidence.
- LOW: rung-5 terminal has zero live hits (declared +
  fixture-tested, never observed — the fallback held 8/8).
- LOW: B-side LeaseSet staleness contributes stale-id drops
  alongside accepted-id drops (phantoms addressed in a3/a5/a6/a7);
  B never re-resolves within an attempt. Secondary signal for
  Plan 267's analysis, not its primary scope.
- LOW: rung-1 setup stops (2/8) are mesh-setup variance (A
  accepted nothing); rung-4 silences (2/8) are B-send variance.
  Retained as typed, not retried.

## Roadmap disposition

Plan 266 is retained-blocked with the ladder mechanism proven
(rung distribution recorded on 8 retained attempts, zero semantic
failures, composition failed closed on exactly the count rule).
Plan 267 is registered in the same commit and owns the derived
scope: receipt-family accepted-id drop disposition. M12 stays
deferred; ADR 0026 stays unpassed.

## WP-D derivation (why Plan 267 is scoped this way)

The distribution's dominant actionable signal is rung 6 (3/8),
and anchored forensics show every rung-6 attempt is an
*accepted-id* drop population: B addressed gate-passing accepted
ids and i2pr dropped them as unresolvable, while a2's identical
shape delivered. Rung 4 (B silent) and rung 1 (A accepted nothing)
carry no i2pr-side question; rung 5 is unobservable by lane
construction. The single missing fact that would separate
"registration found but refused" from "registration not found" —
and hence separate an i2pr-side lifetime/supersession window from
a B-side staleness window — is the drop disposition, which the
retained TSV shape cannot carry. Plan 267 therefore surfaces
exactly that disposition as sanitized diagnostic rows (no new
terminal, vocabulary stays 17) and re-executes the frozen ladder
budget against it. No other scope is derivable from this
distribution without guessing.

## Unblock audit

Audited `plans/registry.md` blocked work plus the transit-tunnels
roadmap dependency graph.

- Plan 267 (registered in this commit) lists Plan 266 as its sole
  hard dependency. Plan 266 now has an authoritative closure
  record and an integrity-checkable evidence set (8 manifests on
  `a9803ca`, retained index, fail-closed composition output), so
  that dependency is closed. Plan 267 has no other hard
  dependency and its interface dependency — manifest v5 + closed
  17-token vocabulary + ladder rows + receipt-only composition
  gate — is a stable written contract in the tree. Plan 267 is
  therefore **dependency-ready** and moves to `ready` in this
  commit.
- Every other registry row naming an M11 plan as a dependency is
  either `retained` with a named corrective that is now closed
  (Plans 258→259→260→261→262→263→264→265→266 chain) or names
  Plan 266 only as M12's deferral condition. M12 has no
  registered plan, so nothing becomes dependency-ready from it.
- No plan is unblocked that depends on the unmet `receipt`
  criterion. Nothing downstream of "M11 experimental
  qualification passed" moves; ADR 0026 is not marked passed.
