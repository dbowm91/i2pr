# Plan 545 — the SSU2 pool must not be floodfill-only

**Closure record.** The plan of record is
`plans/implementation/network-participation/545-the-ssu2-pool-must-not-be-floodfill-only.md`.
This record is the authority on what the plan closed, what it did not, and on
what the evidence is.

## Status

Plan 545 is **closed**. The outbound SSU2 session pool now **prefers
floodfills and falls back** to the version-eligible peers the router has
learned, so a router whose NetDB holds no floodfill advertiser — every
private-LAN and controlled-loopback posture — can still open a session.

The pool's only candidate source was `select_version_gated_floodfills`
(`crates/i2pr-daemon/src/peer_pool.rs`), so it could dial floodfills and
nothing else. Floodfill eligibility requires a **publicly routable** address
(`crates/i2pr-daemon/src/floodfill.rs`, `is_publicly_routable_host`) and the
`controlled_loopback` floodfill profile requires loopback binds, so on a private
LAN that population is **empty by construction** and the pool planned no dial
at all. Measured on the two-host lab before the change:
`normal floodfill opted in; evaluating eligibility requested=true
role="Disabled"`, `floodfill_advertisers=0`, and a silent pool.

Classification: **corrective** (the pool's candidate population was
empty-by-construction in the postures the tree supports, and its silence made
the absence indistinguishable from a failure). **Subsystem:**
network-participation. **Closure date:** 2026-11-12.

Nothing is promoted: `router.version` stays `0.9.62`, the inventory's
advertised count stays **zero**, and `Cargo.toml` and `Cargo.lock` stay
byte-identical.

## What the plan closed

**The pool has a fallback population.** `nearest_peer_candidates` is the whole
store's nearest set for a routing key — the same XOR-distance ordering, the same
exclusion set and the same `policy.max_candidates_considered()` ceiling as
`nearest_floodfill_candidates` — and `select_version_gated_peers` applies the
version gate to it. Both live in `crates/i2pr-netdb/src/lookup_policy.rs` and
both are exported from the crate root.

**The gate is shared, not duplicated.** `gate_candidates` is extracted and
reached by **both** selectors, so the refusal reasons and the
`examined == accepted + refused` identity cannot diverge between the preferred
path and the fallback. The version floor of Plan 503 and the tier gate are
unchanged: the fallback admits a peer the floodfill selector would have
admitted, never one it would have refused.

**The pool prefers floodfills, then falls back.** The floodfill selection is the
first source; the fallback is reached only when its target set is **empty**. A
floodfill owns the nearby keyspace, so a session with it is worth more; the
fallback exists so the pool is never empty. The pool's ceilings
(`MAX_POOLED_PEERS`, `MAX_DIALS_PER_PERIOD`) are unchanged — the fallback adds
candidates, not capacity.

**The absence is now named.** The tick reports the planned, examined, refused,
population-read and started counts; every dial reports its outcome; and the three address-parsing
steps in `ensure_peer_session_for_record` that ended in `.ok()?` each name their
failure. This is not decoration: the pool's silence had already cost a
diagnosis, because the pool **was** planning a dial and the dial **did**
succeed, and both facts were invisible in the log.

**The two-host result.** `ssu2 peer pool tick planned=1 examined=1 refused=0
dials_started=1` followed by `ssu2 peer pool dial outcome outcome=Established`
49 ms later on the real LAN — the first SSU2 session between two `i2pr` routers
on separate machines.

## What the plan did not close

- **Stock-router admission.** The session is i2pr-to-i2pr. i2pd and Java I2P
  admission remains Plan 493's question.
- **A public deployment.** The pool is not claimed to reach a public NetDB, and
  the dial scope is still loopback and RFC1918.
- **The pool's ceilings.** They are unchanged by design; whether they are the
  right values is not this plan's question.
- **T4/T5.** The correlated two-way reply and the peer-initiated session are
  Plan 544's named next tiers, not this plan's.

## Implementation commits

| Commit | What it carries |
| --- | --- |
| `6e807e88` | the production change: `nearest_peer_candidates`, the extracted `gate_candidates`, `select_version_gated_peers`, and the pool's preference with its fallback |
| `6ba8ba98` | the observability: the pool's tick counters, the per-dial outcome, and the three named address-parse failures |
| `421f6aba` | the plan of record, the registry's row and the roadmap's row |
| `e1ed8520` | the lane, the guard and the inventory row (one commit shared with Plan 544's lane, guard and row) |
| `26c53a51` | the guard's ceiling rule, which now counts **both** candidate builders |
| `ea7e221b` | the inventory id, corrected to the name §13 gives it |
| `b626866f` | the documentation, the qualification gates, the conformance account, the netdb deep-dive and the plan's amendment log |
| `6af68a8b` | the two fixes the floor's first run demanded -- Plan 503's retargeted rule and the inherited `PruneOutcome` doc link -- with the amendment log's §17.3 |
| `e92019b1` | the §17.3 correction: the m11 floor failure reproduced, so the claim that it "passes on every re-run" was retracted along with the SIGPIPE explanation it rested on |

The floor ran on the committed tree `6af68a8b`. The closure commit adds **only**
this record and the two closed rows.

The plan of record's **invariants** are at `plans/implementation/network-participation/545-the-ssu2-pool-must-not-be-floodfill-only.md:81`,
its **required production changes** at `plans/implementation/network-participation/545-the-ssu2-pool-must-not-be-floodfill-only.md:124`,
its **design decisions** at `plans/implementation/network-participation/545-the-ssu2-pool-must-not-be-floodfill-only.md:134`, its
**lane** at `plans/implementation/network-participation/545-the-ssu2-pool-must-not-be-floodfill-only.md:192`, its **guard** at
`plans/implementation/network-participation/545-the-ssu2-pool-must-not-be-floodfill-only.md:199`, its **closure evidence** at
`plans/implementation/network-participation/545-the-ssu2-pool-must-not-be-floodfill-only.md:236`, its **amendment log** at
`plans/implementation/network-participation/545-the-ssu2-pool-must-not-be-floodfill-only.md:251`, the qualification gates at
`specs/QUALIFICATION-GATES.md:2365`, the conformance account
at `specs/CONFORMANCE.md:4889` and the netdb deep-dive at
`docs/architecture/i2pr-netdb.md:1571`.

## Requirement-to-evidence matrix

| # | Requirement | Evidence |
| --- | --- | --- |
| 1 | a non-floodfill peer is selectable | the lane's row `a_store_of_plain_peers_yields_no_floodfill_and_a_peer_selection` |
| 2 | a floodfill still wins | the lane's row `a_floodfill_still_wins`; NC-545-1 shows the lane cannot see the ordering |
| 3 | the gate and its refusal counting are unchanged | the lane's row `both_selectors_refuse_a_below_floor_peer_with_the_same_reason`; the guard's rule `the_gate_is_shared`; NC-545-3 restores the defect |
| 4 | the pool prefers floodfills and falls back only when empty | the guard's rule `the_pool_prefers_floodfills`; NC-545-1 and NC-545-2 restore the defect |
| 5 | no pool ceiling is weakened | the guard's rule `the_policy_is_unchanged`; NC-545-5 and NC-545-6 restore the defect |
| 6 | the ordering and the ceiling hold on the peer selection | the lane's rows `the_ordering_is_the_same_as_the_floodfill_selection` and `the_ceiling_is_the_policy_ceiling` |
| 7 | `examined == accepted + refused` holds on the fallback | the lane's row `the_identity_holds_on_the_peer_selection` |
| 8 | the router never names itself | the lane's row `the_router_excludes_itself_from_both_selections`; the guard's rule `the_policy_is_unchanged`; NC-545-4 and NC-545-6 restore the defect |
| 9 | no key or dependency is added | the guard's rule `nothing_new_and_nothing_advertised`; NC-545-2's path; the byte-identical `Cargo.toml`/`Cargo.lock` |
| 10 | nothing is advertised | the guard's rule `nothing_new_and_nothing_advertised` |
| 11 | the peer selection is reachable from the crate root | the guard's rule `the_selection_is_exported` |
| 12 | the predecessors stay byte-identical | the guard's rule `the_predecessors_evidence_is_untouched` — twenty-one pinned digests |
| 13 | the two-host run establishes an SSU2 session | `6ba8ba98`'s observable tick and dial outcome, cited in the plan's §2 |

## Focused verification

| Command | Outcome |
| --- | --- |
| `cargo fmt --all --check` | PASS |
| `cargo check --locked --workspace --all-targets` | PASS |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --locked -p i2pr-netdb --all-targets` | PASS |
| `cargo test --locked -p i2pr-daemon --test peer_pool_fallback -- --test-threads=1` | PASS — 7 rows |
| `cargo test --locked -p i2pr-daemon --test peer_pool_fallback_boundary -- --test-threads=1` | PASS — 12 rows (6 rules, 6 inverted probes) |
| `python3 scripts/check-tooling-inventory.py` | PASS |
| `python3 scripts/check-global-plan-number-uniqueness.py` | PASS |
| `python3 -m unittest discover -s tests/planning -p 'test_*.py'` | PASS |
| `python3 /tmp/545-controls.py` | PASS — 14 controls for both plans, each firing the surface that owns the defect, each restored byte-identically |

The full routine floor, with this plan's and Plan 544's steps appended:

| # | Command | Outcome |
| --- | --- | --- |
| 1 | `cargo fmt --all --check` | PASS (4.1 s) |
| 2 | `cargo check --locked --workspace --all-targets` | PASS (18.7 s) |
| 3 | `cargo build --locked -p i2pr-app-fixture -p i2pr-apphost -p i2pr-appd -p i2pr-appctl` | PASS (0.1 s) |
| 4 | `cargo test --locked --workspace --all-targets -- --test-threads=1` | PASS (2234.9 s) |
| 5 | `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | PASS (5.1 s) |
| 6 | `RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps` | PASS (7.3 s) |
| 7 | `cargo test --locked --workspace --doc` | PASS (3.9 s) |
| 8 | `bash scripts/check-dependency-direction.sh` | PASS (0.1 s) |
| 9 | `python3 scripts/check-global-plan-number-uniqueness.py` | PASS (0.1 s) |
| 10 | `python3 scripts/check-adr-number-uniqueness.py` | PASS (0.0 s) |
| 11 | `python3 scripts/check-portable-service-tunnel-api.py` | PASS (0.0 s) |
| 12 | `bash scripts/check-portable-service-tunnel-consumer.sh` | PASS (6.5 s) |
| 13 | `python3 -m unittest discover -s tests/planning -p 'test_*.py'` | PASS (7.4 s) |
| 14 | `bash scripts/check-runtime-boundaries.sh` | PASS (1.1 s) |
| 15 | `bash scripts/check-console-boundaries.sh` | PASS (0.3 s) |
| 16 | `bash scripts/check-console-browser-security.sh` | PASS (0.1 s) |
| 17 | `python3 scripts/check-tooling-inventory.py` | PASS (0.1 s) |
| 18 | `python3 scripts/check-license-metadata.py` | PASS (0.1 s) |
| 19 | `python3 scripts/check-managed-app-private-client-seams.py` | PASS (0.0 s) |
| 20 | `python3 scripts/check-router-readiness-contract.py` | PASS (0.0 s) |
| 21 | `python3 scripts/check-router-readiness-contract.py --self-test` | PASS (0.2 s) |
| 22 | `python3 scripts/check-core-router-recovery-contract.py` | PASS (0.0 s) |
| 23 | `python3 scripts/check-core-router-recovery-contract.py --self-test` | PASS (0.0 s) |
| 24 | `python3 scripts/plan449-ssu2-preflight.py --self-test` | PASS (0.0 s) |
| 25 | `python3 scripts/plan495-peer-eligibility.py --self-test` | PASS (0.1 s) |
| 26 | `python3 scripts/check-plan476-gate-binding.py` | PASS (0.3 s) |
| 27 | `python3 scripts/check-plan476-gate-binding.py --self-test` | PASS (0.6 s) |
| 28 | `python3 scripts/check-managed-app-package-boundary.py` | PASS (0.1 s) |
| 29 | `python3 scripts/check-managed-app-package-boundary.py --self-test` | PASS (0.1 s) |
| 30 | `python3 scripts/check-managed-app-policy-boundary.py` | PASS (0.1 s) |
| 31 | `python3 scripts/check-managed-app-policy-boundary.py --self-test` | PASS (0.1 s) |
| 32 | `python3 scripts/check-managed-app-secured-sandbox.py` | PASS (0.0 s) |
| 33 | `python3 scripts/check-managed-app-secured-sandbox.py --self-test` | PASS (0.0 s) |
| 34 | `bash scripts/check-service-tunnel-boundaries.sh` | PASS (0.1 s) |
| 35 | `python3 scripts/check-managed-app-gateway-boundary.py` | PASS (0.0 s) |
| 36 | `python3 scripts/check-managed-app-manager-boundary.py` | PASS (0.1 s) |
| 37 | `bash scripts/check-m11-per-epoch-composition.sh` | PASS (0.1 s) |
| 38 | `bash scripts/check-service-anonymity-boundaries.sh` | PASS (0.1 s) |
| 39 | `bash scripts/check-fixture-manifest.sh` | PASS (0.1 s) |
| 40 | `bash scripts/check-ntcp2-vectors.sh` | PASS (0.0 s) |
| 41 | `bash scripts/check-ssu2-vectors.sh` | PASS (0.0 s) |
| 42 | `bash scripts/check-i2cp-vectors.sh` | PASS (6.5 s) |
| 43 | `bash scripts/check-ntcp2-interoperability.sh` | PASS (0.0 s) |
| 44 | `bash scripts/check-constrained-host-lane-boundary.sh` | PASS (0.0 s) |
| 45 | `bash scripts/check-m11-transit-boundaries.sh` | PASS (2.2 s) |
| 46 | `bash scripts/check-m11-transit-qualification-evidence.sh` | FAIL (1.0 s) |
| 47 | `bash scripts/check-sam-acceptance-evidence.sh` | PASS (0.1 s) |
| 48 | `bash scripts/check-ssu2-acceptance-evidence.sh` | PASS (0.0 s) |
| 49 | `bash scripts/check-i2cp-acceptance-evidence.sh` | PASS (0.1 s) |
| 50 | `bash scripts/check-i2pcontrol-acceptance-evidence.sh` | PASS (0.0 s) |
| 51 | `bash scripts/check-els2-type11-transcript-boundary.sh` | PASS (0.1 s) |
| 52 | `bash scripts/check-els2-live-lane-evidence.sh` | PASS (2.0 s) |
| 53 | `bash scripts/check-encrypted-service-consumer-caller.sh` | PASS (0.1 s) |
| 54 | `bash scripts/check-outproxy-request-path.sh` | PASS (0.1 s) |
| 55 | `bash scripts/check-outproxy-wire-lane-evidence.sh` | PASS (0.2 s) |
| 56 | `bash scripts/check-config-secret-hygiene.sh` | PASS (0.1 s) |
| 57 | `python3 scripts/check-workflow-validity.py` | PASS (0.1 s) |
| 58 | `bash scripts/check-floodfill-type5-serve.sh` | PASS (0.0 s) |
| 59 | `bash scripts/check-service-tunnel-acceptance-evidence.sh` | PASS (0.6 s) |
| 60 | `bash scripts/check-exploratory-tunnel-evidence.sh` | PASS (0.1 s) |
| 61 | `bash scripts/check-netdb-tunnel-evidence.sh` | PASS (0.1 s) |
| 62 | `bash scripts/check-destination-tunnel-evidence.sh` | PASS (0.2 s) |
| 63 | `bash scripts/check-streaming-tunnel-evidence.sh` | PASS (5.1 s) |
| 64 | `bash scripts/check-m6-mixed-router-acceptance-evidence.sh` | PASS (3.1 s) |
| 65 | `bash scripts/check-m12-floodfill-qualification-evidence.sh --self-test` | PASS (0.1 s) |
| 66 | `bash scripts/check-m12-floodfill-boundaries.sh --self-test` | PASS (0.6 s) |
| 67 | `python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py'` | PASS (0.1 s) |
| 68 | `cargo deny check advisories bans sources` | PASS (0.9 s) |
| 69 | `cargo test --locked -p i2pr-daemon --test router_info_export -- --test-threads=1` | PASS (0.4 s) |
| 70 | `cargo test --locked -p i2pr-daemon --test router_info_export_boundary -- --test-threads=1` | PASS (0.6 s) |
| 71 | `cargo test --locked -p i2pr-daemon --test peer_pool_fallback -- --test-threads=1` | PASS (3.2 s) |
| 72 | `cargo test --locked -p i2pr-daemon --test peer_pool_fallback_boundary -- --test-threads=1` | PASS (0.6 s) |

The floor's own summary is **71 passed, 1 failed, 72 steps**.

## The negative controls

Fourteen controls mutated the **real tree**, ran the lane or the guard, and
observed the expected row fail. Every control was then restored from the bytes
saved before the mutation, and the SHA-256 of every touched file was compared
with the value it had before, so the tree is provably byte-identical afterwards.
Every control **compiles** and names exactly **one** row, which it runs **by
name**. Six of the fourteen belong to this plan.

| Control | What it broke | What restored the defect |
| --- | --- | --- |
| NC-545-1 | the pool selected peers **first** and floodfills second | the guard's rule `the_pool_prefers_floodfills`. The lane **still passes** — it does not exercise the pool's ordering — which is the division of labour the two files are built around |
| NC-545-2 | the fallback became unconditional | the guard's rule `the_pool_prefers_floodfills` |
| NC-545-3 | the gate admitted every candidate | the lane's row `both_selectors_refuse_a_below_floor_peer_with_the_same_reason` |
| NC-545-4 | the peer selection stopped excluding the router | the lane's row `the_router_excludes_itself_from_both_selections` |
| NC-545-5 | the peer cohort lost its ceiling | the lane's row `the_ceiling_is_the_policy_ceiling` **and** the guard's rule `the_policy_is_unchanged` |
| NC-545-6 | the pool stopped excluding its own hash | the guard's rule `the_policy_is_unchanged` |

**The sweep found a defect in this plan's own guard, and the fix went into the
generator.** The guard is produced by a generator, and the rule that counts both
ceiling applications was first applied to the generated file **by hand**.
Regenerating the guard silently dropped it. NC-545-5's first run caught exactly
that: the lane fired and the guard passed. The rule and its inverted probe now
live in the generator, the guard was regenerated, and the third sweep is the one
this record cites: fourteen fired, both companion claims valid, tree
byte-identical.

**The sweep also corrected its own harness.** In the first run the "companion"
claim was evaluated **after** the tree was restored, so it measured the healthy
tree. The harness now runs the companion against the mutated tree.

## Acceptance criteria

| # | The plan's criterion | Outcome |
| --- | --- | --- |
| 1 | a non-floodfill peer is selectable | MET — the lane's row `a_store_of_plain_peers_yields_no_floodfill_and_a_peer_selection` |
| 2 | a floodfill still wins | MET — the lane's row `a_floodfill_still_wins` and the guard's rule `the_pool_prefers_floodfills`; NC-545-1 restores the defect |
| 3 | the gate and its refusal counting are unchanged | MET — the lane's row `both_selectors_refuse_a_below_floor_peer_with_the_same_reason` and the guard's rule `the_gate_is_shared`; NC-545-3 restores the defect |
| 4 | the pool's ceilings are unchanged | MET — the guard's rule `the_policy_is_unchanged`; NC-545-5 and NC-545-6 restore the defect |
| 5 | no key or dependency is added | MET — the guard's rule `nothing_new_and_nothing_advertised` and the byte-identical manifests |
| 6 | nothing is advertised | MET — the same rule |
| 7 | the two-host run establishes an SSU2 session | MET — the LAN run recorded in §2 of the plan of record |
| 8 | the full floor is green, with the closure record's table complete | PARTIAL — 71 passed, 1 failed, 72 steps. The one failure is `check-m11-transit-qualification-evidence.sh`, a pre-existing floor-only flake named under Findings. No other step failed. |
| 9 | the guard's inverted probes each fail as intended and the tree is restored byte-identically after each | MET — the guard's six inverted probes and the six controls above |

## Invariant review

| # | The plan's invariant | Review |
| --- | --- | --- |
| 1 | the fallback admits only peers the floodfill path would admit | HELD — both selectors reach `gate_candidates`, and the lane asserts refusal parity |
| 2 | the refusal reasons do not change | HELD — the gate is extracted, not rewritten, and `VersionRefusalCounts` is untouched |
| 3 | the ceiling does not move | HELD — both builders apply `policy.max_candidates_considered()`; the guard counts **two** applications |
| 4 | the pool's own ceilings do not move | HELD — `MAX_POOLED_PEERS` and `MAX_DIALS_PER_PERIOD` are unchanged and pinned |
| 5 | the ordering is unchanged | HELD — `candidates.sort()` in both builders, `NearestSelection`'s ordering untouched; the guard counts two orderings |
| 6 | no configuration key is added | HELD — the guard names three candidate tokens and finds none |
| 7 | nothing is advertised | HELD — `router.version` stays `0.9.62`, the advertised count stays zero |
| 8 | the predecessors' evidence is byte-identical | HELD — twenty-one pinned digests |

## Failure, cancellation, restart, and contention review

**Failure.** A selection that yields nothing is still a legitimate outcome: the
pool plans no dial and counts no refusal. The fallback changes the population,
not the failure semantics, and a candidate the gate refuses is still counted
rather than dropped.

**Cancellation.** The pool's `plan` is a pure function over a borrowed store; it
awaits nothing and holds no lock across an await. The plan adds no cancellation
point.

**Restart.** No state is persisted. The pool rebuilds its population from the
store on the next tick.

**Contention.** `plan` borrows the store immutably and takes `&mut self` for the
pool; there is no new lock, no new channel and no new task.

## Migration and compatibility review

**No wire change.** Nothing about SSU2, NTCP2, the RouterInfo or any published
record changes.

**No configuration key.** `ssu2.peer_pool` remains the only switch, and it is
still off by default.

**No persisted state.** No new file, no new store, no new manifest.

**One API surface widens.** `nearest_peer_candidates`, `gate_candidates` and
`select_version_gated_peers` are exported from `i2pr-netdb`; `gate_candidates`
became crate-visible so both selectors could reach it. The guard's rule
`the_selection_is_exported` pins the export that matters.

## Security review

**No secret is touched.** The selection reads router hashes and published
version strings.

**No new disclosure.** The pool's new counters name counts, not peers; the dial
outcome names an outcome, not a payload; the three address-parse failures name
the step that failed, not the address.

**The log does not become a map.** The instrumentation added by `6ba8ba98`
records the tick's counts and one outcome per dial. It does not log the target's
address, its hash or its payload, so the lab log is not a peer list.

**No new surface is reachable.** No listener, no port, no socket, no
configuration key, no dependency, no task, no timer.

**Fail closed.** The guard fails closed on every item it names: a peer-first
ordering, an unconditional fallback, a duplicated or renamed gate, a restated
version rule, a lost ceiling, a lost ordering, a missing export, a configuration
key, a runtime construct, a moved version, an advertised surface, a changed
manifest and a changed predecessor's evidence each make the rule that names them
fail.

## Limitations

- **The lane cannot see the pool.** It proves what the two selections yield; it
  does not construct a pool or observe a dial. The ordering claim is the guard's,
  and NC-545-1 is the demonstration that the two files do not overlap.
- **The gate's parity is asserted on a below-floor peer only.** The lane uses one
  refusal class; the other classes are covered by `i2pr-netdb`'s own tests, not
  by this lane.
- **The two-host result is observed, not asserted.** No test starts two daemons;
  the LAN logs are the evidence, and the plan's §2 records them.
- The plan does not claim a stock-router session, a public deployment, or that
  the pool's ceilings are the right values.
- **`plan`'s fallback triggers on an empty floodfill *target* set**, not on an
  empty candidate set: a floodfill population that is present but wholly
  unacceptable also yields the fallback. That is the intended reading (the
  preferred population produced nothing usable), and it is stated here so a
  successor does not mistake it for a bug.

## Findings by severity

### None — nothing was found that blocks the plan

### Medium — discharged

1. **The guard's ceiling rule passed when one of two builders lost the step.**
   Found by NC-545-5 on the first sweep, after a regeneration silently dropped a
   hand-applied strengthening. Discharged: the rule counts both applications and
   both orderings, the probe removes one, and the change lives in the generator
   so a regeneration cannot lose it. Recorded in the plan's amendment log §17.2.

2. **Plan 503's guard was retargeted, not weakened, and that is a change to a
   closed plan's file.** Its rule required the DSM gate call to sit in the
   **body** of `select_version_gated_floodfills`; extracting `gate_candidates`
   moved it one function down, and the floor's workspace test step failed. The
   rule now pins **two** links and carries a second inverted probe, so it is
   strictly stronger -- but a reader looking at Plan 503's closure record will
   not find this, and it is recorded here because a closed record is never
   rewritten. **Owner:** this plan, discharged; the retargeted file is now the
   twenty-first digest this plan's guard pins.
3. **A `PruneOutcome` intra-doc link never resolved, and two closed records say
   otherwise.** `crates/i2pr-daemon/src/netdb_participation.rs` links
   `PruneOutcome`, which lives in `crate::peer_reconcile` with only a
   function-local import, so `cargo doc` under `RUSTDOCFLAGS="-D warnings"` fails
   for `i2pr-daemon`. The line is unchanged since `05bef9ed`, Plan 542's
   production commit, so the step was already red when Plans 542 and 543 each
   recorded a green floor that includes it. This plan fixes the link; the
   discrepancy is **recorded, not repaired** -- closed records are not rewritten,
   and an independent reader should treat those two floor claims as unverified
   for that step.

### Low — recorded by this plan

1. **The negative-control harness measured its companion on the healthy tree.**
   Found by the first sweep. Discharged in the harness (the companion now runs
   inside the mutation's scope), and recorded here because the sweep is evidence
   and a harness defect is a defect in the evidence.
2. **The pool's tick period is unchanged at 30 s.** A lab operator who starts two
   routers and looks at the log within 30 s sees nothing. Not a defect, but the
   reason the instrumented commit exists.
4. **The inventory id was registered under a name no plan uses.** Corrected in
   `ea7e221b`; recorded so the registry's history is not silently rewritten.
5. **`check-m11-transit-qualification-evidence.sh` fails at step 46 of a full
   floor and nowhere else.** It failed in **both** full-floor runs with "no
   record_cardinality call site for Epoch::Ibgw". It passes at `ba09bd18`
   before this branch's plans, ten times in a row at this commit with the
   floor's own shell and environment, and after all thirty-eight script steps
   that precede it. Its three inputs are untouched on this branch. The first
   hypothesis -- SIGPIPE under `set -o pipefail` -- is **falsified**: the
   upstream `grep -A8` emits 2 258 bytes and the exact pipeline ran 500 times
   with zero failures. The failure is order- or load-sensitive inside the full
   floor and **unexplained**. **Not patched**: not this plan's surface, and a
   checker change needs its own plan-of-record. **Owner:** the
   transit-qualification lane. The floor is therefore **71 of 72**, not 72 of
   72, and this record says so rather than rounding it up.

### Informational

- The pool's silence had already cost a diagnosis before this plan: the pool
  **was** planning a dial and the dial **did** succeed on the LAN, and both facts
  were invisible. `6ba8ba98` is the correction.

## Roadmap disposition

The **network-participation** roadmap's row for Plan 545 moves from
`registered` to `closed`, with the verdict
`passed-the-ssu2-pool-must-not-be-floodfill-only` and this record's path. The
row's prose is **not** rewritten: it records what the plan set out to do at
registration.

The **ssu2** roadmap keeps its own state: this plan changes no transport file
and no runtime file, so the transport's own rows are untouched.

## Unblock audit

| Plan | State after 545 | Why |
| --- | --- | --- |
| 431 | unchanged (`stopped`) | keeps its own plan-of-record |
| 449 | unchanged (blocked) | needs the SSU2 external lane's independent peer |
| 472 | unchanged (blocked) | the same |
| 476 | unchanged (blocked) | the gate binding keeps its requirement |
| 493 | **advanced further** | with 544 this makes the two-host i2pr topology **bidirectional at the session layer**; stock-router admission still needs the reference pins and the ban-surface care in `specs/QUALIFICATION-GATES.md` §3 |
| 433, 435–439, 441–448, 451, 452, 455, 457–462, 464–466, 471–474, 494 | unchanged (externally blocked) | they need a public network or an authorized external peer |

## Handoff notes

- Experimental, default-off, non-advertised.
- **Prefer floodfills, then fall back** is the operator's chosen posture, not a
  temporary hack: a floodfill owns the nearby keyspace. Do not invert it without
  a new plan.
- **Do not duplicate `gate_candidates`.** Both selectors reach it, and that is
  what keeps the refusal reasons and the `examined == accepted + refused`
  identity from diverging.
- The guard is **generated**; a rule changed in the generated file and not in the
  generator will be lost on the next regeneration. That is not hypothetical — it
  happened once and the sweep caught it.
- The predecessors' closure records are **not** rewritten.
