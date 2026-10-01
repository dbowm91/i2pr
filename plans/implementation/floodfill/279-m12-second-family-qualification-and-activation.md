# Plan 279 — M12 second-family qualification, normal opt-in activation, and milestone closure

Status at registration:
**registered-m12-full-advertisement-blocked-on-plan278**

Classification: full router-to-router advertisement gate / milestone closure.

Hard dependency: Plan 278 passed.

## 1. Objective

Satisfy ADR 0026's second-independent-family requirement for broad truthful floodfill
advertisement, then expose a default-off normal-daemon floodfill configuration whose caps=f
advertisement remains conditional on runtime eligibility. Close M12 only after the private
mixed-router and adversarial exit criteria pass.

## 2. Independent reference

Primary second-family target: exact-pinned Java I2P 2.13.0 commit
9134f808337b401e8e53c73734c81fab04280c9d.

This plan is a NetDB/SSU2 qualification and must not resurrect the old Java Streaming scheduler
topology. Use the narrowest stock Java router configuration that can exchange the M12 NetDB
surface over a real authenticated router path.

I2P+ counts as the same implementation family and may substitute only through an explicit
plan amendment if stock Java has a proven reference-specific blocker. Emissary may count only if
its exact M12 surface is complete enough and a new authority decision records that fact.

## 3. Qualification matrix

Repeat the interoperability-critical Plan 278 rows against the second family:
- i2pr as floodfill receives valid RI and required LeaseSet-family stores;
- required acknowledgement behavior;
- RI hit/miss;
- LeaseSet-family hit/miss with requested current reply protection;
- bounded DSRM/exploration;
- direct zero-token replication accepted by the reference;
- role withdrawal publishes non-f RouterInfo.

The two families need not share implementation-specific peer scoring, but must agree on wire,
record validity, lookup reply, and replication behavior.

## 4. Normal activation after qualification

Only after the second-family matrix passes on the candidate code:

A. Add a normal config field under the canonical NetDB/floodfill configuration surface. Initial
public contract is explicit opt-in boolean or equivalent; default is false. Do not add automatic
network-population heuristics unless separately specified.

B. Config intent requests eligibility evaluation; it does not force Active.

C. caps=f is signed/published only when Plan 277 eligibility is true. Loss of eligibility
withdraws f and drains role work.

D. Config false immediately initiates safe withdrawal/drain.

E. Invalid reload/start config fails before listeners/state mutate, following GUARDRAILS.md.

F. User-facing status reports requested state, effective role state, and categorical ineligibility
reason without peer identifiers.

## 5. M12 private mixed-router acceptance

Run a bounded private topology containing i2pr plus both independent implementation families.
Demonstrate:
- correct store/lookup service;
- replication to multiple eligible floodfills;
- no cross-namespace disclosure;
- bounded storage and DSRM amplification;
- restart recovery;
- role churn/health withdrawal;
- sustained but bounded request/store load sufficient to exercise quotas;
- clean shutdown and resource return to baseline.

This is controlled/private testing, not public-network stress.

## 6. Invariants

- Default-off remains permanent M12 posture.
- No configuration value bypasses health/eligibility.
- No public-network test is required or authorized by this plan.
- No downgrade of ECIES reply protection.
- No reference patching.
- Full two-family claim covers only the implemented M12 subset documented in support/conformance.
- M14 still owns broader sustained interoperability/security closure.

## 7. Failure / cancellation / restart / contention

If second-family qualification fails because of an i2pr defect, stop activation and register a
corrective. If it fails because the reference lane is demonstrably unusable, retain Plan 278
experimental progression but keep normal floodfill config/caps=f unavailable; do not waive ADR
0026.

Config disable/health loss follows Plan 277 drain semantics. Restart starts non-advertised until
fresh health eligibility and persistence revalidation complete.

## 8. Compatibility and migration

New config is additive and default false. No existing configuration changes behavior. Persisted
M12 data remains internal/versioned. RouterInfo capability change occurs only for operators who
explicitly opt in and satisfy eligibility.

## 9. Required verification

Local floor:

~~~bash
cargo fmt --all --check
cargo test --locked --workspace --all-targets
cargo test --locked --doc
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-m12-floodfill-boundaries.sh
bash scripts/check-m12-floodfill-qualification-evidence.sh --self-test
~~~

Run exact-pinned second-family external lane, repeat the Plan 278 i2pd qualification on the final
candidate head, run the private mixed-router acceptance, then require exact-head ordinary CI
green.

## 10. Required tests

- config default false;
- explicit true but ineligible -> no f;
- eligible -> f;
- reload true->false -> withdrawal/drain;
- health loss while configured true -> withdrawal;
- recovery requires fresh eligibility, not stale state;
- invalid reload no mutation;
- two-family wire/evidence matrix;
- private topology bounded load/churn/restart;
- storage/queue resource ceilings and baseline cleanup.

## 11. Documentation

Update README, protocol support, specs/support.toml, specs/CONFORMANCE.md, M12 roadmap, config docs,
and known limitations. State exactly what record types/reply modes/reference versions were
qualified. Do not call the router production-ready.

## 12. Acceptance criteria

M12 closes only when:
1. exact-pinned i2pd Plan 278 remains passed on the final candidate;
2. an independent Java-family router passes the required M12 wire matrix;
3. normal floodfill config is default-off and cannot bypass eligibility;
4. caps=f accurately follows Active/withdrawn state;
5. private mixed-router store/lookup/replication succeeds;
6. records remain signature/freshness validated and bounded;
7. adversarial amplification/store/churn tests remain within ceilings;
8. persistence/restart and shutdown are clean;
9. exact-head ordinary CI is green;
10. no critical/high finding remains.

## 13. Stop conditions

Do not expose normal caps=f if second-family evidence is missing, if the final candidate differs
materially from the externally qualified code, or if any health-withdrawal/resource invariant is
unproven.

## 14. Closure evidence

Record both reference pins, qualification SHAs, exact commands, private topology manifest, evidence
indexes, CI run, config/default review, security/resource/migration review, remaining limitations,
and the registry/roadmap unblock audit for M13.

## 15. Handoff

On pass, M12 is closed and M13 may become the next planning frontier. Public-network observation
and production-readiness remain M14/later work.
