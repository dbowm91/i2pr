# Plan 545 — the SSU2 pool must not be floodfill-only

**Status:** registered. **Subsystem:** network-participation.
**Registered:** 2026-11-12. **Successor to:** Plan 544.

## 1. Objective

The outbound SSU2 session pool must be able to keep a session with a peer the
router has learned, not only with a floodfill.

The pool's only candidate source is the floodfill selection:

```text
// crates/i2pr-daemon/src/peer_pool.rs
let selection =
    select_version_gated_floodfills(store, local_hash, &[*local_hash], &self.policy);
```

`select_version_gated_floodfills` yields only records that advertise `f`, so
the pool can dial floodfills and nothing else.

On a private LAN that population is **empty by construction**: floodfill
eligibility requires a publicly routable address
(`crates/i2pr-daemon/src/floodfill.rs:965`, `is_publicly_routable_host`), and
`controlled_loopback` floodfill requires loopback binds, so no private-LAN
record can advertise `f`. Measured on the two-host lab:
`normal floodfill opted in; evaluating eligibility requested=true
role="Disabled"`, `floodfill_advertisers=0`, and the pool planned nothing.

The gap is not LAN-specific. On the public network the pool dials the
floodfill subset and never the peers the router actually learned, so an
ordinary peer and this router never hold an SSU2 session.

The plan makes the pool **prefer floodfills and fall back** to any
version-eligible peer. That is the operator's chosen posture: floodfills first.

## 2. Why ready

**The selection already has a sibling shape.** `select_version_gated_floodfills`
is a truncate-then-gate over `store.floodfill_advertisers()`. The same ordering,
the same exclusion set, the same policy ceiling and the same gate apply to
`store.iter()`; only the iterator differs.

**The eligibility rule does not change.** A dial target is gated by
`dsm_target_eligibility` exactly as it is today, so the fallback cannot admit a
peer the floodfill path would have refused.

**The evidence exists.** With the fallback in place the instrumented pool
reported `planned=1 examined=1 refused=0 dials_started=1` and then
`outcome=Established` in 49 ms — the first SSU2 session between two `i2pr`
routers on separate machines.

## 3. Current implementation evidence

### 3.1 The pool's only source

`crates/i2pr-daemon/src/peer_pool.rs`, in `PeerPoolOwner::plan`: the sole
selection call is `select_version_gated_floodfills`, and the counters it
publishes (`candidates_examined`, `candidates_refused`) come from that
selection's report.

### 3.2 The floodfill selection is floodfill-only

`crates/i2pr-netdb/src/lookup_policy.rs`, `nearest_floodfill_candidates`
iterates `store.floodfill_advertisers()`.

### 3.3 No private-LAN record can advertise `f`

`crates/i2pr-daemon/src/floodfill.rs:965` — `is_publicly_routable_host` refuses
loopback, private, link-local, multicast, broadcast and unspecified. The
`controlled_loopback` floodfill profile is separately refused for RFC1918 binds
at configuration time.

### 3.4 The pool's silence cost a diagnosis

Before this plan's companion instrumentation the pool logged nothing, so an
empty population and a pool that was not running looked identical. That
misreading is recorded in Plan 544's closure record; the counters are now
logged.

## 4. Invariants

1. **Floodfills are preferred.** A non-empty floodfill selection is used
   unchanged; the fallback never displaces it.
2. **The fallback is version-gated.** The same `dsm_target_eligibility` rule,
   the same refusal counting, and the same `examined == accepted + refused`
   identity.
3. **One order, not two.** The fallback uses the same XOR-distance ordering and
   the same `max_candidates_considered` ceiling.
4. **The pool's ceilings are unchanged.** `MAX_POOLED_PEERS`,
   `MAX_DIALS_PER_PERIOD` and the release-on-every-path behaviour are untouched.
5. **The self-exclusion is unchanged.** The router's own hash is still excluded.
6. **No new surface.** No listener, no socket, no configuration key, no
   dependency.
7. **No advertisement.**
8. **The predecessors' evidence stays byte-identical.**

## 5. Scope

### 5.1 In scope

- `crates/i2pr-netdb/src/lookup_policy.rs` — the sibling selection and the
  shared gate.
- `crates/i2pr-netdb/src/lib.rs` — the re-export.
- `crates/i2pr-daemon/src/peer_pool.rs` — the preference and the fallback.

### 5.2 Out of scope

- The floodfill eligibility rule.
- The pool's ceilings and its release-on-failure behaviour.
- The lookup paths, which must keep asking a floodfill.
- The transport's dial scope.

### 5.3 Out of scope, with the owner named

- **`ssu2.advertise` is inert in every posture.**
  (`crates/i2pr-runtime/src/ssu2_runtime.rs:3671`) **Owner:** a successor that
  gives the runtime a lab-side reachability class.
- **`publish_router_hash` has no production caller.**
  (`crates/i2pr-daemon/src/i2pcontrol_inspection.rs:450`) **Owner:** a
  successor that retires it or wires it.
- **T4/T5 and stock-router admission** — Plan 493's topology.

## 6. Required production changes

1. `fn nearest_peer_candidates(...)` — the store-wide nearest set, same order
   and ceiling.
2. `fn gate_candidates(store, candidates) -> VersionGatedFloodfillSelection` —
   the gate extracted so both selectors share one implementation.
3. `pub fn select_version_gated_peers(...)` — the fallback selector.
4. `PeerPoolOwner::plan` — floodfills first; when `targets().is_empty()`, the
   peers selection.

## 7. Design decisions

### 7.1 Prefer, do not replace

A floodfill is the peer that owns the keyspace near the router's own hash, so
keeping a session with one is worth more than keeping one with an arbitrary
peer. The fallback exists so the pool is never empty, not so it ignores
floodfills.

### 7.2 The gate is shared, not duplicated

Two copies of the eligibility rule would eventually disagree about a refusal
reason or the `examined` identity. `gate_candidates` is the single
implementation and both selectors reach it.

### 7.3 The fallback lives in the netdb crate

The selection is routing policy and `i2pr-netdb` owns routing policy. The
daemon chooses the preference; the crate owns what "eligible" means.

### 7.4 The pool's ceilings are not weakened

The fallback adds candidates, not capacity. `MAX_POOLED_PEERS` and
`MAX_DIALS_PER_PERIOD` still refuse, and a refused candidate is still counted
rather than evicting a peer the pool already holds.

## 8. Work packages

1. **WP1 — the selection.** The sibling selector and the shared gate.
2. **WP2 — the pool.** Preference and fallback.
3. **WP3 — observability.** The tick and outcome counters (companion commit
   `6ba8ba98`).
4. **WP4 — the lane.**
5. **WP5 — the guard.**
6. **WP6 — the negative controls.**
7. **WP7 — the documentation and the inventory.**
8. **WP8 — the floor and the closure record.**

## 9. Failure, cancellation, restart and contention semantics

**Failure.** A refusal is counted under the same reasons as before. The
fallback never turns a refusal into an admission.

**Cancellation.** Unchanged: the pool checks cancellation between dials.

**Restart.** The pool rebuilds its state; the store is the population.

**Contention.** One extra pass over `store.iter()` only when the floodfill
selection is empty, under the bootstrap lock the pool already holds for
`plan`. No new lock, no new await under a lock.

## 10. Compatibility and migration

**No wire change. No configuration key. No persisted state. No manifest
change.** The netdb crate gains one additive public function.

## 11. Required tests

### 11.1 The lane

A store holding **only** non-floodfill peers yields a non-empty selection from
`select_version_gated_peers` and an empty one from
`select_version_gated_floodfills`; adding one floodfill makes the floodfill
selection win; a below-floor peer is refused by both with the same reason.

### 11.2 The guard

The pool calls the floodfill selector **first** and the fallback only when it is
empty; the gate is shared rather than duplicated; the ordering, the ceiling and
the self-exclusion are unchanged; no key, dependency, listener or socket is
added; nothing is advertised; the predecessors' evidence is byte-identical.

## 12. Exact verification commands

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked -p i2pr-netdb --all-targets
cargo test --locked -p i2pr-daemon --test peer_pool_fallback -- --test-threads=1
cargo test --locked -p i2pr-daemon --test peer_pool_fallback_boundary -- --test-threads=1
```

plus the routine floor, with this plan's two steps added.

## 13. Documentation updates

`specs/support.toml` (surface `netdb.peer-pool-fallback`),
`specs/CONFORMANCE.md`, `specs/QUALIFICATION-GATES.md`,
`docs/architecture/i2pr-netdb.md`, `docs/architecture/i2pr-daemon.md`, and this
plan's amendment log.

## 14. Acceptance criteria and stop conditions

Accepted when a non-floodfill peer is selectable, a floodfill still wins, the
gate and its refusal counting are unchanged, the pool's ceilings are unchanged,
no key or dependency is added, nothing is advertised, and the two-host run
establishes an SSU2 session.

**Stops** if the fallback would admit a peer the floodfill path refuses, if it
would change a refusal reason, or if it would require weakening a pool ceiling.

## 15. Closure evidence required

Commits; the floor table; lane and guard rows; negative controls with the
restoration proof; the requirement-to-evidence matrix; the invariant, failure,
migration and security reviews; limitations, findings, roadmap disposition and
the unblock audit.

## 16. Handoff notes

- Experimental, default-off, non-advertised.
- Floodfills stay first. Do not let a later change invert the preference: the
  pool's purpose is a session with the peer that owns the nearby keyspace.
- Do not move the eligibility rule into the daemon.
- The predecessors' closure records are **not** rewritten.

## 17. Amendment log

### 17.1 — 2026-11-12, registration

Registered after the two-host LAN run showed an SSU2 session established with
the fallback in place and an empty pool without it. The scope is the selection
and the pool's preference; the floodfill eligibility rule, the pool's ceilings
and T4/T5 are named with their owners and not attempted here.

### 17.2 — 2026-11-12, the guard's ceiling rule and the inventory id

Two corrections, recorded here rather than rewriting the sections they belong to.

**The inventory id is `netdb.peer-pool-fallback`, as §13 names it.** The row was
first registered as `ssu2.pool-candidate-population`, which no plan of record
names; the id was corrected in `specs/support.toml` and the `protocol` field was
kept `<id prefix>.`-consistent with every other row.

**The ceiling rule counts both builders.** §11.2 asks the guard to own the
shape, and the pool's shape now depends on **two** candidate builders: the
floodfill set and the peer set. The guard's first version checked only that the
ceiling step was *present* in the module, which passes when one of the two
builders loses it. The rule now requires exactly two applications of
`policy.max_candidates_considered()` and exactly two nearest-first orderings,
with an inverted probe that removes one of them.

The strengthening was first applied to the **generated** guard file by hand, and
regenerating the guard silently dropped it. The **negative-control sweep caught
this**: NC-545-5 removed the peer builder's ceiling, fired the lane's
`the_ceiling_is_the_policy_ceiling`, and the guard still passed. The rule and
its probe now live in the generator, so a regeneration cannot lose them, and the
sweep was re-run against the corrected tree.

The lane is 7 rows; the guard is 6 rules and 6 inverted probes (12 tests). The
negative-control sweep ran 6 controls against this plan; all fired, and the tree
was restored byte-identically after each. NC-545-1 is the one that documents the
division of labour: reordering the pool fires the guard while the lane still
passes, because the lane does not exercise the pool's ordering.

### 17.3 — 2026-11-12, the shared gate's consequences for a closed guard, and an
inherited doc defect

Two consequences of extracting `gate_candidates` (§7), both found by the
routine floor's first run on this tree and both discharged before the floor was
re-run.

**Plan 503's guard was retargeted.** Its rule
`the_dsm_enforcement_points_consult_the_gate`
(`crates/i2pr-daemon/tests/version_floor_boundary.rs`) required the **body** of
`select_version_gated_floodfills` to contain `dsm_target_eligibility(`. §7 puts
that call in `gate_candidates` instead, so the rule failed and the floor's
workspace test step reported it. The rule is **retargeted, not weakened**, in
the manner that same file already records for Plan 491's rule: it now pins
**two** links — the selection must reach `gate_candidates`, and `gate_candidates`
must consult the DSM gate — and a second inverted probe proves the second link
fires. Plan 503's closure record is **not** rewritten; this is the successor's
correction. The file becomes the twenty-first digest this plan's guard pins, so
a later plan cannot quietly loosen the retarget.

**A doc link in Plan 542's file did not resolve.**
`crates/i2pr-daemon/src/netdb_participation.rs` documents `PruneOutcome`
through an intra-doc link, but `PruneOutcome` is defined in
`crate::peer_reconcile` and the only import of it is **function-local**, so
`cargo doc --locked --workspace --no-deps` under `RUSTDOCFLAGS="-D warnings"`
fails for `i2pr-daemon`. The line is unchanged since `05bef9ed`, Plan 542's
production commit, so the step was already red before this plan started -- and
Plans 542's and 543's closure records both claim a green floor that includes it.
The link now names its path explicitly. This plan records the evidence gap in
its closure record; it does **not** rewrite either closed record.

The floor's first run also reported one failure this plan does not own and does
not fix: `check-m11-transit-qualification-evidence.sh` failed once with "no
record_cardinality call site for Epoch::Ibgw" and passes on every re-run, at
this commit and at `ba09bd18` before it. The checker runs `grep -A8 ... |
grep -q ...` under `set -euo pipefail`, so the upstream `grep` receives SIGPIPE
when the downstream one exits on its first match; the pipeline then reports
non-zero and the check fails. It is a pre-existing flake with its own owner and
is recorded, not patched, here.
