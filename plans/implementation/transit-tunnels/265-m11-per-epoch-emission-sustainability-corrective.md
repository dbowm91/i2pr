# Plan 265 — M11 fixed-budget opportunity-qualified emission sustainability corrective

Status at registration:
**registered-m11-per-epoch-emission-sustainability-corrective-ready**

Original registration baseline:
`6ab9dc2dd80526ce38e62014635ec3e3ad5521f5`

Research refinement baseline:
`1234a5ed6f839762467c1058ac67285837d0c988`

This refinement is planning/specification-only. Plans 263/264 and this planning refinement do
not change the production `crates/*/src` baseline inherited from the Plan 262 implementation
at `514bf1237e86fde21e17fc98c743eb52852edd99`. The implementation agent MUST start from
latest `main`, record the exact pre-implementation HEAD, and bind all Plan 265 counted
attempts to one eventual qualification SHA. The historical SHAs above are provenance, not the
new qualification SHA.

Corrects:

- `plans/implementation/transit-tunnels/264-m11-single-mesh-sustainability-scoping.md`
  (§10 criterion 5): the per-epoch lane closes the deterministic epochs but reference-driven
  emission opportunities remain intermittent;
- `plans/closure/transit-tunnels/264-status.md`: `ibgw-data` 1/2,
  `receipt` 0/2, `participant-data` 1/2, `replay` 0/2 setup stops, with lifecycle rows
  blocked behind the same genuine Participant forward prerequisite;
- the original Plan 265 registration text, which intentionally left the emission counting
  shape open. This refinement freezes that shape before any Plan 265 external execution.

Retains in full:

- Plan 262 production semantics: dedicated `TransitGatewayData`, exact IBGW receive-id
  ownership, source-neutral `route_ibgw_gateway`, self-targeted OBEP TUNNEL loopback,
  `LocalIbgwDelivered` / `LocalIbgwDropped`, no synthetic peer/index mutation, and
  unchanged Participant/OBEP previous-peer locks;
- Plan 263 harness prerequisites: mesh-liveness, relay-NetDB, B-floodfill, canonical SAM
  timeout tail, and fail-closed environment gates;
- Plan 264 per-epoch execution plumbing and the five already-proven deterministic epochs:
  `obep`, `ibgw`, `participant`, `reject`, `obep-data`, each 2/2 on
  `6ab9dc2d`;
- exact-pinned, unmodified i2pd 2.61.0 at
  `635b013a612ff47278ef02acf8580a28e10e26c5`;
- loopback-only controlled qualification, fresh datadirs/ports/evidence roots, no public
  reseed/network fallback, no reference patching.

## 1. Objective

Close the remaining M11 experimental qualification without changing working production routing
code and without converting external-router nondeterminism into "retry until green".

Plan 264 established two different facts:

1. deterministic/build/local-data epochs are stable when isolated per fresh mesh;
2. the remaining failures occur when the unmanaged reference must emit a particular live input
   during a narrow window.

Plan 265 therefore separates **opportunity generation** from **i2pr semantic correctness**.
Every fresh-mesh attempt is retained. The external reference either supplies the predeclared
input opportunity or it does not. Once that opportunity reaches the defined i2pr boundary, any
semantic contradiction is a hard failure; it may not be relabeled as flakiness.

The final M11 closure shape is three fixed-budget scenario families:

- `ibgw-data`
- `receipt`
- `participant-lifecycle`

Each family executes **exactly eight fresh-mesh attempts** on the same Plan 265 qualification
SHA for a successful closure. There is no early stop on success and no extra attempt after the
budget is exhausted. Early termination is permitted only for a semantic contradiction,
security/resource invariant failure, or unclassifiable evidence; those outcomes retain/block
the plan.

The five Plan 264 deterministic epochs remain retained evidence because the production source
tree is unchanged. Plan 265 MUST prove that equivalence mechanically.

This is an evidence/harness corrective only. Production `crates/*/src` diff MUST remain empty.

## 2. Research basis and why this shape is defensible

The qualification design follows four established testing principles:

- flaky/nondeterministic tests should remain visible rather than be erased by reruns; repeated
  pass/fail behavior on unchanged code is itself evidence that must be tracked;
- claims that a flaky test is "fixed" require empirical execution, not a developer label;
- distributed-system testing is strongest when workload/opportunity generation is separated
  from invariant checking;
- the sample budget, stopping rule, classification rule, and primary success predicate must be
  frozen before observing the new execution data.

Reference material:

- Google Testing Blog, "Flaky Tests at Google and How We Mitigate Them":
  `https://testing.googleblog.com/2016/05/flaky-tests-at-google-and-how-we.html`
- Microsoft Research, "A Study on the Lifecycle of Flaky Tests":
  `https://www.microsoft.com/en-us/research/publication/a-study-on-the-lifecycle-of-flaky-tests/`
- Antithesis deterministic simulation testing:
  `https://antithesis.com/docs/resources/deterministic_simulation_testing/`
- Pre-SPEC pre-specified analysis framework:
  `https://pmc.ncbi.nlm.nih.gov/articles/PMC7487509/`

The eight-attempt budget is an engineering bound, not a statistical reliability claim. Plan 264
observed roughly window-like behavior around one opportunity in two for several emission paths;
under an illustrative independent 50% opportunity rate, eight attempts yield a 96.5% chance of
seeing at least two opportunities. Real attempts are not assumed independent and Plan 265 MUST
NOT report that number as a confidence level. It only motivates a finite budget large enough to
avoid another two-run false boundary while preventing open-ended sampling.

## 3. Frozen invariants

### 3.1 Product

- No production `crates/*/src` changes.
- Ordinary i2pr transit remains disabled.
- No RouterInfo capability or `router.version` change.
- No public transit config option.
- No wire-format change.
- No task/channel/queue addition.
- No quota, timeout, message-size, retry-round, or admission-ceiling inflation.
- No M12 implementation.

### 3.2 Reference/network

- i2pd 2.61.0 @ `635b013a612ff47278ef02acf8580a28e10e26c5`.
- Reference source clean/unmodified.
- Loopback-only mesh.
- Public reseed/network disabled.
- Fresh datadirs, ports, and evidence root for every attempt.
- Existing within-attempt bounded setup/send rounds remain frozen from Plan 264.

### 3.3 Evidence

- Every dispatched attempt is retained in the denominator.
- No attempt may be deleted, renamed diagnostic after seeing its result, or replaced.
- No "first two successes count" rule.
- No success-based early stopping. A successful closure executes attempts 1 through 8 for all
  three families.
- A hard semantic failure MAY stop immediately because it already falsifies closure; the closure
  record must retain the failing attempt and no success claim may be made.
- Raw reference logs are diagnostic only. Counted evidence remains sanitized routing facts,
  counts, hashes, statuses, and payload digests.
- Opportunity classification MUST be decided from an input-side/pre-semantic predicate, never
  from the downstream success result.

## 4. Manifest v5 and attempt classification

Extend the Plan 264 manifest to schema v5. Every Plan 265 attempt MUST carry at minimum:

    plan: 265
    scenario: ibgw-data | receipt | participant-lifecycle
    attempt: 1..8
    attempt_budget: 8
    qualification_sha: <exact same SHA for all 24 attempts>
    production_baseline: 514bf1237e86fde21e17fc98c743eb52852edd99
    opportunity: present | absent
    opportunity_reason: <finite typed token>
    semantic: pass | fail | not-applicable
    external_completion: pass | miss | not-applicable
    terminal_class: <finite typed token>

The runner/composition checker MUST reject:

- missing or duplicate ordinals;
- ordinals outside 1..8;
- fewer/more than eight attempts for a successful family;
- mixed qualification SHAs;
- any `semantic: fail`;
- any unrecognized/unclassified terminal;
- opportunity absent with semantic pass/fail;
- opportunity present with semantic `not-applicable`;
- changing `attempt_budget` after the first manifest;
- manifests from Plan 264/263 being silently promoted into Plan 265 family counts.

All eight attempts remain visible in the closure summary as
`attempt -> opportunity -> semantic -> external_completion -> terminal`.

## 5. Scenario family A — IBGW large-input / multicell

Purpose: distinguish "the reference never delivered a large input to this live IBGW" from
"i2pr received a large input and fragmented it incorrectly".

### 5.1 Opportunity predicate

Add test/harness-only input-side observation immediately before the live IBGW gateway processor
is evaluated. Opportunity is PRESENT only when all are true:

1. TunnelGateway addresses a live accepted `IbgwData` receive id;
2. the registration is unexpired and role-correct;
3. the nested standard I2NP byte length is greater than the canonical one-cell payload capacity
   used by the existing Plan 258 fragmentation path;
4. the input occurs inside the named Plan 265 attempt/send window.

This observation MUST be captured before evaluating emitted cells. Do not infer opportunity from
`GatewayDelivered`, cell count, or receiver receipt.

Opportunity-absent terminal:

    ibgw-large-input-not-observed

### 5.2 Semantic predicate

For every opportunity-present input:

- exact receive id accepted;
- `GatewayDelivered`;
- at least two emitted TunnelData cells;
- all cell sizes/counts remain within existing bounds;
- next-router/next-tunnel tuple equals committed registration state;
- zero gateway/forward failures.

A large input followed by zero/one cells is a **semantic failure**, not an environmental miss.

Family closure:

- exactly 8 attempts retained;
- at least 2 opportunity-present semantic passes;
- zero semantic failures;
- all opportunity-absent attempts carry only the exact typed no-opportunity terminal.

The old B-ending receiver socket is not a closing predicate for this family; creator-owned
receipt remains Scenario B.

## 6. Scenario family B — B-sender creator-owned receipt

Purpose: retain the genuine end-to-end mixed-router receipt requirement while separating an
absent B-originated self-targeted action from failure of the corrected local IBGW seam.

### 6.1 Opportunity predicate

Opportunity is PRESENT only when the typed OBEP stage has already produced, before local-loop
processing:

- authenticated origin peer B;
- delivery-type TUNNEL action;
- `target_router == local_router_hash`;
- action tunnel id equals a live counted creator-A IBGW receive id;
- B-side LeaseSet resolution proof remains true.

This is the boundary between upstream/reference opportunity generation and the Plan 262
self-loop/IBGW behavior.

Opportunity-absent terminals are finite and mutually exclusive:

    receipt-no-b-originated-self-action
    receipt-no-live-counted-ibgw-target

A LeaseSet-resolution contradiction remains a setup/error terminal, not a harmless opportunity
absence.

### 6.2 i2pr semantic predicate

For every opportunity-present attempt:

- zero `terminal-garlic-self`;
- local source-neutral IBGW ingress on the exact counted receive id;
- multicell emission for the 1400-byte stimulus;
- zero gateway/forward failures;
- next router is creator A;
- next tunnel is the creator-local inbound id;
- six-field receipt tuple validates;
- no synthetic `PeerId` or peer-index mutation.

Any failure above is a hard semantic failure.

### 6.3 External completion predicate

`external_completion: pass` requires the creator receiver SAM socket to deliver the exact
1400-byte 0xA5 payload/digest exactly once.

If the i2pr semantic predicate passes but the reference creator never produces socket delivery,
record:

    receipt-reference-completion-miss

That is not relabeled as an i2pr semantic failure, but it also does not satisfy the external
interop requirement.

Family closure:

- exactly 8 attempts retained;
- at least 2 attempts with opportunity present + semantic pass + external completion pass;
- zero semantic failures;
- all other attempts fully classified.

## 7. Scenario family C — Participant forward + lifecycle chain

Purpose: stop paying the same rare genuine Participant-forward prerequisite separately for
`participant-data`, `replay`, `expiry`, `session-close`, `cancel`, and `restart`.
Plan 264 already executes these rows as one chain; Plan 265 counts that chain honestly as one
scenario without borrowing evidence across separate meshes.

### 7.1 Opportunity predicate

Add a test/harness-only input-side marker before Participant processing. Opportunity is PRESENT
only when:

- authenticated previous peer is the locked creator A peer;
- TunnelData addresses the live accepted Participant receive id;
- registration is live/unexpired;
- the raw genuine cell is retained for the lifecycle experiment.

Opportunity-absent terminal:

    participant-input-not-observed

Do not infer opportunity from a successful forward.

### 7.2 Forward/external predicate

For every opportunity-present attempt:

- i2pr emits exactly the role-correct next-hop forward;
- next router is B and next tunnel is the committed Participant next tunnel;
- B endpoint observes that exact next tunnel;
- digest/identity binding is preserved.

A present Participant input with no correct local forward is a semantic failure. A correct local
forward with no B endpoint observation is:

    participant-reference-completion-miss

and cannot count as a family success.

### 7.3 Lifecycle chain on the same genuine cell

After the forward + B far-side proof succeeds, execute the existing chain on that same retained
genuine cell:

1. **replay** — replay produces no second local semantic forward; explicitly classify
   `duplicate-dropped`, `contained-no-output`, or `duplicate-forwarded`. Any
   `duplicate-forwarded` is a semantic failure even if B does not receive it. B endpoint delta
   must remain zero for the replayed copy.
2. **expiry** — logical time beyond creation + lifetime drops the cell before transform and
   expiry/sweep state returns to the expected baseline.
3. **session-close** — A mapping removed, unrelated B mapping retained.
4. **cancel** — every bounded owned dimension drains synchronously and new ingress is refused.
5. **restart** — real owner/runtime restart contains zero old secret/live state, sessions are
   re-established under the existing lane contract, and a fresh accepted build is possible.

Every row gets its own terminal evidence key in the same attempt manifest. This is not
cross-attempt evidence merging: one fresh mesh either completes the whole chain or it does not.

Family closure:

- exactly 8 attempts retained;
- at least 2 attempts with opportunity present + correct forward + B far-side proof + all five
  lifecycle rows passing;
- zero semantic/lifecycle failures;
- all other attempts fully classified.

This Plan 265 family composition supersedes only Plan 264's "one manifest-named lifecycle epoch
counts per mesh" rule for the six still-open rows. It does not rewrite Plan 264 history.

## 8. Retained deterministic epochs and production equivalence

Do not spend another external stochastic budget re-proving the five Plan 264 deterministic
epochs. Retain their 2/2 manifests:

- obep
- ibgw
- participant
- reject
- obep-data

Plan 265 MUST instead prove:

    git diff --name-only 514bf1237e86fde21e17fc98c743eb52852edd99..HEAD -- 'crates/*/src'

is empty at the qualification SHA, and that Plan 265 modifications are limited to test/harness,
checker/workflow, planning, and specification files.

The closure record must explicitly distinguish:

- production implementation authority: Plan 262 source tree;
- deterministic external evidence: Plan 264;
- fixed-budget opportunity-qualified emission/lifecycle evidence: Plan 265.

No broad claim that all milestone rows were executed on one repository SHA is permitted.

## 9. Composition gate

Extend `scripts/check-m11-per-epoch-composition.sh` into the Plan 265 closure composer.

A successful compose requires:

1. retained Plan 264 five-epoch 2/2 evidence is referenced and integrity-checked;
2. exactly 8 Plan 265 manifests for each of the three scenario families;
3. all 24 Plan 265 manifests use one qualification SHA;
4. attempt ordinals are exactly 1..8 for each family;
5. no semantic failure or unclassified terminal exists;
6. `ibgw-data`: >=2 opportunity-present semantic passes;
7. `receipt`: >=2 opportunity-present semantic + external-completion passes;
8. `participant-lifecycle`: >=2 opportunity-present forward/far-side/full-chain passes;
9. every opportunity-absent run uses only a declared input-side no-opportunity terminal;
10. no Plan 265 success can be synthesized by combining partial rows from different attempts.

Add negative fixture/tests for:

- 2 successes + 6 retained misses -> accepted only when all misses are valid no-opportunity /
  reference-completion terminals and all semantic opportunities pass;
- 7 successes + 1 semantic failure -> rejected;
- 2 successes then missing attempts 3..8 -> rejected;
- mixed SHAs -> rejected;
- changed budget -> rejected;
- duplicate ordinal -> rejected;
- opportunity inferred from output -> static checker rejection;
- partial lifecycle rows borrowed across attempts -> rejected;
- replay local second-forward with zero B receipt -> rejected.

## 10. Workflow and execution order

Workflow dispatch MUST accept:

    scenario
    attempt

and map exactly one fresh mesh to one manifest.

Execution order is fixed before data collection:

1. land Plan 265 harness/checker/schema changes;
2. run all local tests/static gates;
3. freeze qualification SHA;
4. execute `ibgw-data` attempts 1..8;
5. execute `receipt` attempts 1..8;
6. execute `participant-lifecycle` attempts 1..8;
7. do not add attempt 9;
8. run the composer once against the complete retained set.

Parallel hosted dispatch is permitted only if each attempt has disjoint datadirs/ports/evidence
roots and ordinal assignment is fixed before dispatch. Completion order never changes ordinal.

## 11. Security / resource / compatibility

No user migration.

No change to:

- config schema;
- CLI;
- SAM/I2CP public API;
- RouterInfo capabilities;
- router.version;
- public-network behavior;
- default transit-disabled construction.

Security interpretation remains:

- network transport authentication at the transport owner;
- IBGW authorization by live receive id + role + expiry/resource state, not build creator;
- Participant/OBEP hop provenance peer-locked;
- local self-loop only after decoded target equals own router hash;
- replay fails on a second local semantic forward even if the reference drops it;
- no synthetic peer identity.

No new dependency.

## 12. Required focused tests

At minimum:

1. retained Plan 262 IBGW ownership/self-loop regressions;
2. retained Plan 263 liveness/prerequisite regressions;
3. retained Plan 264 per-epoch/manifest regressions;
4. Plan 265 input-side IBGW opportunity classification;
5. large-input present + multicell pass;
6. large-input present + one-cell output hard-fail;
7. IBGW opportunity absent typed classification;
8. receipt self-action opportunity classification;
9. receipt semantic pass + socket pass;
10. receipt semantic pass + reference-completion miss classification;
11. receipt opportunity present + local ingress failure hard-fail;
12. Participant input-side opportunity classification;
13. Participant input present + no local forward hard-fail;
14. local forward + B completion miss classification;
15. full Participant/lifecycle chain pass;
16. replay `duplicate-dropped` pass;
17. replay `contained-no-output` pass only with zero second local forward and zero B delta;
18. replay `duplicate-forwarded` hard-fail regardless of B receipt;
19. manifest v5 exactly-8 ordinal coverage;
20. budget/mixed-SHA/duplicate/missing/unclassified negative composition rows;
21. lifecycle cross-attempt borrowing rejected;
22. production source diff guard;
23. missing environment fails before network startup;
24. ordinary product transit remains disabled.

## 13. Exact verification floor

Run:

    cargo fmt --all --check
    cargo check --locked --workspace --all-targets
    cargo test --locked -p i2pr-tunnel --all-targets -- --test-threads=1
    cargo test --locked -p i2pr-daemon --all-targets -- --test-threads=1
    cargo test --locked --workspace --all-targets -- --test-threads=1
    cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
    RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
    cargo test --locked --workspace --doc
    cargo deny check advisories bans sources
    bash scripts/check-dependency-direction.sh
    bash scripts/check-runtime-boundaries.sh
    bash scripts/check-service-tunnel-boundaries.sh
    bash scripts/check-m11-transit-boundaries.sh
    bash scripts/check-m11-transit-qualification-evidence.sh
    bash scripts/check-m11-per-epoch-composition.sh
    bash scripts/check-m6-mixed-router-acceptance-evidence.sh
    bash scripts/check-exploratory-tunnel-evidence.sh
    git diff --check

Run every new Plan 265 focused test exactly by name.

After local verification, freeze the qualification SHA and execute the fixed 24-attempt external
budget. Successful closure then requires exact-head ordinary CI green on:

- Quality (ubuntu-latest)
- Quality (macos-latest)
- MSRV (Ubuntu)
- Dependency policy

## 14. Stop conditions

Stop and register a new narrow corrective only if:

- a Plan 265 opportunity-present attempt produces any i2pr semantic contradiction;
- replay produces a second local semantic forward;
- a security/resource invariant regresses;
- the input-side opportunity boundary cannot be instrumented without production code changes;
- any family ends attempt 8 with fewer than two required successes;
- an attempt cannot be classified by the frozen terminal vocabulary;
- reference patching, public fallback, timeout/quota/message-size enlargement, or a new
  dependency would be required.

If fewer than two successes occur within eight attempts but all i2pr semantic opportunities pass,
the successor owns **qualification opportunity generation only**. Do not reopen Plan 262
production routing without a semantic contradiction.

Do not create Plan 266 pre-emptively.

## 15. Acceptance criteria

Plan 265 closes only when all are directly evidenced:

1. Plan 262 production source remains unchanged from `514bf123...`.
2. Plan 264 five deterministic epochs remain integrity-checked and traceable.
3. Manifest v5 + fixed budget + terminal vocabulary are committed before external execution.
4. Exactly 24 Plan 265 attempts exist: 8 per family, fresh mesh each, one qualification SHA.
5. Every attempt is retained and classified.
6. No i2pr semantic failure occurs in any opportunity-present attempt.
7. IBGW-data has >=2 large-input opportunity semantic passes.
8. Receipt has >=2 full tuple-bound exact-once SAM completions.
9. Participant-lifecycle has >=2 forward + B far-side + full lifecycle-chain passes.
10. Replay has no second local semantic forward and no B-side duplicate receipt.
11. No attempt beyond ordinal 8 exists for any family.
12. Complete local verification passes.
13. Exact-head ordinary CI passes all four jobs.
14. Registry/roadmap/support/conformance/dossier/README agree on the final M11 disposition.
15. No product default/capability/version/public-network change.
16. No critical/high finding remains open.

Only then may the closure/unblock audit mark ADR 0026 one-family M11 experimental qualification
passed and make M12 planning dependency-ready. Public transit participation remains a separate
future decision.

## 16. Closure evidence required

Write `plans/closure/transit-tunnels/265-status.md` containing:

- implementation and closure SHAs;
- production-source equivalence proof to Plan 262;
- exact reference pin/version;
- research-refined fixed-budget contract;
- all 24 manifest paths in ordinal order;
- per-family 8-row table:
  `attempt | opportunity | semantic | external_completion | terminal`;
- family success counts and no-opportunity/reference-completion counts;
- replay outcome-kind evidence;
- retained Plan 264 deterministic evidence references;
- complete verification results;
- exact-head CI run id + four jobs;
- security/resource/concurrency/migration review;
- findings by severity;
- unblock audit.

Do not report an empirical success fraction as a reliability estimate or anonymity/privacy
claim.

## 17. Handoff order

1. implement manifest v5, finite terminal classes, and input-side opportunity markers;
2. implement the fixed-budget composition gate and all negative tests;
3. implement replay outcome-kind hardening;
4. prove zero production source diff;
5. run the complete local floor;
6. freeze one qualification SHA;
7. execute all 24 external attempts in the fixed order/budget;
8. compose once;
9. obtain exact-head ordinary CI;
10. close M11 only if §15 passes exactly.

No production routing changes and no post-result adjustment of the counting rule.
