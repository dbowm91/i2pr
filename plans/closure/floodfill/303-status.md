# Plan 303 status: passed

- Plan: `plans/implementation/floodfill/303-m12-matrix-execution-with-unseeded-publisher-trigger.md`
- Closure token: `passed-m12-matrix-execution-with-unseeded-publisher-trigger`
- Corrective: none required. No successor needed for the i2pd one-family
  matrix — it passed. Plan 279 moves to ready (it does not auto-pass).
- Stop classification: n/a — the single bounded attempt passed all rows.

## 1. What was executed

The single Plan 303 §3 lane change plus one bounded matrix attempt:

1. **Driver diff (seed change only, commit `c069e6c`)**:
   `crates/i2pr-daemon/tests/floodfill_i2pd_external.rs` seeds exactly the
   two floodfill peers (A, B) and no longer seeds the reference client (C).
   C is still loaded for the `client-peer-identified` hash record; its NetDB
   record exists from matrix A onward via its own publisher store, and the
   matrix C/E waits already run after A — the §3 fixture concern is closed
   by existing test ordering, no reorder needed.
2. **Local proof pre-exists**: `fresh_publisher_store_inserts_and_plans_direct_replication`
   (Plan 302) locks the exact new lane shape — two seeded peers, fresh
   publisher key → Inserted + ack + ≥1 zero-token DirectFlood.
3. **One bounded attempt**: fresh frozen budget 1 (`attempt budget: 1
   (frozen)`), exact-pinned i2pd 2.61.0 at
   `635b013a612ff47278ef02acf8580a28e10e26c5`, loopback-only, reseed
   disabled. Evidence root `target/interop/m12-floodfill-evidence`
   (the runner's canonical root; the override env var is not honored and
   was not needed — no prior evidence existed at that root on this head).
4. **No production code change**, as expected. No new defect surfaced, so
   the §4 no-guarantee branch (register a corrective) did not fire.

## 2. Live results (sanitized counts only)

```
phase 1: controlled activation passes; published 680-byte caps=f RouterInfo
replication-targets-seeded count=2 (was 3)
publisher-store-insert-outcome: outcome=["inserted"] replication_offered=1 replication_absent=0
publisher-store-accepted=1, store_ack_delivered=1
lookup-replies-classified: hit=0 dsrm=7 tunnel=0
replication-direct-store-delivered count=4, zero-token-direct-only, no tunnel fallback
dispatch_errors=[], queue_full=0, owner-cancel-drained graceful
reference c.log: "Publishing confirmed" x1, "Message expired" x0, "Can't find floodfill" x0
evidence.json: m12_floodfill_one_family = "passed-via-i2pd-2.61.0", attempt_budget=1, attempts_used=1
```

All 10 evidence rows `passed`: controlled-activation, reference-verified,
role-active, publisher-store (A), store-ack (A), lookup-answered (C/E),
replication-direct (F), no-tunnel-flood, dispatch-clean, workspace-gates.

What this proves:

- **The Plan 303 trigger works exactly as designed.** The first publisher
  store inserts (fresh key) and offers replication; the planner turns it
  into 4 zero-token direct replica stores. Matrix F passes.
- **The Plan 302 fix holds live.** Publish confirmed, zero expired drops,
  no floodfill-selection loop.
- **Lookup answers stay valid without a seeded C.** All 7 reference
  lookups answered (DSRM class — genuine exploration lookups for keys the
  controlled NetDB does not hold). The hit=0 (vs 2 in the Plan 302 run) is
  expected: the earlier hits answered lookups for C's own key from the
  seeded replica; C's live-published record now covers that key from matrix
  A onward, and no post-A lookup for it arrived. DSRM is a legitimate reply
  class, and the C/E row records exactly what was observed.

## 3. Verification

- Lane `workspace-gates` row passed on the attempt head.
- `cargo fmt --all --check` clean.
- `cargo check --locked --workspace --all-targets` clean.
- `cargo test --locked --workspace --all-targets -- --test-threads=1`:
  **3257 passed, 31 ignored, 0 failed** (109 suites) with `--no-fail-fast`,
  exit 0, on head `c069e6c`.
- One transient failure occurred on an earlier full-workspace pass (a
  9-test loopback suite, 8/1, ~37 s) between two green full runs; the four
  timing-sensitive 9-test candidates (`ssu2_local`,
  `service_tunnels_local_roundtrip`, `destination_tunnel_live`,
  `netdb_tunnel_live`) each pass 9/9 in isolation. Load flake under three
  back-to-back full runs, consistent with the documented loopback-suite
  flakiness — not a product signal, and unrelated to the test-driver-only
  Plan 303 diff. Recorded here per the no-overclaim rule.
- `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` clean.
- `RUSTDOCFLAGS="-D warnings" cargo doc` + workspace doc-tests clean.
- `scripts/check-dependency-direction.sh`, `check-runtime-boundaries.sh`,
  `check-m12-floodfill-boundaries.sh` all pass.

## 4. Security / resource review

- Production delta: none. The only change is 7 lines in the
  `#[ignore]`-gated external driver (seed set 3→2 + comment).
- No guard, policy, budget, declaration, or threshold change. No secret
  material touched; evidence rows are counts/category labels only.
- No `unsafe`, no new dependencies, no runtime-boundary change.
- No capability, version, or RouterInfo advertisement change of any kind.
  Normal-daemon `caps=f` stays unadvertised; this closes only the
  one-family (i2pd) controlled matrix.

## 5. Unblock audit

- Plan 278 stays stopped as a historical record (per Plan 303 §8).
- Plan 279 (second-family qualification) moves **blocked → ready**: its
  blocker was the unexecuted matrix, now executed via Plan 303. It does
  not auto-pass; its own qualification still has to run.
- Plans 284/285 stay retained. No other registered plan lists the matrix
  as a hard dependency.
- Scope honesty: the matrix is one-family (i2pd 2.61.0) only. Java-family
  qualification is a separate, unregistered lane owned by Plan 279's
  execution — nothing here substitutes for it.
