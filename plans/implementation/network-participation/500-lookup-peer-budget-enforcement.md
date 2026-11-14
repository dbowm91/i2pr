# Plan 500 — The lookup engine's peer budget becomes load-bearing

| Field | Value |
| --- | --- |
| Plan | 500 |
| Subsystem | `network-participation` |
| State | `closed` (`passed-lookup-peer-budget-enforcement`) |
| Depends on | Plan 489 (`passed-floodfill-indexed-routing-table`), Plan 497 (`passed-netdb-participation-consumer-phase`) |
| Discharges | Plan 497 L2 — `LookupPolicy::max_peers_per_lookup` is declared and validated but read by nothing |
| Closure record | `plans/closure/network-participation/500-status.md` |

## 1. Objective

Make the lookup engine's **peer budget** a real bound.

`LookupPolicy::max_peers_per_lookup` is declared, validated at construction
and exposed by an accessor — and **no production code path reads it**. The
only reader in the tree is an assertion inside the policy's own test module.
The engine therefore queries as many peers as the candidate list contains
(up to `max_candidates_considered`, default 32), while the policy declares a
tighter per-lookup peer ceiling (default 8) that it never applies.

This plan makes that ceiling load-bearing: a lookup stops querying peers at
the budget and terminates with a **typed** exhaustion, distinct from the
existing candidate-exhaustion state. The bound narrows **attempts**, never
**candidates** — the routing table's selection and Plan 489's frozen
candidate list are untouched.

The change is deliberately narrow. It is one enum variant, one guard in the
engine's attempt loop, one mapping in the daemon's participation driver, and
the tests that prove the bound holds.

## 2. Why ready

- The engine **already holds the policy**: `RouterInfoLookup { policy:
  LookupPolicy }` (`crates/i2pr-netdb/src/lookup_engine.rs:248`, constructed
  at `:255`). The budget is therefore in scope where it is needed; nothing
  has to be threaded through a new API.
- The policy's validations are already load-bearing at construction
  (`crates/i2pr-netdb/src/lookup_policy.rs:87-91`): a zero peer budget is
  `LookupPolicyError::ZeroPeers`, and a peer budget larger than the
  candidate ceiling is `PeerBudgetExceedsCandidates`.
- The terminal-state vocabulary already exists and is already carried as a
  **value** (not matched exhaustively) by both the daemon's routing outcomes
  (`crates/i2pr-daemon/src/netdb_participation.rs:1238`, `:1271`) and the
  client's (`crates/i2pr-client/src/routing.rs:695`). A new variant is
  additive.
- The `i2pr-netdb` crate is runtime-neutral with no I/O, so the whole change
  is unit-testable on this host with **no external environment**.
- The netdb's boundary section exists in `scripts/check-runtime-boundaries.sh`
  (Plan 497 discharged Plan 488 F1), so the crate is already policed.

## 3. Current implementation evidence

### 3.1 The budget is declared, validated and never read

`crates/i2pr-netdb/src/lookup_policy.rs`:

| Item | Site |
| --- | --- |
| `DEFAULT_MAX_PEERS_PER_LOOKUP = 8` | `:24` |
| `DEFAULT_MAX_CANDIDATES_CONSIDERED = 32` | `:33` |
| `const _: () = { assert!(DEFAULT_MAX_PEERS_PER_LOOKUP <= DEFAULT_MAX_CANDIDATES_CONSIDERED); ... }` | `:37-40` |
| the field `max_peers_per_lookup: usize` | `:47` |
| `ZeroPeers` refusal | `:87-89` |
| `PeerBudgetExceedsCandidates` refusal | `:90-92` |
| the accessor `max_peers_per_lookup()` | `:129-131` |

A repo-wide search for `max_peers_per_lookup` returns the declaration, the
constructor, the accessor, the two validations, the two `Display` arms, and
**one** call — `crates/i2pr-netdb/src/lookup_policy.rs:337`, inside the
`mod tests` block that begins at `:283`. **There is no production reader.**

The policy's own documentation states the intent:

> `crates/i2pr-netdb/src/lookup_policy.rs:29-33` — "Bound on the candidate
> set the selector produces. The value matches the maximum number of
> candidate floodfills the lookup state machine will ever attempt to query."

and

> `crates/i2pr-netdb/src/lookup_policy.rs:76-80` — "The candidate ceiling must
> be at least as large as the per-lookup peer ceiling."

Two distinct ceilings are declared. Only one is enforced.

### 3.2 The engine's attempt loop has no peer bound

`crates/i2pr-netdb/src/lookup_engine.rs:686-721` is the attempt loop. For
each candidate it skips the already `queried` or `excluded` peers, requires
a reply path, pushes the peer onto `state.queried`, increments
`state.attempts` (`:699-700`), and emits the attempt. When no eligible
candidate remains it terminates with `LookupFinalState::PeerExhausted`
(`:728`) or `NoEligibleCandidates` (`:726`).

The only bound on the number of attempts is therefore the **length of the
candidate list**, which `select_floodfill_candidates` truncates to
`max_candidates_considered` (`crates/i2pr-netdb/src/lookup_policy.rs:279`).
The tighter peer ceiling is never consulted.

### 3.3 The terminal-state vocabulary already distinguishes the two cases

`crates/i2pr-netdb/src/lookup_action.rs:124-140`:

```rust
pub enum LookupFinalState {
    Success,
    /// The lookup exhausted the peer budget without a usable
    /// `DatabaseStore`.
    PeerExhausted,
    Timeout,
    Cancelled,
    PathUnavailable,
    /// The lookup found no eligible floodfill candidates.
    NoEligibleCandidates,
}
```

`PeerExhausted` today means *the candidate list ran out*. With a peer
budget enforced, "the budget ran out" is a **different** event with a
different cause and a different remedy (raise the budget, versus find more
candidates). Conflating them would make the bound invisible in the
diagnostics, so this plan adds a distinct variant.

## 4. Invariants that must not regress

1. **The budget narrows attempts, never candidates.** The candidate set the
   selector produces is unchanged, and Plan 497's invariant — the routing
   table may **reorder** the flat store's selection but never narrow or
   widen it — is untouched. A peer that the budget prevented querying is
   simply not queried; it is never *removed* from the candidate set.
2. **Plan 489's frozen candidate list is unchanged.**
   `RouterInfoLookup::start_with_candidates` keeps its exact semantics: the
   caller's list is the candidate list, and the budget applies to how many
   of them are *queried*, not to which ones exist.
3. **Plan 497's preference prefix is unchanged.**
   `start_with_preferred` keeps its exact semantics: the preference is a
   priority prefix, and the store is re-selected on every advance.
4. **The candidate ceiling is unchanged.**
   `select_floodfill_candidates`'s truncation to `max_candidates_considered`
   is not touched.
5. **The policy's existing validations stay load-bearing.** A zero peer
   budget is still `ZeroPeers`; a peer budget above the candidate ceiling is
   still `PeerBudgetExceedsCandidates`.
6. **No lookup can exceed the budget.** At most `max_peers_per_lookup`
   attempts are emitted, across every entry point (`start`,
   `start_with_candidates`, `start_with_preferred` and every subsequent
   advance).
7. **The exhaustion is typed and distinguishable.** A budget exhaustion is
   not reported as a candidate exhaustion, and not as a timeout.
8. **No unbounded growth.** The budget adds no allocation and no new
   structure; it reads a value the policy already holds.
9. **No dependency, no advertisement, no promotion.** `Cargo.toml` and
   `Cargo.lock` stay byte-identical; `specs/support.toml` stays at 94
   surfaces with 0 advertised; `router.version` stays `0.9.62`; `caps` and
   the published RouterInfo are unchanged.

## 5. Scope

### 5.1 In scope

- **WP1 — a distinct typed terminal state.**
  `LookupFinalState::PeerBudgetExhausted`, with a `Display` arm and a doc
  that says what it means and how it differs from `PeerExhausted`.
- **WP2 — the engine's attempt loop honours the budget.**
  Before an attempt is emitted, the engine checks the budget; when the
  budget is spent it terminates with `PeerBudgetExhausted` instead of
  querying another peer. **Every** attempt site is covered — the first
  attempt from each of the three entry points and every subsequent advance.
- **WP3 — the daemon's participation driver maps the new state.**
  The routing driver's terminal mapping gains a distinct, counted reason so
  the exhaustion is visible in the receipt rather than folded into another
  bucket.
- **WP4 — the tests.**
- **WP5 — the documentation.**

### 5.2 Explicitly out of scope

- **The candidate ceiling.** `max_candidates_considered` is enforced and
  correct; it is not touched.
- **The routing table's selection semantics.** Plan 489's and Plan 497's
  invariants are preserved exactly.
- **The participation driver into the product loop.** That composition is
  Plan 433's scope and is registered blocked.
- **Any change to the policy's public constructor or its error vocabulary.**
  The policy's validation surface is already correct.
- **Any behavioural change to a successful lookup.** A lookup that
  succeeds within the budget behaves exactly as it does today.
- **A stock-peer lane, an advertisement, a capability promotion, or a
  version change.**

### 5.3 The one behaviour change, stated plainly

Enforcing the budget changes what a **failed** lookup does. Today a lookup
that fails may have queried up to 32 peers; with the budget enforced it
queries at most `max_peers_per_lookup` (default 8) and then stops with a
typed exhaustion. The lookup therefore **does less work and finishes
sooner**, and it may **fail where it previously succeeded** — a lookup that
needed the ninth candidate now stops at the eighth.

That is the declared policy's intent, and the trade-off is the point of a
peer budget: a bounded number of peers per lookup, bounded again by the
lookup's total deadline. But it is a real change to the live destination
lookup path, so this plan must:

- prove the **default** (8) against the engine's own existing rows, and
- report honestly in the closure record what the default does to a
  failing lookup, including the measured attempt counts.

If the engine's existing rows show that the default of 8 breaks a lookup
that a larger budget would have completed, the honest outcomes are to
**raise the default** or to **record the regression as a finding** — not to
weaken the bound.

## 6. Production changes

| File | Change |
| --- | --- |
| `crates/i2pr-netdb/src/lookup_action.rs` | `LookupFinalState::PeerBudgetExhausted` and its `Display` arm. |
| `crates/i2pr-netdb/src/lookup_engine.rs` | The attempt loop consults `self.policy.max_peers_per_lookup()` before emitting an attempt, at every attempt site, and terminates with the new state. |
| `crates/i2pr-daemon/src/netdb_participation.rs` | The routing driver's terminal mapping gains the new state as a distinct counted reason. |
| `crates/i2pr-netdb/src/lookup_engine.rs` (tests) | The discriminator, the control and the refusal rows. |
| `crates/i2pr-daemon/tests/` | A row proving the driver's receipt distinguishes the exhaustion. |

No new workspace dependency.

## 7. Work packages

### WP1 — a distinct typed terminal state

- `LookupFinalState::PeerBudgetExhausted` with a `Display` arm.
- The doc must distinguish it from `PeerExhausted`: the budget is a
  **policy** ceiling on attempts; `PeerExhausted` is the **candidate list**
  running out.
- Audit every exhaustive `match` on `LookupFinalState` and update the
  arms that need it. The type is carried as a value by the daemon's and the
  client's outcomes, so most matches are already non-exhaustive.

### WP2 — the engine honours the budget

- Before an attempt is emitted, check the number of attempts already made
  against `self.policy.max_peers_per_lookup()`.
- When the budget is spent, terminate with `PeerBudgetExhausted` and do
  **not** emit another attempt, and do **not** mark the peer queried.
- Cover **every** attempt site: the three entry points and every advance.
  A budget that binds only one of them is not a bound.
- The check must not consume a candidate: an unqueried candidate stays
  unqueried.

### WP3 — the daemon's driver maps the exhaustion

- The routing driver's terminal mapping gains the new state as its own
  counted reason.
- The receipt must stay counts-only: no peer hash, no key, no payload.

### WP4 — the tests

- **The discriminator.** A lookup over a candidate set larger than
  `max_peers_per_lookup` stops at the budget, reports
  `PeerBudgetExhausted`, and emits **at most** that many attempts.
- **The control.** A lookup whose candidate set is at or below the budget
  behaves exactly as today — the budget never fires spuriously, and a
  successful lookup is unaffected.
- **The refusal row.** The policy's existing validations still refuse a
  zero budget and a budget above the candidate ceiling.
- **The invariant rows.** Plan 489's frozen candidate list and Plan 497's
  preference prefix are unchanged under the budget: the candidate set is
  the same, only the number of queried peers differs.
- **The daemon row.** The driver's receipt distinguishes the budget
  exhaustion from the candidate exhaustion.

### WP5 — the documentation

## 8. Failure, cancellation, restart, and contention semantics

- **The budget is spent.** The lookup terminates with
  `PeerBudgetExhausted`. It is not an error: it is a typed, counted final
  state, exactly like `PeerExhausted` and `NoEligibleCandidates`.
- **The candidate list runs out first.** Unchanged: `PeerExhausted`.
- **No reply path.** Unchanged: `NeedsPath` — the state is preserved and
  the candidate is not consumed.
- **Cancellation, timeout.** Unchanged.
- **Restart.** The policy is configuration; the budget is read fresh from
  it. Nothing is persisted.
- **Contention.** The engine is runtime-neutral and single-threaded per
  lookup; the budget introduces no shared state.

## 9. Compatibility and migration

- No migration. No stored format changes. No configuration key is added or
  changed.
- `LookupFinalState` gains a variant. It is a public enum, so this is a
  source-breaking change for an external exhaustive matcher — but the type
  is workspace-internal in practice, and the daemon and the client both
  carry it as a value.
- The lookup's **success** path is unchanged. The lookup's **failure** path
  changes as described in §5.3.

## 10. Required tests

- The discriminator, the control and the refusal rows of WP4.
- The engine's existing rows must still pass — the budget must not break
  them, and the plan must report the measured attempt counts.
- The daemon's routing-driver row.
- The routine floor in `AGENTS.md` passes in full.

## 11. Exact verification commands

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-netdb --all-targets
cargo test --locked -p i2pr-daemon --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
```

and the full routine floor from `AGENTS.md`.

## 12. Documentation updates

- `docs/architecture/i2pr-netdb.md` — the peer budget as a load-bearing
  bound, the new terminal state, and the attempt-count semantics.
- `docs/architecture/i2pr-daemon.md` — the driver's reason mapping.
- `specs/CONFORMANCE.md` — the exact claim and the exact non-claims. This
  is a lookup-engine bound, not a network capability; **nothing is
  promoted**.

## 13. Acceptance criteria and stop conditions

### 13.1 Acceptance

1. A lookup over a candidate set larger than the budget stops at the budget
   with `PeerBudgetExhausted`, and emits at most that many attempts.
2. Every attempt site honours the budget, not just one.
3. A lookup within the budget is unaffected.
4. The policy's existing validations still refuse.
5. Plan 489's and Plan 497's invariants are unchanged — the candidate set
   is identical under the budget; only the queried count differs.
6. The daemon's receipt distinguishes the exhaustion.
7. The engine's existing rows still pass.
8. The full routine floor passes.
9. `specs/support.toml`, `Cargo.toml` and `Cargo.lock` are byte-identical.

### 13.2 Stop conditions

Stop and record a finding, rather than working around it, if:

- enforcing the budget would require changing the **candidate set** — the
  budget must narrow attempts only;
- enforcing the budget would break Plan 489's frozen-candidate-list or
  Plan 497's preference-prefix semantics;
- the default of 8 can only be made to work by weakening the bound;
- the daemon's driver could only reflect the new state by folding it into
  another reason;
- closing the finding would require a capability promotion, an
  advertisement, or a version change.

## 14. Closure evidence required

The closure record must carry: the commits; a requirement-to-evidence
matrix naming the discriminator, the control and the refusal rows; the
exact commands and their outcomes; an invariant review against §4, row by
row; a failure, migration and security review; the **measured attempt
counts** for the default budget; the limitations, stated plainly —
including that the participation driver's live composition stays Plan 433's
blocked scope and that no stock-peer evidence exists; findings by severity;
the roadmap disposition; and the unblock audit.

## 15. Handoff notes

- Plan 497's closure record is **not** rewritten. The disposition of its L2
  finding is recorded in this plan's closure record and in the registry
  rows, matching the repo's practice for Plans 490, 496 and 497.
- The plan-of-record is load-bearing on one point: the budget narrows
  attempts, never candidates. A future change that makes it narrow the
  candidate set would break Plan 497's invariant and needs its own
  plan-of-record.
