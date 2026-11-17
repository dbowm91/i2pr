# Plan 538 — the NetDB's floodfill statistic must be accounted, not scanned

**Closure record.** The plan of record is
`plans/implementation/network-participation/538-the-netdb-floodfill-statistic-must-be-accounted-not-scanned.md`.
This record is the authority on what the plan closed, what it did not, and on
what the evidence is.

## Status

Plan 538 is **closed**. The NetDB's floodfill advertiser count is **accounted**
rather than derived by a scan: the validated store maintains it on the same
paths that maintain its encoded byte total, so `stats()` is **O(1)** where it
was O(the peer population). And the daemon's bootstrap computes the store's
statistics **once** per operation, so the router's NetDB lock is no longer held
across a population walk to obtain a number.

Classification: **corrective** (the store already accounted its byte total
incrementally, while the floodfill advertiser count — the second number the
same struct reports about the same set — was derived by walking every record
on every call; and the bootstrap read that statistic while holding the router's
NetDB lock, and read it twice per state computation).
**Subsystem:** network-participation. **Closure date:** 2026-11-11.

Nothing is promoted: `router.version` stays `0.9.62`, the inventory's
advertised count stays **zero**, and `Cargo.toml` stays byte-identical.
`Cargo.lock` differs from the pre-plan lock by **exactly** one declared
transitive security update, `rustix 1.1.5` to `rustix 1.1.6`, which the
floor's `cargo deny` step required; see **The transitive security update the
floor required** below.

## What the plan closed

**The tree no longer carries the crate with a published unsound advisory.** The
floor's `cargo deny check advisories bans sources` step failed closed on
RustSec's RUSTSEC-2026-0486, which names `rustix 1.1.5`. The plan carries the
upstream fix -- `rustix 1.1.6` -- and the advisory check is **not** narrowed;
see **The transitive security update the floor required** below.

**The store accounts the count.** `RouterInfoStore`
(`crates/i2pr-netdb/src/store.rs:83`) carries a
`floodfill_advertiser_count` field beside `total_encoded_bytes`
(`crates/i2pr-netdb/src/store.rs:92`), and the same discipline applies to both.
`insert` (`crates/i2pr-netdb/src/store.rs:126`) accounts the incoming
advertiser on the **new-key** path and the **capability delta** on the
**replacement** path, because a replacement of the same identity may publish a
different capability set. `remove` (`crates/i2pr-netdb/src/store.rs:207`)
releases the advertiser it removed, and the store's own `prune` goes through
`remove`, so it accounts as it removes. The adjustment
(`crates/i2pr-netdb/src/store.rs:230`) is **checked** in both directions and
refuses an impossible adjustment rather than wrapping it.

**`stats()` is O(1).** It reads the maintained counter and walks **no** record
map (`crates/i2pr-netdb/src/store.rs:270`). The record map's *length* is O(1)
and is not a walk; a walk is a scan of the records themselves.

**The count is exact.** Not a sample, not a cap, not an estimate. The
bootstrap's `BootstrapState` decision compares the floodfill advertiser count
against the configured floor, so an inexact count would change when the router
believes it is ready.

**The bootstrap computes the statistics once.** The refresh now **takes** the
statistics it refreshes (`crates/i2pr-daemon/src/bootstrap.rs:565`), so every
call site computes them once and the snapshot's counts and the computation's
counts are the same numbers by construction. The state computation
(`crates/i2pr-daemon/src/bootstrap.rs:445`) reads them **once** and decides on
that one computation, so it can no longer disagree with the snapshot it just
refreshed.

**The router's NetDB lock is no longer held across a population walk to obtain
a statistic.** The bootstrap's mutex *is* the router's NetDB lock — it guards
the only `RouterInfoStore` in the daemon
(`crates/i2pr-daemon/src/bootstrap.rs:215`) — and the bootstrap read the
statistics at five points, every one of them under that lock. The prune is on
the router's **live maintenance path**, so the lock was held across the
population walk on every reconcile pass that prunes.

**The public surfaces are untouched.** The bootstrap's `snapshot()`,
`diagnostics()` (`crates/i2pr-daemon/src/bootstrap.rs:574`),
`store_summary()` (`crates/i2pr-daemon/src/bootstrap.rs:700`) and the
`BootstrapSnapshot` fields (`crates/i2pr-daemon/src/bootstrap.rs:143`) are
exactly what the tree already had, and so are the store's selection paths —
`floodfill_advertisers()` (`crates/i2pr-netdb/src/store.rs:313`) still yields
exactly the advertisers and `iter()` still yields every record.

## What the plan did not close

- **The store's prune is still O(population) under the router's NetDB lock.**
  This plan removes the *statistic* scan; the prune's own examination of every
  record remains, because a prune must examine every record. Moving the prune
  out of the lock is a consequence of the router having a **NetDB owner of its
  own**, which is the larger successor Plans 536 and 537 both named.
  **Owner:** the successor that gives the router a NetDB owner of its own.
- **The router's validated store is still the bootstrap's store.** The
  bootstrap owns the only `RouterInfoStore` in the daemon
  (`crates/i2pr-daemon/src/bootstrap.rs:215`). Separating the router's NetDB
  from the bootstrap's lifetime is a larger change and was not attempted here.
  This is Plan 536's recorded **Low** finding 1, carried forward unchanged.
  **Owner:** the same successor.
- **The composition root's shape is proven statically, not behaviourally.**
  This is Plan 536's recorded **Low** finding 2, carried forward unchanged.
  **Owner:** the successor that makes the composition root's relay arm
  reachable from a test.
- **The production SSU2 peer-test driver** — Plan 530's Low finding 2.
- **The `B` advertisement and `caps=f`** — their own owners.
- **The stock-peer half** — Plan 493's authorized two-host topology.

## Implementation commits

| Commit | What it carries |
| --- | --- |
| `d0273251` | the plan of record, the registry's row and the roadmap's row |
| `0ec88980` | the production change: the store's maintained count and its O(1) statistics, and the bootstrap's single computation per operation |
| `32473796` | the plan's lane and guard surface, settled by the implementation, and the plan's amendment log |
| `ae5920b5` | the lane and the guard |
| `74302efe` | the documentation, the inventory and the plan's amendment log |
| `7b3b60c3` | the declared extension of Plan 507's retention boundary guard, and Plan 538's own guard's tenth rule that pins it |
| `c7b55ac3` | the declared transitive security update the floor's `cargo deny` step required, and the guard's rule that pins the patched version |

The floor ran on the committed tree `7b3b60c3`. The closure commit adds **only**
this record and the two closed rows, exactly as Plan 537's closure commit did.

The plan of record's **invariants** are at
`plans/implementation/network-participation/538-the-netdb-floodfill-statistic-must-be-accounted-not-scanned.md:153`,
its **design decisions** at
`plans/implementation/network-participation/538-the-netdb-floodfill-statistic-must-be-accounted-not-scanned.md:344`,
its **amendment log** at
`plans/implementation/network-participation/538-the-netdb-floodfill-statistic-must-be-accounted-not-scanned.md:641`,
its **declaration of the extension of Plan 507's guard** at
`plans/implementation/network-participation/538-the-netdb-floodfill-statistic-must-be-accounted-not-scanned.md:237`,
its **declaration of the transitive security update** at
`plans/implementation/network-participation/538-the-netdb-floodfill-statistic-must-be-accounted-not-scanned.md:265`,
the netdb deep-dive's account of the new shape at
`docs/architecture/i2pr-netdb.md:1451`, the daemon deep-dive's account of the
bootstrap's new shape at `docs/architecture/i2pr-daemon.md:4418`, and the
qualification gates at `specs/QUALIFICATION-GATES.md:2118`.

## Requirement-to-evidence matrix

| # | Requirement | Evidence |
| --- | --- | --- |
| 1 | the count is **accounted**, not derived, and it equals the number of retained records that advertise `f` at every point in time | the lane's rows `a_new_key_insert_accounts_the_advertiser`, `a_replacement_that_gains_the_capability_accounts_the_advertiser`, `a_replacement_that_loses_the_capability_releases_the_advertiser`, `a_removal_releases_the_advertiser` and `a_prune_that_removes_several_records_releases_every_advertiser`; the guard's rule `the_insert_and_remove_paths_maintain_the_count` |
| 2 | the count is **exact** — no sampling, no cap, no estimate | the lane's row `the_count_is_exact_for_a_large_population` |
| 3 | `stats()` is **O(1)**: it reads maintained counters and never walks the record map | the guard's rule `the_store_maintains_the_floodfill_advertiser_count` |
| 4 | the statistics are computed **once** per operation | the guard's rule `the_bootstrap_computes_the_statistics_once` |
| 5 | the router's NetDB lock is never held across a population scan to obtain a statistic | the guard's rules `the_store_maintains_the_floodfill_advertiser_count` and `the_bootstrap_computes_the_statistics_once`, together with the production change at `crates/i2pr-netdb/src/store.rs:270` and `crates/i2pr-daemon/src/bootstrap.rs:565` |
| 6 | the store's public surface is unchanged | the guard's rules `the_stores_selection_surface_is_unchanged` and `the_bootstrap_public_surface_is_unchanged`; the lane's row `the_selection_surface_is_unchanged` |
| 7 | the bootstrap's snapshot semantics are unchanged | the lane's rows `the_bootstrap_reports_the_same_count` and `the_bootstrap_reaches_the_same_state`; the guard's rule `the_bootstrap_public_surface_is_unchanged` |
| 8 | no public API is removed or renamed | the guard's rules `the_stores_selection_surface_is_unchanged` and `the_bootstrap_public_surface_is_unchanged` |
| 9 | no configuration key is added, and no dependency is added | the guard's rule `no_new_surface_and_the_manifest_is_unchanged`, and the two digests below |
| 15 | the lock carries the patched `rustix` the floor's `cargo deny` required, and **not** the version the advisory names | the guard's rule `no_new_surface_and_the_manifest_is_unchanged`, and the two version probes |
| 10 | no new task, timer, listener or socket | the guard's rule `no_new_surface_and_the_manifest_is_unchanged` |
| 11 | no advertisement | the guard's rule `the_advertisement_is_unchanged`; `crates/i2pr-netdb/src/local.rs:172` |
| 12 | the selection paths are untouched | the guard's rule `the_stores_selection_surface_is_unchanged`; the lane's row `the_selection_surface_is_unchanged` |
| 13 | Plan 536's and Plan 537's evidence is byte-identical | the guard's rule `the_predecessors_evidence_is_untouched` |
| 14 | the extension of Plan 507's retention boundary guard is **exactly** the declared one, and the extended guard is pinned | the guard's rule `the_extension_of_plan507s_guard_is_the_declared_one` |

The two digests the guard pins are the manifest's
`39c44e33add62693eef8866220de17dcfe137437b62732e5044fcaefc4870269` and the
lock's
`f0cec0685c4ed460c7d63273cc318f7a683dc2357735d7edaa6dfa1ac243d833`.

The lock's digest is **not** the pre-plan lock's digest, and that is the
declared change the floor required; see **The transitive security update the
floor required** below.

The guard's tenth rule pins a third digest: the extended Plan 507
retention boundary guard's, which is
`5ef83044dcb8036e0ac2ccc8e7e9cb9bf6b7a21127b822b6e634da6170fdd78d`. A later change to that guard must change the pinned constant
**and** say why.

## Focused verification

| Command | Outcome |
| --- | --- |
| `cargo fmt --all --check` | PASS |
| `cargo check --locked --workspace --all-targets` | PASS |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --locked -p i2pr-daemon --test floodfill_statistic_accounting -- --test-threads=1` | PASS — 14 rows |
| `cargo test --locked -p i2pr-daemon --test floodfill_statistic_accounting_boundary -- --test-threads=1` | PASS — 32 rows |
| `cargo test --locked -p i2pr-daemon --test netdb_retention_boundary -- --test-threads=1` | PASS — 26 rows |
| `cargo test --locked -p i2pr-netdb --all-targets` | PASS |
| `cargo test --locked -p i2pr-daemon --lib -- --test-threads=1` | PASS — 1001 rows |
| `cargo test --locked -p i2pr-daemon --test ssu2_relay_requester_lock --test ssu2_relay_requester_lock_boundary --test ssu2_relay_target_lock --test ssu2_relay_target_lock_boundary -- --test-threads=1` | PASS — 7 rows, 33 rows, 7 rows and 29 rows |
| `python3 scripts/check-tooling-inventory.py` | PASS |
| `python3 scripts/check-global-plan-number-uniqueness.py` | PASS |
| `python3 scripts/check-license-metadata.py` | PASS |
| `python3 -m unittest discover -s tests/planning -p 'test_*.py'` | PASS — 51 tests |
| `python3 /tmp/538-negative.py` | PASS — 17 controls, each firing the surface that owns the defect, each restored byte-identically |

The closure record's **citations** are verified mechanically, not by reading.
The verifier recomputes every cited line from the tree and requires the record
to name exactly that line:

- the **closure record's** citations are recomputed against the **current
  tree** — the first line that carries the cited text is the line the record
  must name;
- the **plan of record's** citations are recomputed against the **registration
  commit** `d0273251`, because the plan's §3 is the evidence of the tree
  *before* the plan and is never rewritten;
- the **lane's and the guard's rows** are checked by name, so the record
  cannot claim a row the evidence file does not carry;
- the **manifest's and the lock's digests** are checked, so the record cannot
  claim byte-identity the tree does not have.

One rule is scoped rather than absolute, and the scope is stated. The rule is
that **no citation may be a bare `:line` shorthand**, because a shorthand is not
mechanically checkable and a reader cannot confirm it. The plan's body — §1
through §16 and §17.1 — carries **no** bare shorthand at all. The plan's
amendment log is the one place where a bare token is **quoted** rather than
cited: row 17.2.6 reports the drift the plan itself found — that §3 had been
written with bare line numbers — and a findings row quotes the form it
reports. The scoped check therefore requires that every bare shorthand in §17.2
sits on a line that also names the shorthand form it reports, so the token can
only be there as the quoted form of the drift the log records. Both checks are
mechanical and fail closed; neither is an unbounded exemption.

## The routine floor

The floor ran on the committed tree `c7b55ac3` and it is **green**. Every
step is recorded in `/tmp/538-floor.log`, with each step's own log under
`/tmp/538f-<step>.log`. The floor is a plain sequence of `step` runs that
records each step's exit status; no step is skipped, no `|| true`, no
`continue-on-error` and no filename filtering. The workspace step runs with
`--no-fail-fast`, so every test binary's result is reported rather than the
run stopping at the first failure.

Plan 538's floor is Plan 537's floor plus two steps -- the daemon lane and
the daemon guard for the NetDB's floodfill statistic accounting. **No other
step changed.** The only other plan's guard file this plan touches is Plan
507's retention boundary guard, and only its rule 7 **anchor**, exactly as
the plan's section 5.4 declares; Plan 536's and Plan 537's lanes and guards
stay byte-identical.

| Step | Outcome |
| --- | --- |
| `floor-tree-clean` | PASS |
| `fmt` | PASS |
| `check` | PASS |
| `app-siblings` | PASS |
| `app-fixture-static` | PASS |
| `app-fixture-copy` | PASS |
| `app-fixture-rebuild` | PASS |
| `plan522-peer-pool-lane` | PASS |
| `plan522-peer-pool-boundary` | PASS |
| `plan522-peer-pool-unit` | PASS |
| `transport-ssu2-unit` | PASS |
| `runtime-unit` | PASS |
| `netdb-unit` | PASS |
| `plan523-introducer-lane` | PASS |
| `plan523-introducer-boundary` | PASS |
| `plan524-introducer-request-lane` | PASS |
| `plan524-introducer-request-boundary` | PASS |
| `plan520-publication-lane` | PASS |
| `plan520-publication-boundary` | PASS |
| `plan525-introducer-publication-lane` | PASS |
| `plan525-introducer-publication-boundary` | PASS |
| `plan526-introducer-advertisement-lane` | PASS |
| `plan526-introducer-advertisement-boundary` | PASS |
| `plan521-peer-test-role-boundary` | PASS |
| `plan527-reachability-contradiction-lane` | PASS |
| `plan527-reachability-contradiction-boundary` | PASS |
| `plan528-peer-test-verdict-lane` | PASS |
| `plan528-peer-test-verdict-boundary` | PASS |
| `plan529-observed-address-lane` | PASS |
| `plan529-observed-address-boundary` | PASS |
| `plan530-explicit-bind-lane` | PASS |
| `plan530-explicit-bind-boundary` | PASS |
| `plan531-relay-requester-lane` | PASS |
| `plan531-relay-requester-boundary` | PASS |
| `plan532-relay-target-lane` | PASS |
| `plan532-relay-target-boundary` | PASS |
| `plan533-relay-establishment-lane` | PASS |
| `plan533-relay-establishment-boundary` | PASS |
| `plan534-relay-session-peer-lane` | PASS |
| `plan534-relay-session-peer-boundary` | PASS |
| `plan535-explicit-bind-refresh-lane` | PASS |
| `plan535-explicit-bind-refresh-boundary` | PASS |
| `plan536-relay-target-lock-lane` | PASS |
| `plan536-relay-target-lock-boundary` | PASS |
| `plan537-relay-requester-lock-lane` | PASS |
| `plan537-relay-requester-lock-boundary` | PASS |
| `plan538-floodfill-statistic-lane` | PASS |
| `plan538-floodfill-statistic-boundary` | PASS |
| `test-workspace` | PASS |
| `clippy` | PASS |
| `doc` | PASS |
| `test-doc` | PASS |
| `dependency-direction` | PASS |
| `plan-number-uniqueness` | PASS |
| `adr-number-uniqueness` | PASS |
| `portable-service-tunnel-api` | PASS |
| `portable-service-tunnel-consumer` | PASS |
| `planning-tests` | PASS |
| `runtime-boundaries` | PASS |
| `console-boundaries` | PASS |
| `console-browser-security` | PASS |
| `tooling-inventory` | PASS |
| `license-metadata` | PASS |
| `managed-app-private-client-seams` | PASS |
| `router-readiness-contract` | PASS |
| `router-readiness-self-test` | PASS |
| `core-router-recovery-contract` | PASS |
| `core-router-recovery-self-test` | PASS |
| `plan449-ssu2-preflight` | PASS |
| `plan495-peer-eligibility` | PASS |
| `plan476-gate-binding` | PASS |
| `plan476-gate-binding-self-test` | PASS |
| `managed-app-process-boundary` | PASS |
| `managed-app-process-boundary-self-test` | PASS |
| `managed-app-package-boundary` | PASS |
| `managed-app-package-boundary-self-test` | PASS |
| `managed-app-policy-boundary` | PASS |
| `managed-app-policy-boundary-self-test` | PASS |
| `managed-app-secured-sandbox` | PASS |
| `managed-app-secured-sandbox-self-test` | PASS |
| `service-tunnel-boundaries` | PASS |
| `managed-app-gateway-boundary` | PASS |
| `managed-app-manager-boundary` | PASS |
| `m11-per-epoch-composition` | PASS |
| `service-anonymity-boundaries` | PASS |
| `fixture-manifest` | PASS |
| `ntcp2-vectors` | PASS |
| `ssu2-vectors` | PASS |
| `i2cp-vectors` | PASS |
| `ntcp2-interoperability` | PASS |
| `constrained-host-lane-boundary` | PASS |
| `m11-transit-boundaries` | PASS |
| `m11-transit-qualification-evidence` | PASS |
| `sam-acceptance-evidence` | PASS |
| `ssu2-acceptance-evidence` | PASS |
| `i2cp-acceptance-evidence` | PASS |
| `i2pcontrol-acceptance-evidence` | PASS |
| `els2-type11-transcript-boundary` | PASS |
| `els2-live-lane-evidence` | PASS |
| `encrypted-service-consumer-caller` | PASS |
| `outproxy-request-path` | PASS |
| `outproxy-wire-lane-evidence` | PASS |
| `config-secret-hygiene` | PASS |
| `workflow-validity` | PASS |
| `floodfill-type5-serve` | PASS |
| `service-tunnel-acceptance-evidence` | PASS |
| `exploratory-tunnel-evidence` | PASS |
| `netdb-tunnel-evidence` | PASS |
| `destination-tunnel-evidence` | PASS |
| `streaming-tunnel-evidence` | PASS |
| `m6-mixed-router-acceptance-evidence` | PASS |
| `m12-floodfill-qualification-evidence` | PASS |
| `m12-floodfill-boundaries` | PASS |
| `ntcp2-harness` | PASS |
| `cargo-deny` | PASS |

The floor's own summary is **115 passed, 0 failed**, and the table above lists every step it ran.

## The negative controls

Seventeen controls mutated the **real tree**, ran the lane or the guard, and
observed the expected row fail. Every control was then restored from the bytes
saved before the mutation, and the SHA-256 of every touched file was compared
with the value it had before, so the tree is provably byte-identical
afterwards. Every control **compiles**: a control that broke the build would
prove nothing about the row it names.

| Control | What it restored | What it broke |
| --- | --- | --- |
| NC-1 | the statistics walk the records again | the guard's rule `the_store_maintains_the_floodfill_advertiser_count` |
| NC-2 | the insert path stops accounting the new advertiser | the guard's rule `the_insert_and_remove_paths_maintain_the_count` |
| NC-3 | the replacement path stops accounting the capability delta | the guard's rule `the_insert_and_remove_paths_maintain_the_count` |
| NC-4 | the removal path stops releasing the advertiser | the guard's rule `the_insert_and_remove_paths_maintain_the_count` |
| NC-5 | the count's adjustment wraps instead of checking | the guard's rule `the_count_never_goes_below_zero` |
| NC-6 | the state computation reads the statistics twice | the guard's rule `the_bootstrap_computes_the_statistics_once` |
| NC-7 | the snapshot refresh reads the statistics itself | the guard's rule `the_bootstrap_computes_the_statistics_once` |
| NC-8 | the bootstrap's public surface changes | the guard's rule `the_bootstrap_public_surface_is_unchanged` |
| NC-9 | the store's selection surface stops filtering on the capability | the guard's rule `the_stores_selection_surface_is_unchanged` |
| NC-10 | the workspace manifest changes | the guard's rule `no_new_surface_and_the_manifest_is_unchanged` |
| NC-11 | the inventory advertises a surface | the guard's rule `the_advertisement_is_unchanged` |
| NC-12 | a predecessor's guard changes | the guard's rule `the_predecessors_evidence_is_untouched` |
| NC-13 | the store over-reports the advertiser count | the lane's row `a_record_without_the_capability_accounts_nothing` |
| NC-14 | the store under-reports on a replacement | the lane's row `a_replacement_that_gains_the_capability_accounts_the_advertiser` |
| NC-15 | the extended guard changes | the guard's rule `the_extension_of_plan507s_guard_is_the_declared_one` |
| NC-16 | the extended guard's rule 7 probe is absent | the same guard rule |
| NC-17 | the lock carries the version the advisory names | the guard's rule `no_new_surface_and_the_manifest_is_unchanged` |

The controls cover **both** halves of the evidence. NC-1 through NC-12 and
NC-15 and NC-16 restore a defect the **guard** is the authority for — the
shape. NC-13 and NC-14 restore a defect the **lane** is the authority for —
the count's exactness — and they are the two that move the number rather than
the structure, which is what makes the lane's independent count worth having.
NC-15 and NC-16 are the **extension's** controls: the first changes the
extended Plan 507 guard, and the second removes its rule 7 probe, and both
fire the plan's own tenth rule. NC-17 is the **security update's** control:
it restores the lock the advisory names, and the guard's rule 7 fires on the
version it carries. The lock it restores is one cargo accepts, so
`--locked` stays satisfied and the mutation compiles.

## Acceptance criteria

| # | The plan's criterion | Outcome |
| --- | --- | --- |
| 1 | the reported count is **exact** after every mutation path, and the lane proves it against an independent count | MET — the lane's fourteen rows, all of which compare the reported count with a count computed from the store's own iterator: `a_new_key_insert_accounts_the_advertiser`, `a_record_without_the_capability_accounts_nothing`, `an_idempotent_insert_leaves_the_count_unchanged`, `a_conflicting_insert_leaves_the_count_unchanged`, `a_stale_insert_leaves_the_count_unchanged`, `a_capacity_refusal_leaves_the_count_unchanged`, `a_replacement_that_gains_the_capability_accounts_the_advertiser`, `a_replacement_that_loses_the_capability_releases_the_advertiser`, `a_removal_releases_the_advertiser`, `a_prune_that_removes_several_records_releases_every_advertiser`, `the_count_is_exact_for_a_large_population`, `the_selection_surface_is_unchanged`, `the_bootstrap_reports_the_same_count`, `the_bootstrap_reaches_the_same_state` |
| 2 | `stats()` is **O(1)** and the guard proves it walks no record map | MET — the guard's rule `the_store_maintains_the_floodfill_advertiser_count`, and NC-1 restores the scan and fires it |
| 3 | the bootstrap computes the statistics **once** per operation, and the `BootstrapState` decisions are unchanged | MET — the guard's rule `the_bootstrap_computes_the_statistics_once`, NC-6 and NC-7, and the lane's row `the_bootstrap_reaches_the_same_state` |
| 4 | the store's and the bootstrap's **public surfaces** are unchanged, and no configuration key, task, timer, listener or socket is added | MET — the guard's rules `the_bootstrap_public_surface_is_unchanged`, `the_stores_selection_surface_is_unchanged` and `no_new_surface_and_the_manifest_is_unchanged` |
| 5 | `router.version` stays `0.9.62`, and the inventory's advertised count stays **zero** | MET — the guard's rule `the_advertisement_is_unchanged`, and NC-11 |
| 6 | the full floor is green, with the closure record's table complete | MET — the table above |
| 7 | the guard's inverted probes each fail as intended and the tree is restored byte-identically after each | MET — the seventeen controls above |
| 8 | the extension of Plan 507's retention boundary guard is **declared** in the plan of record, and it is **one anchor string** | MET — the plan's §5.4 and its amendment-log row 17.2.8; the guard's rule `the_extension_of_plan507s_guard_is_the_declared_one`, and NC-15 and NC-16 |
| 9 | the transitive security update the floor's `cargo deny` step required is **declared**, and the check is **not** narrowed | MET — the plan's §5.5 and its amendment-log row 17.2.9; the guard's rule `no_new_surface_and_the_manifest_is_unchanged`, which pins the patched `rustix` version as well as the lock's digest; `deny.toml` is unchanged, and the floor's `cargo deny check advisories bans sources` step passes |

## Invariant review

| # | The plan's invariant | Review |
| --- | --- | --- |
| 1 | the count is accounted, not derived | **Held.** The field is at `crates/i2pr-netdb/src/store.rs:92` and the three mutation paths maintain it. The lane proves the count equals an independent count after each path. |
| 2 | the count is exact | **Held.** No sampling, no cap, no estimate. The lane's large-population row compares the reported count with the independent count over two hundred records. |
| 3 | `stats()` is O(1) | **Held.** `crates/i2pr-netdb/src/store.rs:270` reads the maintained counter and the record map's length; it walks no map. The guard forbids the walk constructs outright. |
| 4 | the statistics are computed once per operation | **Held.** `crates/i2pr-daemon/src/bootstrap.rs:565` takes the statistics; `crates/i2pr-daemon/src/bootstrap.rs:445` reads them once. The guard requires every call site to pass a computed value. |
| 5 | the router's NetDB lock is never held across a population scan to obtain a statistic | **Held.** The only statistic the bootstrap reads is now O(1). |
| 6 | the store's public surface is unchanged | **Held.** `stats()` keeps its name, its return type and its fields; no item is removed or renamed. |
| 7 | the bootstrap's snapshot semantics are unchanged | **Held.** The same fields, the same values, the same refresh points and the same `BootstrapState` decisions. |
| 8 | no public API is removed or renamed | **Held.** The guard pins both public surfaces. |
| 9 | no configuration key is added, and no dependency is added | **Held.** `Cargo.toml` is byte-identical and the guard pins its digest. `Cargo.lock` differs from the pre-plan lock by **exactly** the declared transitive security update, and the guard pins its digest and the patched `rustix` version. |
| 10 | no new task, timer, listener or socket | **Held.** The two changed modules are runtime-neutral, and the guard forbids the infrastructure constructs in them. |
| 11 | no advertisement | **Held.** `crates/i2pr-netdb/src/local.rs:172` still declares `0.9.62`, and the inventory advertises nothing. |
| 12 | the selection paths are untouched | **Held.** `crates/i2pr-netdb/src/store.rs:313` still filters on the capability, and the lane's row `the_selection_surface_is_unchanged` observes it. |
| 13 | Plan 536's and Plan 537's evidence is byte-identical | **Held.** The guard's rule `the_predecessors_evidence_is_untouched` pins both pairs' digests. |
| 14 | the extension of Plan 507's retention boundary guard is exactly the declared one | **Held.** The plan's §5.4 declares it, the guard's rule `the_extension_of_plan507s_guard_is_the_declared_one` pins the extended guard's digest, and NC-15 and NC-16 fire it. |
| 15 | the lock carries the patched `rustix` and not the version the advisory names | **Held.** The lock carries `rustix 1.1.6` and does not carry `rustix 1.1.5`, the guard's rule `no_new_surface_and_the_manifest_is_unchanged` pins both facts, and the floor's `cargo deny check advisories bans sources` step passes. |

## Failure, cancellation, restart, and contention review

**Failure.** The store's accounting uses checked arithmetic in both directions
and refuses an impossible adjustment rather than wrapping it, so a count that
could not be maintained would fail loudly rather than under-report the
router's floodfill population. The daemon has no new failure mode: the
bootstrap's decisions are made on the same numbers as before.

**Cancellation.** The change touches no async path. The store's mutation
methods are synchronous and the bootstrap's refresh is synchronous, so there is
nothing to cancel and no lock held across an await.

**Restart.** Nothing is persisted. The count is rebuilt from the store's own
contents as the store is rebuilt, so a restart cannot inherit a stale count.

**Contention.** The router's NetDB lock is no longer held across a population
walk to obtain a statistic. The prune's own examination of every record
remains, because a prune must examine every record; that work is named in
**What the plan did not close** with its owner. The NetDB discovery tick, the
publication tick, the peer reconcile, the peer pool and the relay roles are no
longer queued behind a count.

## Migration and compatibility review

**No wire change.** The RouterInfo's bytes, the store's contents and the
bootstrap's decisions are exactly what the tree already produced.

**No configuration migration.** **No key is added.** The daemon's
configuration surface is unchanged, and the NetDB configuration keeps exactly
its twelve keys.

**No persisted state.** The count is derived from the store's contents and is
never persisted, so there is nothing to migrate.

**No change to the netdb crate's public surface.** `stats()` keeps its name,
its return type and its fields, and no `i2pr-netdb` item is removed or
renamed.

**No change to the bootstrap's public surface.** `snapshot()`,
`diagnostics()`, `store_summary()` and the `BootstrapSnapshot` fields are
unchanged, and the `BootstrapState` decisions are unchanged.

**No change to the manifest; the lock changes by exactly one declared update.**
`Cargo.toml` is byte-identical, and the guard pins its digest. `Cargo.lock`
differs from the pre-plan lock by **exactly** the transitive security update the
floor required -- `rustix 1.1.5` to `rustix 1.1.6` -- and the guard pins its
digest and the patched version. No requirement is added to any manifest, and
`deny.toml` is unchanged: the advisory check is not narrowed.

## Security review

**No secret is touched.** The store holds public RouterInfo material and the
count is a number about it. Nothing in this plan reads, writes or derives from
a private key, a signing seed or a session secret.

**No new disclosure.** The count is a count of public records that advertise
the `f` capability. It is exactly the number the store already reported, and
the plan removes no information from it: the reported value is unchanged.

**No new surface is reachable.** The plan adds no listener, no port, no socket
and no configuration key, so it opens no new way in.

**No peer is blamed.** The count is a fact about **this router's** NetDB, not
about any peer. The plan writes nothing into a peer's profile and creates no
peer-side condition.

**Fail closed.** The accounting refuses an impossible adjustment rather than
wrapping, and the guard fails closed on every item it names: a missing field,
a missing adjustment, a changed signature, a changed surface, a changed
manifest, an absent version and a changed predecessor's evidence each make the
rule that names them fail.

## Limitations

- The lane proves the count's **exactness** against an independent count; it
  cannot prove that `stats()` is O(1), because O(1) is a property of the
  method's shape. That is the guard's claim, and the guard's claim is a
  static one.
- The lane drives the store and the bootstrap **directly**. It does not drive
  the composition root's reconcile pass, so the *call path* that holds the
  router's NetDB lock across the prune is not exercised end to end. The
  composition root's shape is the guard's claim.
- The plan's effect on the lock's **contention** is structural rather than
  measured: no timing assertion is made, because a timing assertion under a
  shared disk would be flaky and would prove less than the shape does.
- The store's prune remains O(population) under the router's NetDB lock. The
  plan does not claim otherwise, and the work keeps its named owner.

## Findings by severity

### None — nothing was found that blocks the plan

### Medium — discharged

- **The tree carried a crate with a published unsound advisory.** RustSec's
  RUSTSEC-2026-0486 names `rustix 1.1.5`, which this tree's `Cargo.lock`
  carried, and the floor's `cargo deny check advisories bans sources` step
  failed closed on it. The advisory is not this plan's doing: the lock is
  byte-identical at the plan's registration commit. **Discharged in this plan**:
  the upstream fix, `rustix 1.1.6`, which is five lines of the lock, adds no
  requirement to any manifest, and leaves `Cargo.toml` byte-identical. The
  advisory check was **not** narrowed and `deny.toml` was **not** touched.
- **The statistic was derived by a scan, under the router's NetDB lock.** The
  daemon's bootstrap read the store's floodfill advertiser count while holding
  the router's NetDB lock, at five points, every one of them under that lock,
  and the count was computed by walking every record. The prune is on the
  router's live maintenance path, so the router's NetDB lock was held across
  the population walk on every reconcile pass that prunes. **Discharged in this
  plan**: the count is accounted, `stats()` is O(1), and the bootstrap computes
  the statistics once per operation.

### Low — recorded by this plan

- **The store's prune is still O(population) under the router's NetDB lock.**
  A prune must examine every record, so this is inherent to the operation and
  not a defect in the accounting. Moving it out of the lock needs the router to
  have a NetDB owner of its own. **Owner:** the successor that gives the
  router a NetDB owner of its own — the same successor Plans 536 and 537 named.
- **The router's validated store is still the bootstrap's store.**
  (`crates/i2pr-daemon/src/bootstrap.rs:215`) This is Plan 536's recorded
  **Low** finding 1, carried forward unchanged. **Owner:** the same successor.
- **The composition root's shape is proven statically, not behaviourally.**
  This is Plan 536's recorded **Low** finding 2, carried forward unchanged.
  **Owner:** the successor that makes the composition root's relay arm
  reachable from a test.
- **The production SSU2 peer-test driver** — Plan 530's Low finding 2.
- **The `B` advertisement and `caps=f`** — their own owners.
- **The stock-peer half** — Plan 493's authorized two-host topology.

## The transitive security update the floor required

The full floor's `cargo deny check advisories bans sources` step went **red**
on this tree. The cause is not this plan.

RustSec published
[RUSTSEC-2026-0486](https://rustsec.org/advisories/RUSTSEC-2026-0486) after
the tree was locked, and it names `rustix 1.1.5`, which this tree's
`Cargo.lock` carried. The advisory is an **unsound** one: `rustix`'s
extended-attribute functions mark unwritten bytes as initialised when they are
given a zero-length buffer, which is undefined behaviour reachable from safe
code.

**The plan did not cause it.** `Cargo.lock` is byte-identical at this plan's
**registration commit** `d0273251` and at the tree the floor ran on, so this
plan added no dependency, and the advisory names a tree this plan did not
change. The plan's own `Cargo.toml` was byte-identical throughout.

**What was done.** The upstream fix: `rustix 1.1.5` becomes `rustix 1.1.6`,
which is **five lines** of `Cargo.lock` -- the version, its checksum, and the
feature unification that moves `windows-sys 0.52.0` to `windows-sys 0.61.2`
for the three packages that share it. No requirement is added to any
manifest, `Cargo.toml` stays byte-identical, and the plan of record's §5.5
declares the change.

**What was deliberately not done.** No ignore entry was added to `deny.toml`,
and the `cargo deny` check was not narrowed, excluded, or waived. A published
unsound advisory is fixed at its source or it is not fixed, and the check is
not the plan's to relax.

**The tripwire.** The guard's rule `no_new_surface_and_the_manifest_is_unchanged`
pins the lock's digest **and** the patched `rustix` version, so the reason for
the lock's change cannot be quietly lost behind a digest.

## The extension of Plan 507's retention boundary guard

The full floor found a consequence this plan had not seen, and the plan's
§5.4 declares it.

Plan 507's retention boundary guard proves its rule 7 — *the router's own
RouterInfo is never a prune victim* — with an inverted probe that injects
`self.local.take();` into the bootstrap's **prune** path. The probe found that
path by anchoring on the literal `self.refresh_snapshot();`, and this plan
changed the bootstrap's refresh so that it **takes** the statistics it
refreshes. The anchor therefore moved to `self.refresh_snapshot(stats);`, and
the injected mutation moved with it.

**The extension is one anchor string in one inverted probe.** The rule, its
meaning, its coverage, its evidence file, the probe's intent and every other
row of that guard are exactly what they were, and its 26 rows all pass. Plan
538's own guard pins the extended guard's SHA-256 as a tripwire, and NC-15 and
NC-16 fire that rule.

The alternative was to keep a literal `self.refresh_snapshot();` in the
bootstrap for the anchor's sake. That was rejected: it would have been
cargo-culting a string the tree does not use.

## Roadmap disposition

The **network-participation** roadmap's row for Plan 538 moves from
`registered` to `closed`, and the registry's row moves from `registered` to
`closed`. No roadmap row is reopened, renumbered or rewritten, and no other
plan's row changes.

The plan **advances** the network-participation line's lock-contention work:
Plan 536 removed the relay target's answer from the router's NetDB lock, Plan
537 removed the relay requester's outbound step from it, and Plan 538 removed
the daemon's bootstrap's **population walk** from it. The line's remaining
named owner is unchanged: the successor that gives the router a NetDB owner of
its own.

## Unblock audit

- **Plan 538's own findings** are discharged above, and the two that remain
  have a named owner.
- **Plan 536's recorded Low finding 1** — the router's validated store is
  still the bootstrap's store — is **carried forward unchanged**, with the
  same owner.
- **Plan 536's recorded Low finding 2** — the composition root's shape is
  proven statically — is **carried forward unchanged**, with the same owner.
- **Plan 537's named out-of-scope items** — the NetDB ownership and the
  composition root's reachability — are **carried forward unchanged**, with
  the same owner.
- **Plan 530's Low finding 2** — the production SSU2 peer-test driver — stays
  with its owner.
- **Plan 493's stock-peer half** stays blocked externally, on Plan 493's
  authorized two-host topology. This plan needs no stock peer.
- **No plan is unblocked by this one.** The plan changes no advertisement, no
  capability, no version and no configuration, so it opens no gate for another
  plan.

## Handoff notes

- The plan is **experimental** and default-off. Do not describe the change as
  released, supported or advertised.
- The delivery is confined to the controlled loopback profile.
- The stock-peer half stays blocked on Plan 493's authorized two-host
  topology.
- Plan 536's closed closure record is **not** rewritten. The discharge of its
  Low findings lives in this plan's closure record and in the registry's row.
- Plan 537's closed closure record is **not** rewritten. The discharge of its
  named out-of-scope items lives in this plan's closure record and in the
  registry's row.
- The plan **extends** exactly one other plan's guard, and the plan's §5.4
  declares it: Plan 507's retention boundary guard's rule 7 inverted probe
  anchor moves with the bootstrap's refresh. Plan 536's lane and guard, and
  Plan 537's lane and guard, stay byte-identical, and the guard pins both
  pairs' digests.
- The plan carries **one** transitive security update, and the plan's §5.5
  declares it: `rustix 1.1.5` to `rustix 1.1.6`, which the floor's
  `cargo deny` step required. `Cargo.toml` is byte-identical, no dependency is
  added, and `deny.toml` is unchanged. A successor must **not** narrow the
  `cargo deny` check, and must not add an ignore entry for the advisory.
