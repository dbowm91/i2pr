# Plan 269 — Post-M11 global roadmap and support-authority reconciliation

Status at registration:
**registered-post-m11-global-roadmap-support-reconciliation-ready**

Classification: **polish / planning-authority reconciliation only**.

Hard dependency: Plan 268
(`passed-m11-receipt-family-three-completions-zero-semantic-failures-path-divergence-falsified-exact-head-ci-green`).

## 1. Objective

Reconcile stale repository-wide "current state", support, roadmap, and next-work prose with
the authoritative Plan 268 M11 closure without changing historical evidence, protocol
semantics, runtime behavior, feature advertisement, or the M12 architecture.

The resulting repository must have one unambiguous present-tense execution state:

```text
M11 one-family experimental qualification = passed via Plan 268
M11 public transit capability              = unclaimed / disabled / non-advertised
M12 floodfill                              = Plans 270-279 registered but execution-blocked behind Plan 269
next executable product plan               = Plan 269 cleanup; Plan 270 becomes ready only after Plan 269 closes
```

This plan deliberately precedes M12 architecture registration so stale historical prose
cannot silently become an M12 requirement.

## 2. Why ready

- Plan 268 is passed and is the terminal M11 corrective.
- Plan 268's §8 authority transition explicitly makes M12 floodfill planning
  dependency-ready.
- Exact-head ordinary CI for the Plan 268 closure is green (run `36811447204` on
  `778818b`), with the follow-up authority reconciliation also green on
  `8106eef` (run `36863248021`).
- No production defect is being corrected here.
- The stale statements are identifiable documentation/planning drift: the old Plan 118
  "Current execution state", old protocol-support "next executable" prose, and support
  inventory fields that still name M11 as the next product layer.

## 3. Current implementation evidence to preserve

Do not reinterpret or rewrite:

- Plan 193 exact-pinned i2pd M6 mixed-router progression evidence.
- Plan 247 Java full-router compatibility as retained/deferred nonblocking debt.
- Plans 214/215 M10 product authority.
- Plans 249–267 retained M11 evidence and their exact historical failure/stop boundaries.
- Plan 268's eight retained receipt attempts, 3/8 genuine completions, zero i2pr semantic
  failures, 353/353 dispatched install paths, passed composition, and exact-head CI.
- The distinction between experimental progression evidence and full two-family router
  conformance under ADR 0026.
- The fact that ordinary product transit remains disabled and non-advertised.
- Existing historical Plan 115–118 and early M3/M5 evidence gaps.

## 4. Invariants

1. Closure/status records remain authoritative over prose.
2. Historical documents may retain dated statements when clearly labeled historical.
3. No historical failed/blocked/retained result may be relabeled as passed.
4. No document may claim public transit participation, production readiness, anonymity,
   privacy, or full router conformance.
5. No RouterInfo capability, router.version, daemon default, transport default, config
   field, protocol codec, test harness, reference pin, dependency, or Rust production
   source changes.
6. M12 remains unimplemented by this plan; owner-authorized successor Plans 270-279 may be registered as blocked and must not execute before this plan closes.
7. "M12 planning dependency-ready" must not be rewritten as "M12 implemented", "M12
   active", or "floodfill supported".
8. The manual M11 external workflow's zero-job push failure remains documented as a
   workflow-level artifact and is not converted into an ordinary-CI blocker.

## 5. In scope

Audit and reconcile present-tense or machine-consumed execution/support statements in:

- `README.md`
- `plans/registry.md`
- `plans/implementation/workspace-foundation/000-mvp-roadmap.md`
- `plans/subsystems/*-roadmap.md` where they contain global "next/current" claims
- `docs/protocol-support.md`
- `docs/architecture.md` and `docs/architecture/` only where current milestone/support
  statements are stale
- `specs/CONFORMANCE.md`
- `specs/support.toml`
- `specs/protocols/04-reseed-netdb.md` and `05-tunnels.md` only where they expose
  repository-current status
- `AGENTS.md` only if it contains active-plan or next-plan guidance

Classify each candidate statement as one of:

```text
historical dated evidence
current implementation/support authority
current execution/planning authority
future milestone specification
obsolete execution instruction
```

Only current authority or obsolete instruction statements require correction when they
conflict with Plan 268; historical snapshots stay historically truthful.

## 6. Explicitly out of scope

- Designing M12 floodfill architecture.
- Executing any M12 implementation plan. Owner-authorized Plans 270-279 may exist in the registry as blocked successors.
- Implementing floodfill storage, lookup serving, replication, capability advertisement,
  persistence, or role health.
- Reopening M11 qualification.
- Retrying Java, NTCP2, or the M11 external lane.
- Editing retained closure evidence to make old prose appear current.
- Broad documentation rewriting unrelated to current-state authority.

## 7. Work packages

### WP A — current-state inventory

Search all tracked Markdown/TOML planning/support surfaces for stale terms and plan
authorities, at minimum:

```text
Plan 118
Plan 119
next executable
next product layer
M6 ... next
M11 ... next
M12
live mixed-router ... pending
transit participation ... pending
floodfill ... deferred
```

Produce a bounded before/after inventory in the Plan 269 closure record. Dated historical
narrative may remain when its date/context makes it non-authoritative.

### WP B — global roadmap reconciliation

Update the global MVP roadmap so its original milestone definitions remain intact while
its "Current execution state" section reflects the current authority through Plan 268.
Do not rewrite the historical Plan 118 state as though it never existed; move or label it
as a historical snapshot if useful.

The current-state summary must identify M12 as the next **planning frontier**, with M13 and
M14 still future.

### WP C — protocol/support reconciliation

Update `docs/protocol-support.md` and `specs/support.toml` so current rows accurately
reflect the closed M6–M11 progression while preserving bounded-support language.

At minimum:

- remove obsolete "next Plan 119/123/125" present-tense guidance;
- replace `next_product_layer = "m11-transit-tunnels"` with a value consistent with the
  Plan 268 transition;
- reconcile next-executable/current-planning fields with the owner-authorized state: Plan 269 is the sole ready plan, while M12 Plans 270-279 are registered but blocked until this cleanup closes;
- update the tunnel/transit support summary so it no longer says live transit execution
  is pending where Plan 268 now supplies controlled one-family experimental evidence;
- keep public transit `advertised = false` and capability unclaimed.

### WP D — canonical/conformance consistency

Audit `specs/CONFORMANCE.md` and the relevant protocol dossiers for any current-state
sentence that conflicts with the Plan 268 closure. Change only status/progression prose;
do not alter normative protocol requirements in this plan.

### WP E — stale-instruction guard

Add or extend a lightweight documentation/planning checker only if an existing checker
surface can express the invariant without creating substantial new infrastructure.
Preferred invariant: machine-consumed support/registry fields must not simultaneously name
a closed milestone as both current-next and completed.

If no appropriate existing checker exists, record the review grep/query in closure
evidence rather than adding a bespoke framework.

## 8. Failure, cancellation, restart, and contention semantics

This plan changes documentation only. It adds no runtime task, channel, queue, lock,
persistence format, or cancellation path.

If main advances while the pass is executing, rebase the inventory against the new head and
re-evaluate only conflicting authority statements. Never overwrite a newer closure/status
record with this plan's snapshot.

## 9. Compatibility and migration

No user migration, storage migration, config migration, wire compatibility change, or API
compatibility change.

Historical links and global plan numbers must remain stable.

## 10. Required verification

At minimum:

```bash
git diff --check
git diff --name-only <plan269-parent>..HEAD -- 'crates/**' 'tests/**' 'tools/**' '.github/**' 'Cargo.toml' 'Cargo.lock'
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-m11-transit-boundaries.sh
```

The production/test/workflow diff command must be empty. If a pre-existing documentation
checker exists for support/conformance, run it as well. Parse `specs/support.toml` with
the repository's normal validation path.

No external-router execution is required.

## 11. Documentation updates

The closure must identify every changed authority surface and distinguish:

- corrected current-state text;
- retained historical snapshots;
- future milestone definitions left unchanged.

It must explicitly state that M12 architecture remains a separate planning decision.

## 12. Acceptance criteria

Plan 269 closes only when:

1. all repository-wide present-tense execution-state surfaces agree that Plan 268 closed
   M11 experimental progression;
2. M11 transit remains disabled, non-advertised, and not claimed as public capability;
3. M12 floodfill is consistently described as planned with Plans 270-279 registered but blocked behind Plan 269;
4. no stale current-state surface identifies M6, Plan 119/123/125, or M11 as the next
   product implementation frontier;
5. `specs/support.toml`, `plans/registry.md`, README, global roadmap, conformance prose,
   and protocol-support prose do not contradict one another;
6. historical closure evidence is unchanged;
7. no production/test/workflow/Cargo dependency file changes;
8. local structural verification is green;
9. no critical/high documentation-authority ambiguity remains;
10. the unblock audit moves only Plan 270 to ready after Plan 269 closes; Plans 271-279 remain blocked on their declared predecessors.

## 13. Stop conditions

Stop and open a separate architecture/ADR decision instead of normalizing prose if:

- two canonical surfaces impose materially different M12 requirements;
- reconciliation would require changing a normative protocol rule rather than status text;
- a support claim cannot be backed by an existing closure record;
- an M11 capability/default/advertisement change appears necessary.

## 14. Closure evidence required

The Plan 269 status record must contain:

- exact parent and closure commit;
- file-by-file authority reconciliation table;
- stale-current-state search results before and after;
- commands run and outcomes;
- confirmation of empty production/test/workflow/dependency diff;
- support/conformance consistency review;
- unblock audit stating that Plans 270-279 remain registered in dependency order and that closure moves only Plan 270 to ready.

## 15. Handoff

Plans 270-279 are owner-authorized blocked successors. After Plan 269 closes, the unblock audit moves only Plan 270 (M12 architecture authority) to ready. Plan 269 itself authorizes no M12 production implementation.
