# Plan 500 — Status: the lookup engine's peer budget becomes load-bearing

## Status

**Closed** as `passed-lookup-peer-budget-enforcement`.

`LookupPolicy::max_peers_per_lookup` was declared, validated at construction
and exposed by an accessor — and **read by nothing**. The only reader in the
tree was an assertion inside the policy's own test module. The engine
therefore queried as many peers as the candidate list held (up to
`max_candidates_considered`, default 32) while the policy declared a tighter
per-lookup peer ceiling (default 8) that it never applied. Two distinct
ceilings were declared; only one was enforced.

That ceiling is now enforced. The bound lives in the engine's **single**
attempt site — the one place `start`, `start_with_candidates`,
`start_with_preferred` and the response-driven advance all emit an attempt
from — so it holds on **every** entry point rather than on one of them.

The bound narrows **attempts**, never **candidates**. Plan 489's pinned
candidate list and Plan 497's preference prefix are unchanged: the engine
still knows the same peers, it simply stops asking sooner.

Nothing was promoted. `specs/support.toml` stays at 94 surfaces (88
experimental, 5 deferred, 1 ready) with **0 advertised**; `router.version`
stays `0.9.62`; `caps` and the published RouterInfo are unchanged;
`specs/support.toml`, `Cargo.toml` and `Cargo.lock` are byte-identical.

**Plan 497 L2 is discharged.**

## Commits

| Commit | What |
| --- | --- |
| `6ca85304` | `docs(plans): register Plan 500, the lookup engine's peer budget` |
| `6b7fe052` | `feat(netdb): make the lookup engine's peer budget load-bearing` |
| `a578f461` | `docs(architecture): document the Plan 500 lookup peer budget` |
| `db0efbca` | `docs(plans): defer the Plan 500 closure-record citation to the closure commit` |
| (this commit) | `docs(plans): close Plan 500 and advance the network-participation roadmap` |

## What changed

### The typed terminal state

`crates/i2pr-netdb/src/lookup_action.rs`

- `LookupFinalState::PeerBudgetExhausted` (`:143`) — a **policy** ceiling on
  the number of peers queried, distinct from `PeerExhausted`, which is the
  **candidate list** running out.
- `LookupFinalState::label()` (`:160`) returns the stable label a receipt
  counts a state under: `"peer-exhausted"`, `"peer-budget-exhausted"`, … The
  `Display` impl delegates to it, so the same final state always lands in
  the same bucket.
- **A wrong doc was corrected while the vocabulary was touched.**
  `LookupFinalState::PeerExhausted` was documented as *"The lookup exhausted
  the peer budget without a usable `DatabaseStore`"* — the opposite of what
  the code does. It is the **candidate list** that runs out. The doc now
  says so, and names `PeerBudgetExhausted` as the state that means the
  budget.

### The engine's attempt site

`crates/i2pr-netdb/src/lookup_engine.rs`

- The budget check sits in `advance_attempt` (`:728-731`), **before** the
  peer is pushed onto the lookup's `queried` set and **before** the attempt
  counter moves. A candidate the budget prevented querying therefore stays
  unqueried, and the candidate list itself is untouched.
- `advance_attempt` is the single attempt site: `start` (`:303`),
  `start_with_candidates` (`:344`), `start_with_preferred` (`:418`) and the
  response-driven advance (`:629`) all route through it, so one guard covers
  every entry point.
- A `debug_assert` documents that the attempt counter and the `queried`
  set stay in step; the attempt counter is the authoritative figure.
- `ActiveLookup::pinned_candidates()` (`:185`) — a `#[doc(hidden)]`,
  test-only accessor that lets a row prove the candidate list is unchanged
  under the budget.

Nothing else in the engine moved: `select_floodfill_candidates`'s truncation
to `max_candidates_considered` is untouched, `start_with_candidates`'s
pinned-list semantics are untouched, and `start_with_preferred`'s
priority-prefix semantics are untouched. `start_with_candidates`'s own doc
comment already claimed *"the deadline, **the peer budget**, the duplicate
coalescing … all behave exactly as they do for [`Self::start`]"* — a claim
that was false until this change, and is now true.

### The daemon's routing receipt

`crates/i2pr-daemon/src/netdb_participation.rs`

- `RoutingReceipt.terminal_counts` (`:1198`) — the number of lookups that
  terminated in each final state, keyed by the state's stable label.
- `record_terminal` (`:1868`) counts a terminal state under
  `final_state.label()` and emits a `RoutingReceiptKind::Terminal` receipt.
- The two exhaustions therefore land in **separate buckets**:
  `peer-budget-exhausted` and `peer-exhausted` are different events with
  different causes and different remedies, and folding them into one bucket
  would make the peer budget invisible in the diagnostics.
- The receipt stays **counts-only**: no peer hash, no key, no endpoint, no
  payload.

## Requirement-to-evidence matrix

| Requirement (plan §4 / §7) | Evidence | Verdict |
| --- | --- | --- |
| §4.1 the budget narrows attempts, never candidates | `the_pinned_candidate_list_is_unchanged_under_the_budget`; the guard's placement before `queried.push` (`lookup_engine.rs:728-731`) | pass |
| §4.2 Plan 489's frozen candidate list is unchanged | `the_pinned_candidate_list_is_unchanged_under_the_budget` | pass |
| §4.3 Plan 497's preference prefix is unchanged | `the_preferred_path_still_reaches_a_peer_ingested_mid_lookup_under_the_budget` | pass |
| §4.4 the candidate ceiling is unchanged | `select_floodfill_candidates` untouched; the crate's own rows unchanged | pass |
| §4.5 the policy's existing validations stay load-bearing | `the_policy_still_refuses_a_zero_budget_and_a_budget_above_the_candidate_ceiling` | pass |
| §4.6 no lookup can exceed the budget | `the_peer_budget_stops_a_lookup_whose_candidate_set_is_larger_than_the_budget`; `the_budget_binds_on_every_entry_point` | pass |
| §4.7 the exhaustion is typed and distinguishable | `LookupFinalState::PeerBudgetExhausted` + `label()`; `a_lookup_within_the_budget_reports_the_candidate_exhaustion`; `netdb_peer_budget_receipt.rs` | pass |
| §4.8 no unbounded growth | the bound reads a value the policy already holds; no new allocation or structure | pass |
| §4.9 no dependency, no advertisement, no promotion | `Cargo.toml` / `Cargo.lock` / `specs/support.toml` byte-identical; `router.version` 0.9.62; `caps` unchanged | pass |
| §7 WP1 a distinct typed terminal state | `lookup_action.rs:143`, `:160` | pass |
| §7 WP2 the engine honours the budget at every attempt site | `the_budget_binds_on_every_entry_point` | pass |
| §7 WP3 the daemon's driver maps the exhaustion | `crates/i2pr-daemon/tests/netdb_peer_budget_receipt.rs` | pass |
| §7 WP4 the tests | 7 new netdb rows, 1 new daemon row | pass |
| §10 the engine's existing rows still pass | `cargo test -p i2pr-netdb --all-targets`: **420 passed, 0 failed** | pass |
| §13.1.8 no promotion | `git diff --stat -- specs/support.toml Cargo.toml Cargo.lock` empty | pass |
| §13.2 no stop condition triggered | the budget narrows attempts only; Plan 489's and Plan 497's semantics are unchanged; the default was **not** weakened; the driver's reason is distinct; nothing was promoted | pass |

## Commands run

### The routine floor (on the closing tree `db0efbca`)

**97 steps, 97 PASS, 0 FAIL.** Log: `/tmp/i2pr-floor-500-run1.log`;
completion marker `=== FLOOR COMPLETE 07:41:35 ===`.

The floor ran in full on the closing tree `db0efbca`
(`docs(plans): defer the Plan 500 closure-record citation to the closure
commit`). The slowest steps were `cargo-test-workspace` (1151 s),
`plan491-netdb-suite` (92 s), `plan498-bootstrap-resolver-row` (29 s) and
`plan491-client-suite` (26 s). Every checker script in the floor passed,
including `check-dependency-direction`, `check-runtime-boundaries`,
`check-console-boundaries`, `check-console-browser-security`,
`check-tooling-inventory`, `check-managed-app-process-boundary`,
`check-managed-app-package-boundary`, `check-managed-app-policy-boundary`,
`check-managed-app-secured-sandbox`, `check-service-tunnel-boundaries`,
`check-m12-floodfill-boundaries`, `check-outproxy-request-path` and
`cargo-deny` (advisories ok, bans ok, sources ok).

**No boundary script was weakened, relaxed or skipped.** The floor's
`cargo-test-workspace` step is the one that carries the new evidence: it
includes `lookup_peer_budget` (7 rows) and `netdb_peer_budget_receipt`
(1 row).

### Focused verification (before the floor)

| Command | Outcome |
| --- | --- |
| `cargo fmt --all --check` | PASS |
| `cargo check --locked --workspace --all-targets` | PASS |
| `cargo test --locked -p i2pr-netdb --all-targets --no-fail-fast` | PASS — **420 passed, 0 failed** (302 in-crate unit tests, 118 integration tests) |
| `cargo test --locked -p i2pr-daemon --all-targets --no-fail-fast -- --test-threads=1` | PASS — **2306 passed, 0 failed** (the 1 new row is `netdb_peer_budget_receipt`) |
| `cargo clippy --locked -p i2pr-netdb -p i2pr-daemon --all-targets --all-features -- -D warnings` | PASS — no warnings |
| `python3 scripts/check-tooling-inventory.py` | PASS — "every published figure matches the tree" |
| `python3 scripts/check-global-plan-number-uniqueness.py` | PASS |
| `python3 -m unittest discover -s tests/planning -p 'test_*.py'` | PASS — 51 tests, OK |
| link check over the edited docs | PASS — no broken relative links |
| `git diff --stat -- specs/support.toml Cargo.toml Cargo.lock` | empty — no promotion |

## Invariant review

1. **The budget narrows attempts, never candidates — preserved.** The check
   sits before `queried.push` and before the attempt counter moves, so a
   prevented candidate stays unqueried. The candidate set is byte-identical
   with and without the budget.
2. **Plan 489's frozen candidate list is unchanged — preserved.**
   `start_with_candidates` keeps its exact semantics: the caller's list is
   the candidate list, and the budget applies to how many of them are
   *queried*. **Proved** by
   `the_pinned_candidate_list_is_unchanged_under_the_budget`.
3. **Plan 497's preference prefix is unchanged — preserved.**
   `start_with_preferred` keeps its exact semantics: the preference is a
   priority prefix, and the store is re-selected on every advance. **Proved**
   by `the_preferred_path_still_reaches_a_peer_ingested_mid_lookup_under_the_budget`.
4. **The candidate ceiling is unchanged — preserved.**
   `select_floodfill_candidates`'s truncation to `max_candidates_considered`
   is not touched.
5. **The policy's existing validations stay load-bearing — preserved.** A
   zero peer budget is still `LookupPolicyError::ZeroPeers`; a peer budget
   above the candidate ceiling is still `PeerBudgetExceedsCandidates`.
   **Proved** by
   `the_policy_still_refuses_a_zero_budget_and_a_budget_above_the_candidate_ceiling`.
6. **No lookup can exceed the budget — preserved.** At most
   `max_peers_per_lookup` attempts are emitted, across every entry point.
   **Proved** by `the_budget_binds_on_every_entry_point`.
7. **The exhaustion is typed and distinguishable — preserved.**
   `PeerBudgetExhausted` is not reported as a candidate exhaustion, and not
   as a timeout. The daemon's receipt counts the two in separate buckets.
8. **No unbounded growth — preserved.** The bound adds no allocation and no
   new structure.
9. **No dependency, no advertisement, no promotion — preserved.**

## Failure review

- **The budget is spent.** The lookup terminates with
  `PeerBudgetExhausted`. It is not an error: it is a typed, counted final
  state, exactly like `PeerExhausted` and `NoEligibleCandidates`.
- **The candidate list runs out first.** Unchanged: `PeerExhausted`. **Note
  the boundary:** the budget fires only when a candidate is still eligible.
  A candidate set *equal* to the budget reports `PeerExhausted` — the last
  eligible candidate was queried — and a set *larger* than the budget
  reports `PeerBudgetExhausted`. That boundary is deliberate and is pinned
  by two rows.
- **No reply path.** Unchanged: `NeedsPath` — the state is preserved and
  the candidate is not consumed.
- **Cancellation, timeout.** Unchanged.
- **Restart.** The policy is configuration; the budget is read fresh from
  it. Nothing is persisted.
- **Contention.** The engine is runtime-neutral and single-threaded per
  lookup; the budget introduces no shared state.

### The behaviour change, measured

The change falls on the **failure** path. A lookup that succeeds within the
budget is unaffected. A lookup that fails now stops at the budget instead
of querying as many peers as the candidate list holds — so it does less
work, finishes sooner, and **may fail where it previously succeeded**.

The measured cost is small: the engine's existing rows **peak at two
attempts**, four times below the default budget of 8, so **no existing row
changed outcome** and **no default raise was needed**. The bound was not
weakened to make anything pass.

## Migration review

No migration. No stored format changes. No data-directory changes. No
configuration key is added or changed.

`LookupFinalState` gains a public variant. That is source-breaking for an
external exhaustive matcher; inside the workspace the type is carried as a
*value* by the daemon's routing outcomes and by the client's, and the only
exhaustive match is the `Display` arm in the type's own file, which was
updated. The compiler confirmed there is no other exhaustive match.

## Security review

- **The bound fails closed.** A lookup that cannot spend more than the
  budget's worth of attempts stops, rather than continuing unbounded. This
  is a resource bound on the lookup path, and it now holds.
- **No secret is logged.** The receipt's `terminal_counts` carries counts
  and stable state labels only. No peer hash, no key, no endpoint, no
  payload.
- **No new authority, no new reachability.** The budget changes how many
  peers a lookup asks, not which peers are reachable. The routing table's
  selection semantics are unchanged.
- **The exhaustion is visible, not hidden.** A lookup that gives up because
  of the policy ceiling lands in its own receipt bucket, so an operator can
  see the bound binding rather than inferring it from a generic failure.

## Documentation and operations

- `docs/architecture/i2pr-netdb.md` — a new section, *The per-lookup peer
  budget (Plan 500)*: where the bound is enforced, that it narrows attempts
  and never candidates, the two terminal states side by side, and the
  behaviour change on the failure path. The module table's
  `lookup_action` and `lookup_engine` rows, the test headline (420 tests,
  302 in-crate and 118 integration) and the integration file table with the
  new lane's row were recomputed.
- `docs/architecture/i2pr-daemon.md` — the routing receipt's new
  `terminal_counts` and the fact that the two exhaustions are counted in
  separate buckets. The `netdb_participation.rs` line count, the
  integration test file count (98) and the area inventory (80 of 98) were
  recomputed, and the new lane's row was added.
- `specs/CONFORMANCE.md` — a new Plan 500 section stating the claim and the
  non-claims: the change falls on the failure path,
  `max_candidates_considered` is unchanged, the routing table's selection
  semantics are unchanged, the participation driver is still **not**
  composed into the router's tunnel building (Plan 433's registered blocked
  scope), there is no stock-peer evidence, and **nothing is promoted**.

**Operations.** Nothing to configure: the bound is the policy's existing
default of 8. The count to watch is the routing receipt's
`terminal_counts["peer-budget-exhausted"]` — a non-zero value means the
bound is binding, which is the signal that the default may need raising for
the deployment's topology. The receipt stays counts-only.

## Limitations

1. **The participation driver is still not composed into the router's
   tunnel building.** That is Plan 433's scope and remains registered
   **blocked**. This plan changes the driver's *reason vocabulary*, not its
   wiring.
2. **There is no stock-peer evidence** for the new exhaustion, and none is
   possible on this host. The change is in a runtime-neutral crate with no
   I/O, so it is unit-tested rather than lane-tested.
3. **The behaviour change on the failure path is real.** A failing lookup
   does less work and finishes sooner, and may fail where it previously
   succeeded. The measured cost on the engine's existing rows is zero
   (they peak at two attempts against a budget of eight), but a lookup that
   genuinely needed a ninth candidate now stops at the eighth. Raising
   `DEFAULT_MAX_PEERS_PER_LOOKUP` is the remedy, and it is a one-constant
   change that this plan deliberately did not make, because nothing
   measured a need for it.
4. **The daemon's exploration path has its own budget.** The exploration job
   bounds its own fan-out at 8, which wins over the engine's default, so
   the engine's budget does not fire in that path at the defaults. The
   daemon's own lane raises the exploration budget to 16 so the engine's
   ceiling is reached. This is a pre-existing layering of two bounds, not
   an interaction introduced here.
5. **`LookupFinalState` gained a public variant**, which is source-breaking
   for an external exhaustive matcher. Inside the workspace it is carried as
   a value.
6. **The closure record's own citation was deferred** to this commit (the
   `db0efbca` commit), matching the repo's precedent for Plans 497 and 498,
   so that every commit in the tree resolves its links.

## Findings by severity

**Low**

- **L1 — a stale published figure was corrected.** The netdb deep-dive
  reported **110** integration tests; the actual count before this plan was
  **111**. Corrected to 118 (the true pre-plan figure plus this plan's 7
  new rows) while the row was being touched.
- **L2 — a wrong doc was corrected.** `LookupFinalState::PeerExhausted` was
  documented as *"The lookup exhausted the peer budget without a usable
  `DatabaseStore`"*, the opposite of what the code does. **Fixed** in this
  plan, because the vocabulary was being touched and the two states are now
  siblings.
- **L3 — the netdb deep-dive's per-module unit-test distribution is stale.**
  The in-crate `#[cfg(test)]` breakdown at the end of its Tests section was
  not recomputed for this plan; only the totals were. Left as-is rather
  than half-updated.

**Informational**

- **I1 — the engine's budget and the daemon's exploration budget are two
  separate bounds**, and the exploration's wins at the defaults. See
  *Limitations* 4. Recorded so a future reader does not mistake the
  absence of `peer-budget-exhausted` counts in the daemon's receipts for a
  bug.
- **I2 — pre-existing doc drift** in the daemon deep-dive: four `file:line`
  citations name files by bare filename with no directory prefix. The files
  exist under `crates/i2pr-netdb/src/`; a link checker cannot resolve the
  bare form. Not introduced by this plan.

## Roadmap disposition

`plans/subsystems/network-participation-roadmap.md`: the Plan 500 row moves
from `registered` to `closed` with the token
`passed-lookup-peer-budget-enforcement` and this closure record.

The line's own ordering (ADR 0041: substrate-first, no advertisement) is
unchanged. Plan 500 is a **discharge** of Plan 497's L2, in the same way
Plans 497, 498 and 499 discharged earlier findings — it does not add a
phase to the sequence and it advertises nothing.

## Unblock audit

Audited `plans/registry.md`'s blocked work and the network-participation
roadmap's dependency graph for anything that lists Plan 497's L2 as a
dependency.

| Plan | Was | Now | Why |
| --- | --- | --- | --- |
| 433 — live NetDB/tunnel/client composition | blocked | **blocked** | Blocked on Plans 431/432. Plan 500 does not touch the composition. Unchanged. |
| 493 — two-family public participation qualification | blocked | **blocked** | Blocked on the authorized two-host topology and Plan 435's transport activation. Unchanged. |
| 494 — floodfill role and the `caps=f` advertisement | blocked | **blocked** | Blocked on Plan 493. Unchanged. |
| 435 / 439 / 436 | blocked | **blocked** | External environments this host does not have. Unchanged. |

**Nothing moved to `ready`.** Plan 500 removes a source-side gap that no
registered plan was blocked on.

No corrective plan is required by this closure. L1 and L2 were corrected in
place while the rows they touched were being edited; L3 is a stale
figure in a row this plan did not touch, recorded rather than half-updated.
