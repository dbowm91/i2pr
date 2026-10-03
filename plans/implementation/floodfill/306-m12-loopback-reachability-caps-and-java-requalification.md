# Plan 306 — M12 loopback reachability-caps design and Java requalification

Status at registration:
**registered-m12-loopback-reachability-caps-and-java-requalification**

Classification: narrow design + dual-lane requalification. Follows the Plan
279 stop (`plans/closure/floodfill/279-status.md` §§2–3).

Hard dependencies: Plan 279 stopped with the exact boundary recorded (Java
never initiates toward the caps-`f`-only controlled RI; zero matrix rows
reachable). Plans 278/303 (one-family line) unchanged; Plan 279 §4 normal
opt-in code retained as tested default-off infrastructure.

## 1. Objective

Decide, implement behind the existing eligibility/permit gates, and
requalify one bounded thing: what a loopback controlled RouterInfo may
truthfully advertise so a stock second family initiates to it — then repeat
the Java matrix and the i2pd requalification on the closing head.

## 2. Why-ready (what Plan 279 proved)

- The Java lane is fully built (mesh, rendezvous, census, withdrawal,
  checker with self-test) and fails at a single localized gate: initiation.
  J219 shows P loaded, verified, floodfill-listed — never dialed.
- The address entry is complete and dialable (i2pd proves it); the router
  caps carry `f` only. Stock Java gates initiation/selection on caps
  letters (`R` at `TunnelPeerSelector.allowAsIBGW`; bandwidth tier
  derivation), all of which Plan 101 forbids in controlled options.
- No i2pr wire defect is evidenced (zero packets from JC); no production
  change was made to go green. The question is now a design decision, not
  a diagnosis.

## 3. Design decision (required before any lane run)

Record as an ADR or a reviewed amendment to the Plan 101 posture, minimally:

- `R` (reachable) is TRUE in lane scope iff inbound SSU2 is proven (a
  completed inbound session, as the i2pd lane already demonstrates). Scope
  the advertisement to controlled/loopback publication; public-network
  advertisement still requires the standing corroboration bar.
- Bandwidth tiers (`L/M/N/O/P/X`) and all other forbidden letters stay
  forbidden. If Java additionally requires a tier, this plan STOPS and
  reports that instead of fabricating one.
- The decision must state why `R`-in-lane-scope is truthful (loopback
  acceptance is demonstrated, never leaves loopback, seeded by file) and
  why it does not weaken the no-false-advertisement invariant.

## 4. Production changes (only what §3 authorizes)

- `crates/i2pr-netdb/src/local.rs` (`validate_options` and the
  `build_floodfill` caps path): admit exactly the decided letters under
  exactly the decided evidence, with a test that fails on anything wider.
- Activation (`crates/i2pr-daemon/src/floodfill.rs`): supply the decided
  caps only when the decided evidence holds; loopback evidence rules must
  be explicit and separate from public-network rules.
- Nothing else: no wire-format change, no selector change, no reference
  change, no budget-rule change.

## 5. Work packages (ordered)

1. ADR/amendment + caps implementation + unit tests (narrow admission,
   wider-cap rejection, withdrawal path unchanged).
2. Routine floor on the implementation head.
3. i2pd requalification on the closing head (fresh frozen budget 1;
   Plan 279 §12 criterion 1 and the §13 staleness note). Stop on any
   regression vs Plan 303.
4. Java requalification (fresh frozen budget 3; same runner, same guarded
   rows, same pins; poll lane ports free before phase 0; record
   per-attempt CREATE classes per 279-status §9).
5. Closure with the requirement matrix of Plan 279 §12 re-evaluated.

## 6. Failure / cancellation / restart / contention

- Any lane attempt that stops before matrix A keeps its recorded delta
  rule; blind re-runs are forbidden. Spent budgets (279's 3) are not
  widened — these are fresh budgets owned by this plan.
- If the decided caps do not unlock Java initiation, STOP (do not iterate
  letters): record the negative result and return the question to planning.
- If any production regression appears (fmt/check/tests/clippy/boundaries),
  stop and fix before any lane run.

## 7. Compatibility and migration

Additive and default-false. No existing configuration changes behavior.
Controlled RIs stay loopback-scoped and file-seeded; no public-network
participation is authorized by this plan.

## 8. Required tests

- caps admission: decided letters pass validation iff evidence holds;
  every other forbidden letter still fails;
- `build_floodfill` emits the decided caps exactly; withdrawal emits none;
- existing `floodfill_controlled_lifecycle`, `floodfill_normal_optin`,
  netdb, and daemon suites stay green;
- the static checker (`check-m12-floodfill-qualification-evidence.sh
  --self-test`) still passes unchanged (no new guarded rows unless the
  plan says so before the attempt).

## 9. Required verification

Local floor (`AGENTS.md` routine floor in full) on the implementation head
and again on the closing head, plus:

```bash
bash tests/integration/floodfill/run-i2pd.sh
bash tests/integration/floodfill/run-java-floodfill.sh
```

each under its fresh frozen budget, then exact-head ordinary CI green.

## 10. Documentation

Update the Plan 101 posture note (or the new ADR), the floodfill roadmap,
`specs/support.toml`/`specs/CONFORMANCE.md` exactly to what was qualified
(and no further), README limitations, and the M13 unblock audit.

## 11. Acceptance criteria

M12 advances toward closure iff, on one closing head: (1) i2pd requal
passes; (2) the Java matrix passes all guarded rows including the
LeaseSet-family census and withdrawal; (3) caps policy is reviewed and
minimal; (4) no critical/high finding remains. Otherwise this plan stops
with its own status record and M12 stays open.

## 12. Stop conditions

Do not advertise beyond the decided letters. Do not waive ADR 0026. Do not
run public-network qualification. Do not reuse spent budgets.

## 13. Closure evidence required

Both reference pins, qualification SHAs, exact commands with outcomes,
evidence indexes (sanitized only), the caps decision record with review
trail, CI run, security/resource/migration review, and the registry/roadmap
unblock audit for M12/M13.

## 14. Handoff

On pass, M12 may proceed to closure review and M13 becomes the planning
frontier. On stop, record the exact boundary the same way Plan 279 did and
register the next narrow step (or retain the stop).
