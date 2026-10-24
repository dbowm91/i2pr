# Plan 495 — Peer-eligibility preflight and the first-failure-owner gate

**Status:** closed as `passed-peer-eligibility-gate-and-first-failure-owner`.
**Owner:** no production crate. The deliverable is the gate's *enforcement*
(`scripts/plan495-peer-eligibility.py`) and its canonical model
(`specs/QUALIFICATION-GATES.md`).
**Baseline:** `work/plans-477-480-router-recovery@dbc80596`.
**Predecessors:** the blocked closures of Plans 449, 476, 493 and 434, and the
Plan 230 mixed-router findings
(`plans/closure/mixed-router-interop/230-status.md`).

---

## 1. What was delivered, and what it is not

Plan 495 is a **gate repair**, not a capability plan. It produces no
interoperability evidence, authorizes no external run, changes no
production behaviour, changes no wire format, and claims no capability,
advertisement or support-inventory entry.

Its deliverable is narrower and more useful than an interoperability result:
**a later lane cannot misreport a peer-side condition as an `i2pr` defect.**

Before this plan, an external lane assumed the stock peer would select and
dial `i2pr`, spent an attempt, and read whatever came back as a verdict
about `i2pr`. In an isolated topology the peer has **no alternatives**, so its
own state — its profile store, its selection filters, a ban, a cooldown — can
dominate the outcome completely, and one bad interaction is the whole
topology where on the live network it would be noise. The Plan 230 finding
`P230-D-ELIGIBLE-BUT-NO-PROFILE` ("eligible 3/3 on this SHA; 30 s D poll
exhausted with count=1, C absent") had already produced that false failure
once.

The gate now says, before any protocol attempt, whether the peer *could*
admit us and *could* select us — and says which side owns a failure when it
happens.

### 1.1 The enforcement surface

| Artifact | What it is |
| --- | --- |
| `specs/QUALIFICATION-GATES.md` | The canonical gate model: the asymmetry statement, the per-family source-cited predicates, the lab RouterInfo profile, the tier ladder, the outcome taxonomy, the hygiene protocol, the withholding control, the checker contract. |
| `scripts/plan495-peer-eligibility.py` | The checker. 19 source-derived predicates over 2 families; the lab-profile drift guard; the evidence-shape validator; the hygiene validator; the deliberate-withholding control; and a source-purity audit of itself. |
| `tests/integration/ssu2/plan495-eligibility-fixtures/` | 20 deterministic fixtures: 2 baseline lab profiles, 2 peer configurations, 7 negative lab profiles, the withholding profile, 6 evidence records and 2 hygiene records. |

The checker is **pure**: it opens no socket, starts no process and touches no
network. That is not a promise but a checked property — `audit_purity()`
walks the checker's own AST and rejects any networking or process import and
any process-spawning call, and the self-test carries six inverted probes
that prove the audit fires.

### 1.2 What the checker found in the pinned sources

The predicates were derived by reading the **pinned references' own source**,
not by reading our prose about them. Three findings changed the gate model
and are recorded as F1, F2 and F3 in §9.

The most consequential is **F1**: advertising the floodfill capability is
**necessary but not sufficient** for the pinned i2pd to file us as a
floodfill. `RouterInfo::IsEligibleFloodfill()` also requires
`router.version >= 0.9.62` (`libi2pd/NetDb.hpp:55`,
`NETDB_MIN_FLOODFILL_VERSION`), a published IPv4 address, and a non-DSA
signing key. A RouterInfo below that floor is *never* filed, so an
exploratory request can never target it, and the peer falls back to its
generic dial path (`libi2pd/Transports.cpp:526-543`). The operator procedure
and the SSU2 roadmap prose were corrected accordingly.

---

## 2. Commits

| Commit | Subject | Note |
| --- | --- | --- |
| `9a076bb1` | `feat(qualification-gate): land the Plan 495 peer-eligibility gate and its enforcement` | The deliverable: the checker, the fixtures, the canonical model, the operator-procedure corrections, the plan bindings, and the floor/CI/tooling wiring. |

`dbc80596` (the gate rewrite that preceded this plan) is **not part of the
deliverable**; it is the baseline this plan implements and corrects.

---

## 3. Requirement-to-evidence matrix

| # | Requirement | Evidence | Verdict |
| --- | --- | --- | --- |
| R1 | **The gate model is published, canonical, citable and bound from `specs/CONFORMANCE.md` and from the operator procedure.** | `specs/QUALIFICATION-GATES.md` (9 sections: the asymmetry statement and the two-axis failure owner, the derivation method and the frozen pins, the per-family predicates, the lab profile, the tier ladder and the taxonomy and the evidence shape, the hygiene protocol, the withholding control, what the checker guarantees, what the model does not settle). Bound from `specs/CONFORMANCE.md` ("External qualification gates: the first failure owner", now four binding rules) and from `docs/operations/ssu2-private-lan-qualification.md`. | Held. |
| R2 | **An offline checker evaluates a candidate lab RouterInfo against each pinned family's admission and selection predicates and reports each predicate's state.** | `scripts/plan495-peer-eligibility.py` — 19 predicates (12 i2pd, 7 java-i2p), each carrying its requirement and the pinned source file and line it was derived from; `evaluate()` reports `satisfied` / `unsatisfied` / `deliberately-unsatisfied` / `unknown` per predicate; `--preflight` emits them with a predicate digest. Fixtures: 2 baseline lab profiles and 2 peer configurations. | Held. |
| R3 | **The checker refuses a lane-ready verdict on an unknown predicate.** | `classify()` — an `unknown` predicate yields `not-lane-ready` with the outcome class `environment-unavailable` and the reason `predicate-unknown`; the self-test case `unknown-predicate-did-not-refuse`. | Held. |
| R4 | **The checker opens no socket, starts no process and touches no network, and a static guard with an inverted probe proves it.** | `audit_purity()` walks the checker's own AST; `FORBIDDEN_MODULES` / `FORBIDDEN_CALLS`; the self-test asserts the real source is clean and that six injected probes (`import socket`, `import subprocess`, `import urllib.request`, `os.system`, `eval`, `__import__`) are each detected. The deny-list's own literals are strings, not imports, so the audit cannot trip over itself. | Held. |
| R5 | **The tier ladder and outcome taxonomy are machine-checkable, and the validator rejects evidence that omits a tier, carries two outcome classes, or reports a pass without the tier that earned it.** | `validate_evidence()` — the outcome class must be one of the seven; the cause must be one of the four; tiers must be contiguous with a reason each; a reached tier implies every lower tier was recorded; `pass` requires the target tier to be `pass`; `i2pr-defect` requires the cause `i2pr-lab-profile` and a named failing tier; a peer-side outcome requires the cause `peer-state` or `intentional-withholding`. Eight validator negative cases in the self-test; the fixture `evidence-invalid-tier-gap.json` is rejected. | Held. |
| R6 | **A `peer-state-poisoned` outcome does not consume the attempt budget.** | `validate_evidence()` — a `peer-state-poisoned` attempt must carry `consumes_budget: false` and a cooldown, and `attempts_consumed` is recomputed from the non-peer-state attempts; the self-test case `evidence-validator-missed:peer-state-consumed-budget`; the fixture `evidence-peer-state-poisoned.json` records `attempts_consumed: 0`. | Held. |
| R7 | **The deliberate-withholding control classifies as `peer-selection-refused` with the peer's own reason, and never as `i2pr-defect` and never as a pass.** | `--withholding` mode; the fixture `withholding-no-floodfill.json` (the floodfill route withheld); the self-test case `withholding-not-classified-as-peer-selection-refused` and its three siblings; the mode returns exit 0 only when the verdict is `peer-selection-refused` with the cause `intentional-withholding`. | Held. |
| R8 | **The lab profile is a checked artifact, and drift between the documented profile and the checked profile fails a static guard.** | `LAB_PROFILE_ROWS` in the checker and §4 of `specs/QUALIFICATION-GATES.md`; `profile_drift()` compares them field by field; `--profile-drift`; the self-test case `lab-profile-drift` plus an inverted probe that mutates the documented value and requires the guard to fire; the mode runs in `ci.yml` as well as in the floor. | Held. |
| R9 | **The peer-state hygiene protocol is an executable precondition.** | `validate_hygiene()` — five conditions plus a stale-file count; the fixtures `hygiene-fresh.json` and `hygiene-poisoned.json`; a failed condition yields the outcome class `peer-state-poisoned` naming each failed condition. | Held. |
| R10 | **Every outcome names its failure owner.** | The two-axis model: the outcome class says *where* the run ended; the `cause` says *whose artifact* was responsible (`i2pr-lab-profile`, `i2pr-implementation`, `peer-state`, `intentional-withholding`, `unattributed`). `classify()` sets the cause; `validate_evidence()` enforces the pairing — only `i2pr-implementation` may accompany `i2pr-defect`, and a peer-side outcome may never name the implementation; §1 and §5 of the canonical model state it; `specs/CONFORMANCE.md` rule 4 binds it. | Held. |
| R11 | **Plans 449, 476 and 493 bind to the gate, and Plan 493's attempt-budget rule is reconciled with the canonical causal-corrective policy.** | The three plans' gate-binding paragraphs name the canonical model, the operator procedure and the checker; their verification blocks no longer say the checker "does not exist yet" and now name the four checker modes; Plan 493's attempt-budget reconciliation (a peer-state change *is* a material change) stands as amended. Plan 476's Tier A was re-worded so it **carries** the delivered gate rather than re-delivering it, which would have duplicated closed work; Plan 493's WP1 was corrected to the source-derived mechanism (F1, F7). | Held. |
| R12 | **No support inventory, conformance, advertisement or capability claim changes, and the routine floor is green on the closing tree.** | `specs/support.toml` unchanged; `router.version` unchanged; no `crates/**` diff at all; the floor table in §7.4. | Held. |
| R13 | **The gate is enforced routinely, not only on request.** | The checker's `--self-test` and `--profile-drift` are steps in `AGENTS.md`'s routine floor and in the `quality` job of `.github/workflows/ci.yml`; `docs/architecture/tooling.md` records the script, the floor step count and the top-level script count. | Held. |

---

## 4. The predicate set, as derived

The predicates are not a restatement of our prose: they were read out of the
pinned sources. The full table with every citation is in
`specs/QUALIFICATION-GATES.md` §3; this is the shape.

**i2pd 2.61.0** (`635b013a612ff47278ef02acf8580a28e10e26c5`):

| Kind | Predicates |
| --- | --- |
| admission | `i2pd-ri-signature`, `i2pd-ri-netid-present`, `i2pd-ri-netid-matches`, `i2pd-ri-version-present`, `i2pd-ri-transport-present`, `i2pd-ri-family-signature` |
| dial | `i2pd-not-unreachable`, `i2pd-dial-version-floor` (0.9.58 **or** high bandwidth), `i2pd-reachable-from-peer` |
| explore, route `floodfill-exploration` | `i2pd-floodfill-declared`, `i2pd-floodfill-eligible` (0.9.62 + published IPv4 + non-DSA) |
| explore, route `ssu2-peer-test` | `i2pd-peer-test-cap` (the `B` address cap, which i2pd strips below 0.9.62) |

**Java I2P 2.13.0** (`9134f808337b401e8e53c73734c81fab04280c9d`):

| Kind | Predicates |
| --- | --- |
| admission | `java-ri-signature`, `java-ri-netid`, `java-ri-version-present`, `java-ri-transport-present`, `java-ban-surface` |
| explore, route `java-selection` | `java-profile-creation` (`shouldCreate`), `java-not-excluded` (`shouldExclude`) |

A profile is lane-ready when every admission and dial predicate is
satisfied **and** at least one explore route is available. The routes are
alternatives, not requirements: the peer may reach us by more than one path.

---

## 5. Reviews

### 5.1 Invariant review

| Invariant | Verdict | Evidence |
| --- | --- | --- |
| 1 — the failure owner is always named | Held | `classify()` always returns an outcome class and a cause; `validate_evidence()` rejects a record whose outcome is outside the taxonomy; the two-axis model in §1 of the canonical model. |
| 2 — a peer-side outcome is never an `i2pr` verdict | Held | The five non-verdict classes are enumerated separately (`NON_VERDICT_OUTCOMES`); the checker never emits `i2pr-defect`; `i2pr-defect` is only reachable from a lane's own evidence, and only with the cause `i2pr-implementation` and a named failing tier. |
| 3 — lower tiers survive higher-tier failure | Held | The evidence validator requires each tier to be recorded independently and forbids a gap; a run that reaches T4 and fails at T5 is representable and is not collapsed into a single verdict. |
| 4 — no capability claim exceeds the tier reached | Held | `pass` requires the target tier to be recorded as `pass`; the canonical model's §9 states that a one-family result is progression evidence, not conformance evidence. |
| 5 — no forged reachability, bandwidth or capability | Held | The lab profile is the only place `R` appears, its value is scoped to the lab, and §4 of the canonical model states that `R` is set only when the bound socket is real. No production configuration was changed. |
| 6 — the peer is never patched, vendored or tuned to pass | Held | The pinned sources were read at their exact pins and are not modified (`git status --porcelain` clean on both checkouts); the peer configuration is modelled as data in the peer-config fixture, never as a patch. |
| 7 — fail closed, never silently skip | Held | An unknown predicate refuses a lane-ready verdict; a missing fixture, a malformed fixture or an unknown family is a named error, not a skip; the checker's exit code is 1 on a violation and 2 when it cannot run. |
| 8 — raw reference logs are never evidence | Held | The checker emits states, counts and digests only; no RouterInfo, address, key or identity reaches an output. |

### 5.2 Failure, cancellation, restart and contention semantics

The checker is **stateless and pure**: it holds no lock, starts no task, owns
no socket and has no cancellation surface, because it performs no I/O beyond
reading the fixtures it is given. A run that is interrupted produces no
partial state. The lane-facing semantics it *enforces* are:

- **Exactly one outcome class per run.** The evidence validator rejects a
  record that carries an outcome outside the taxonomy.
- **A `peer-state-poisoned` outcome does not consume the attempt budget.** A
  cooldown expiring or a profile store reset is a material change and
  licenses a controlled rerun inside the same plan.
- **An unknown predicate is not a pass and not a skip.** It refuses the
  lane-ready verdict and names the predicate.
- **A deliberately withheld predicate is a control, not a defect.** It
  produces `peer-selection-refused` with the cause
  `intentional-withholding`.
- **A lab-profile defect and an implementation defect are different
  causes.** `i2pr-lab-profile` means the operator-facing profile the
  preflight evaluates is wrong; `i2pr-implementation` means the daemon's own
  code path is at fault. Only the latter may accompany `i2pr-defect`.

### 5.3 Compatibility and migration review

No wire change, no `router.version` change, no migration, no support
inventory change, no advertisement change, no new configuration key, no new
I2NP message, no RouterInfo option. `specs/support.toml` is byte-identical
and `router.version` stays at the implemented I2NP surface. The gate is
additive to the planning, specification and evidence surface.

**No existing closure record was rewritten.** The Plan 230 and Plan 161
findings those records carry are the evidence this plan builds on, and their
tokens remain authoritative.

### 5.4 Security and privacy review

- **No secret material** is read, derived, printed or written. The checker
  reads fixtures that describe a lab RouterInfo *by property*, never by bytes;
  the fixtures carry no key, address or identity.
- **No raw reference log, RouterInfo, address, key or identity** reaches an
  evidence file. The checker's outputs are states, counts and digests.
- **The checker cannot reach the network.** This is enforced over its own
  AST and proved by inverted probes; a gate that could talk to the peer
  could not be falsified by an offline fixture.
- **The pinned sources were fetched read-only** and are not committed; they
  live under the gitignored `target/interop/` cache, at their exact pins,
  with a clean tree. The reference routers are neither patched nor vendored.
- **No permission, namespace, container, VM or root operation** was used.
- **The `R` capability is not forged.** It is scoped to the lab profile and
  is a true statement about the lab's bound socket; nothing in this plan
  changes a production configuration.

### 5.5 Contention review

Not applicable in the usual sense: the checker opens no socket and starts no
process, so there is no port, no shared resource and no contention to
resolve. The *lane's* contention risk — the peer's own state — is handled as
a first-class precondition (§6 of the canonical model) rather than as a
concurrency problem.

---

## 6. Documentation and operational evidence

| Document | Change |
| --- | --- |
| `specs/QUALIFICATION-GATES.md` | **New.** The canonical gate model. |
| `specs/CONFORMANCE.md` | The binding section now names the canonical model, the operator procedure, the checker and the frozen pins, and carries **four** binding rules — the fourth adds the two-axis failure owner. |
| `docs/operations/ssu2-private-lan-qualification.md` | Binds the canonical model; corrects the floodfill claim (F1) and the version row; adds the Java ban surface; replaces "record which predicates" with the four checker invocations. |
| `plans/implementation/core-router-recovery/449-*.md` | Gate binding names the canonical model and the checker; the verification block no longer says the checker does not exist and names the four modes. |
| `plans/implementation/core-router-recovery/476-*.md` | Gate binding names the canonical model and the checker, and **re-scopes Tier A to carry** the delivered gate rather than to re-deliver it. |
| `plans/implementation/network-participation/493-*.md` | Gate binding names the canonical model and the checker; the verification block is updated the same way; **WP1's mechanism claim is corrected** to the source-derived one (the floodfill cap alone is not sufficient; Java's predicate is `shouldCreate`). |
| `plans/implementation/core-router-recovery/495-*.md` | The status line is authoritative and points at this closure record; the body is the plan as written before implementation and is not rewritten. |
| `docs/architecture/tooling.md` | The new script is documented; the top-level script count and the routine-floor step count are recomputed; the SSU2 lane row names the fixtures and the checker. |
| `AGENTS.md`, `.github/workflows/ci.yml` | The checker's self-test and profile-drift modes are routine floor steps and a CI step. |

---

## 7. Commands run

### 7.1 Focused verification (local)

```text
python3 scripts/plan495-peer-eligibility.py --self-test
# plan495-peer-eligibility: self-test ok (15 predicate negatives, purity probes,
# evidence and hygiene controls, profile-drift control)

python3 scripts/plan495-peer-eligibility.py --profile-drift
# {"schema": "i2pr-plan495-profile-drift-result-v1", "spec":
#  "specs/QUALIFICATION-GATES.md", "valid": true, "violations": []}

python3 scripts/check-tooling-inventory.py
# check-tooling-inventory: every published figure matches the tree

python3 scripts/check-tooling-inventory.py --self-test
# check-tooling-inventory: self-test ok

python3 scripts/check-workflow-validity.py
# CI workflow validity: 11 workflow files parse and are structurally valid

python3 scripts/check-global-plan-number-uniqueness.py
python3 -m unittest discover -s tests/planning -p 'test_*.py'
python3 scripts/check-adr-number-uniqueness.py
python3 scripts/check-license-metadata.py
bash scripts/check-dependency-direction.sh
# all PASS
```

### 7.2 Per-fixture verdicts (local)

| Fixture | Verdict | Outcome class | Cause | Named predicates |
| --- | --- | --- | --- | --- |
| `lab-profile-i2pd.json` | lane-ready | — | — | — |
| `lab-profile-java-i2p.json` | lane-ready | — | — | — |
| `negative-missing-netid.json` | not-lane-ready | `peer-admission-refused` | `i2pr-lab-profile` | `i2pd-ri-netid-present`, `i2pd-ri-netid-matches` |
| `negative-mismatched-netid.json` | not-lane-ready | `peer-admission-refused` | `i2pr-lab-profile` | `i2pd-ri-netid-matches` |
| `negative-version-below-dial-floor.json` | not-lane-ready | `peer-selection-refused` | `i2pr-lab-profile` | `i2pd-dial-version-floor` |
| `negative-version-below-floodfill-floor.json` | not-lane-ready | `peer-selection-refused` | `i2pr-lab-profile` | `i2pd-floodfill-eligible`, `i2pd-peer-test-cap` |
| `negative-introducer-only-address.json` | not-lane-ready | `peer-selection-refused` | `i2pr-lab-profile` | `i2pd-reachable-from-peer` |
| `negative-java-ban-surface.json` | not-lane-ready | `peer-admission-refused` | `i2pr-lab-profile` | `java-ban-surface` |
| `negative-java-missing-reachable-cap.json` | not-lane-ready | `peer-selection-refused` | `i2pr-lab-profile` | `java-profile-creation` |
| `withholding-no-floodfill.json` | not-lane-ready | `peer-selection-refused` | `intentional-withholding` | the three explore predicates |

### 7.3 Evidence and hygiene verdicts (local)

| Fixture | Valid | Note |
| --- | --- | --- |
| `evidence-pass-t4.json` | yes | A T4 pass with a peer-state attempt that consumed no budget. |
| `evidence-peer-selection-refused.json` | yes | `peer-selection-refused` with the cause `intentional-withholding`. |
| `evidence-peer-state-poisoned.json` | yes | `peer-state-poisoned` with a cooldown and `attempts_consumed: 0`. |
| `evidence-invalid-tier-gap.json` | **no** | `evidence-tier-gap:3`, `evidence-tier-gap:4` — a T3/T4 record with no T2. |
| `evidence-i2pr-defect.json` | yes | `i2pr-defect` with the cause `i2pr-implementation` and a named failing tier. |
| `evidence-invalid-i2pr-defect-cause.json` | **no** | `evidence-i2pr-defect-cause-invalid` — the defect names the lab profile instead of the implementation. |
| `hygiene-fresh.json` | yes | — |
| `hygiene-poisoned.json` | **no** | `peer-state-poisoned`, four conditions named. |

### 7.4 The routine floor

`AGENTS.md`'s routine floor was executed in full on the closing tree, with
`CARGO_BUILD_JOBS=4` exported **inside** the runner (this host has 15 GiB of
RAM and 16 cores; the daemon's test binaries do not fit when 16 `rustc`
processes run at once — a host property, not a change to any check).

| # | Step | Outcome | Seconds |
|---|---|---|---|
| 1 | `cargo-fmt` | PASS | 3 |
| 2 | `cargo-check` | PASS | 1 |
| 3 | `cargo-build-siblings` | PASS | 0 |
| 4 | `cargo-build-static-fixture` | PASS | 0 |
| 5 | `cargo-copy-static-fixture` | PASS | 0 |
| 6 | `cargo-rebuild-plain-fixture` | PASS | 0 |
| 7 | `cargo-test-workspace` | PASS | 1119 |
| 8 | `cargo-clippy` | PASS | 1 |
| 9 | `cargo-doc` | PASS | 0 |
| 10 | `cargo-test-doc` | PASS | 4 |
| 11 | `check-dependency-direction` | PASS | 0 |
| 12 | `check-global-plan-number-uniqueness` | PASS | 0 |
| 13 | `check-adr-number-uniqueness` | PASS | 0 |
| 14 | `check-portable-service-tunnel-api` | PASS | 0 |
| 15 | `check-portable-service-tunnel-consumer` | PASS | 7 |
| 16 | `planning-tests` | PASS | 5 |
| 17 | `check-runtime-boundaries` | PASS | 0 |
| 18 | `check-console-boundaries` | PASS | 2 |
| 19 | `check-console-browser-security` | PASS | 0 |
| 20 | `check-tooling-inventory` | PASS | 0 |
| 21 | `check-license-metadata` | PASS | 1 |
| 22 | `check-managed-app-private-client-seams` | PASS | 0 |
| 23 | `check-router-readiness-contract` | PASS | 0 |
| 24 | `check-router-readiness-contract-self` | PASS | 0 |
| 25 | `check-core-router-recovery-contract` | PASS | 0 |
| 26 | `check-core-router-recovery-contract-self` | PASS | 0 |
| 27 | `plan449-ssu2-preflight-self` | PASS | 0 |
| 28 | `plan495-peer-eligibility-self` | PASS | 0 |
| 29 | `plan495-peer-eligibility-profile-drift` | PASS | 0 |
| 30 | `check-managed-app-package-boundary` | PASS | 0 |
| 31 | `check-managed-app-package-boundary-self` | PASS | 0 |
| 32 | `check-managed-app-policy-boundary` | PASS | 0 |
| 33 | `check-managed-app-policy-boundary-self` | PASS | 0 |
| 34 | `check-managed-app-secured-sandbox` | PASS | 0 |
| 35 | `check-managed-app-secured-sandbox-self` | PASS | 0 |
| 36 | `check-service-tunnel-boundaries` | PASS | 0 |
| 37 | `check-managed-app-gateway-boundary` | PASS | 0 |
| 38 | `check-managed-app-manager-boundary` | PASS | 0 |
| 39 | `check-m11-per-epoch-composition` | PASS | 1 |
| 40 | `check-service-anonymity-boundaries` | PASS | 0 |
| 41 | `check-fixture-manifest` | PASS | 0 |
| 42 | `check-ntcp2-vectors` | PASS | 0 |
| 43 | `check-ssu2-vectors` | PASS | 0 |
| 44 | `check-i2cp-vectors` | PASS | 0 |
| 45 | `check-ntcp2-interoperability` | PASS | 0 |
| 46 | `check-constrained-host-lane-boundary` | PASS | 0 |
| 47 | `check-m11-transit-boundaries` | PASS | 2 |
| 48 | `check-m11-transit-qualification-evidence` | PASS | 2 |
| 49 | `check-sam-acceptance-evidence` | PASS | 0 |
| 50 | `check-ssu2-acceptance-evidence` | PASS | 0 |
| 51 | `check-i2cp-acceptance-evidence` | PASS | 0 |
| 52 | `check-i2pcontrol-acceptance-evidence` | PASS | 0 |
| 53 | `check-els2-type11-transcript-boundary` | PASS | 0 |
| 54 | `check-els2-live-lane-evidence` | PASS | 2 |
| 55 | `check-encrypted-service-consumer-caller` | PASS | 0 |
| 56 | `check-outproxy-request-path` | PASS | 0 |
| 57 | `check-outproxy-wire-lane-evidence` | PASS | 0 |
| 58 | `check-config-secret-hygiene` | PASS | 0 |
| 59 | `check-workflow-validity` | PASS | 0 |
| 60 | `check-floodfill-type5-serve` | PASS | 0 |
| 61 | `check-service-tunnel-acceptance-evidence` | PASS | 1 |
| 62 | `check-exploratory-tunnel-evidence` | PASS | 0 |
| 63 | `check-netdb-tunnel-evidence` | PASS | 0 |
| 64 | `check-destination-tunnel-evidence` | PASS | 0 |
| 65 | `check-streaming-tunnel-evidence` | PASS | 5 |
| 66 | `check-m6-mixed-router-acceptance-evidence` | PASS | 4 |
| 67 | `check-m12-floodfill-qualification-evidence-self` | PASS | 0 |
| 68 | `check-m12-floodfill-boundaries-self` | PASS | 0 |
| 69 | `ntcp2-harness-tests` | PASS | 1 |
| 70 | `plan488-mesh-lane` | PASS | 0 |
| 71 | `plan488-evidence-deterministic` | PASS | 0 |
| 72 | `plan489-mesh-lane` | PASS | 0 |
| 73 | `plan489-evidence-deterministic` | PASS | 0 |
| 74 | `plan490-mesh-lane` | PASS | 0 |
| 75 | `plan490-evidence-deterministic` | PASS | 0 |
| 76 | `plan490-ssu2-transport-suite` | PASS | 6 |
| 77 | `plan490-transport-boundary-guard` | PASS | 0 |
| 78 | `plan491-mesh-lane` | PASS | 1 |
| 79 | `plan491-evidence-deterministic` | PASS | 0 |
| 80 | `plan491-boundary-guard` | PASS | 0 |
| 81 | `plan491-proto-suite` | PASS | 0 |
| 82 | `plan491-netdb-suite` | PASS | 86 |
| 83 | `plan491-client-suite` | PASS | 24 |
| 84 | `plan492-posture-suite` | PASS | 1 |
| 85 | `plan492-m11-lane` | PASS | 4 |
| 86 | `cargo-deny` | PASS | 1 |

**No step was skipped, filtered or forgiven.** There is no `|| true`, no
`continue-on-error`, no filename filtering, no fake peer environment, no
broad exclusion and no early-return-success anywhere in the runner.

---

## 8. Known limitations

1. **No external run was performed and none is authorized by this plan.** The
   gate is enforcement, not evidence. Nothing here shows interoperability
   with any reference router.
2. **The predicates are derived offline and encoded; they are not re-derived
   at run time.** The checker applies the encoded set faithfully and reports
   every predicate's state, but it does not read the pinned source at run
   time — a run-time re-derivation would need the checkout and would make
   the checker environment-dependent.
3. **A predicate the source does not determine is `unknown`, not
   `satisfied`.** Where the peers' internals leave a gap, the gate refuses a
   verdict rather than guessing. The plan's stop condition anticipated this
   and it did not trigger: every predicate in §4 is determinable offline.
4. **The controlled SSU2 lane's own fixture is below i2pd's floodfill
   floor** (F6). That lane's direction-B redial therefore rests on i2pd's
   generic dial path, not on floodfill exploration. The lane's fixture was
   **not** changed here: it is test code owned by Plan 161's lane, and
   changing the published `router.version` would change the wire behaviour
   that Plan 161's closure evidence was collected against.
5. **The Java family is modelled, not exercised.** No Java I2P router is
   orchestrated by any lane in this repository, so the Java predicates are
   source-derived but unexercised. The same is true of the Java ban surface
   (F2).
6. **`specs/support.toml` is unchanged and nothing is advertised.** The
   support inventory remains 94 surfaces (88 experimental, 5 deferred, 1
   ready), 0 advertised.

---

## 9. Findings by severity

### Critical

None.

### High

None.

### Medium

**F1 — the floodfill cap is necessary but not sufficient, and the gate prose
said otherwise.** The rewrite committed in `dbc80596` stated that the pinned
peer dials back because we advertise the floodfill capability. The pinned
i2pd 2.61.0 source shows `RouterInfo::IsEligibleFloodfill()` additionally
requires `router.version >= 0.9.62`
(`libi2pd/NetDb.hpp:55`), a published IPv4 address, and a non-DSA signing
key; below that floor the RouterInfo is never filed and an exploratory
request can never target it. A lab profile below 0.9.62 can still be
*dialled* (the generic dial path needs only the dial predicates), but it can
never be *explored*. **Corrected** in `docs/operations/ssu2-private-lan-qualification.md`
§2.2 and §3, in `specs/QUALIFICATION-GATES.md` §3.1, in Plan 493's WP1,
and in the SSU2 roadmap's gate-repair note.

**F2 — Java I2P bans a peer on an exact `(caps, version)` pair, and the
profile-creation predicate needs `R`.** The pinned Java source's NTCP2
inbound-establish path bans a RouterInfo whose caps are exactly `LU` at
version `0.9.56`, that carry `L` at version `0.9.57`, or that carry `G`,
`X` and `f` at version `0.9.67`
(`router/java/src/net/i2p/router/transport/ntcp/InboundEstablishState.java:797-822`).
The cooldown is 30 to 120 minutes — long enough to make every later attempt
in the same lane fail, and invisible in the peer's own logs. **Mitigated
offline**: the new `java-ban-surface` predicate refuses such a profile
before an attempt is spent, and the Java `R` requirement is recorded
explicitly rather than implied.

**F3 — the lab is more permissive than the live network, and the gate said
so only in prose.** i2pd files a newly learned floodfill immediately only
while it holds fewer than 2000 floodfills
(`NETDB_NUM_FLOODFILLS_THRESHOLD`, `libi2pd/NetDb.hpp:45`), or when the
peer's profile for that router is already *real*
(`libi2pd/NetDb.cpp:333-344`). An isolated lab peer holds zero floodfills,
so our router is filed immediately; on the live network the same router is
filed only after it has actually been talked to. **Corrected**: the
asymmetry is now stated as a source-visible fact in
`specs/QUALIFICATION-GATES.md` §1, with the citation.

### Low

**F4 — the checker is a non-`check-*` tracked script, so
`check-tooling-inventory.py`'s rule 3 does not require an inventory row for
it.** It is documented in a new "Tracked non-`check-*` scripts" table in
`docs/architecture/tooling.md`, matching the `plan449-ssu2-preflight.py`
precedent. The rule was **not** weakened.

**F5 — `docs/architecture/tooling.md`'s glance row "Integration lane
directories | 10" is stale against the tree.** Thirteen directories exist
under `tests/integration/`; `core-router-recovery/` and
`network-participation/` have no row in the lane table. The row is enforced
by no rule and was not invalidated by this plan (the new fixtures live
*inside* an existing lane directory). Left for a tooling-inventory pass
rather than silently republishing a number this plan did not derive.

**F6 — the controlled SSU2 lane's own RouterInfo publishes
`router.version = "0.9.58"`, below i2pd's floodfill floor.** See §8.4.
Recorded with a named owner: the controlled lane is Plan 161's fixture, and
correcting it needs a plan-of-record because it changes published wire
bytes.

**F7 — the operator procedure named the wrong Java symbol.** It cited
`heardAbout()` as the profile-creation predicate; the pinned source names it
`ProfileManagerImpl.shouldCreate(caps)`, and `heardAbout` is the caller.
Corrected in the operator procedure and in Plan 493's WP1, and recorded
correctly in `specs/QUALIFICATION-GATES.md` §3.2.

---

## 10. Roadmap disposition

**Closed.** Plan 495 is closed as `passed-peer-eligibility-gate-and-first-failure-owner`.

Subsystem rows advanced:

- `plans/subsystems/core-router-recovery-roadmap.md` — the Plan 495
  milestone row moves from `ready` to the closed form, with the closure
  record and the implementation commit.
- `plans/subsystems/ssu2-roadmap.md` — the gate-repair note moves from
  `ready` to closed and names the canonical model and the checker.
- `plans/subsystems/network-participation-roadmap.md` — the Plan 495
  reference in the Plan 493 row and in the qualification-gate note is
  updated to the closed state; the Plan 493 row itself stays **blocked**.

`plans/registry.md` — the Plan 495 row moves from the dependency-ready
section to *recently closed*, and the network-participation line is updated:
Plan 493's gate dependency is now discharged, and its remaining blockers are
the authorized two-host topology and Plan 435.

---

## 11. Unblock audit

Audited: every registered plan that lists Plan 495 as a hard or interface
dependency.

| Plan | Relation to 495 | Other hard dependencies | Disposition |
| --- | --- | --- | --- |
| 449 — portable SSU2 independent-LAN lane | The live column is governed by the gate | The authorized two-host host pair (environment) | **Stays blocked.** The gate dependency is discharged; the environment dependency is not, and no environment exists here. The infrastructure scope of Plan 449 was already closed as `passed-portable-preflight-infrastructure-only`. |
| 476 — authorized SSU2 LAN live executor | The executor must carry the gate | A local process executor (buildable) and the authorized host pair | **Stays blocked.** The gate dependency is discharged. The Tier A process executor remains an engineering gap that the missing host pair does not excuse; it needs its own corrective pass. |
| 493 — two-family public participation matrix | The matrix runs under the gate | The authorized two-host topology (Plan 431 stopped) and Plan 435's transport activation | **Stays blocked.** The gate dependency is discharged; the topology and transport dependencies are not. |
| 434 — current-pinned i2pd reverse-initiator runner | Named as a corrective sibling | The pinned i2pd runner | **Stays blocked.** Unaffected by this plan. |

**No plan moves to `ready` in this commit.** Every remaining blocker is an
external environment or an unbuilt executor, not the gate. Saying otherwise
would be the same error this plan exists to prevent: reading a condition as
resolved when it is not.

Two items are registered for a successor rather than silently dropped:

1. The **controlled lane's floodfill-floor fixture** (F6) needs a
   plan-of-record, because it changes published wire bytes owned by Plan 161.
2. **Plan 476's Tier A process executor** remains buildable now; the missing
   host pair blocks Tier B but does not excuse it.

---

## 12. Handoff notes

- **Do not weaken the classification to make a lane green.** A lane that
  cannot say which side owns a failure must record `timeout` or
  `environment-unavailable`, not `pass` and not `i2pr-defect`.
- **The canonical policy already required this.** `plans/README.md`'s
  bounded-execution rules 1, 2, 3 and 6, and `plans/implementation/README.md`'s
  external-router handoff rules, already mandate source-derived selection
  prerequisites, a first-failure-owner boundary, and a randomized peer
  selector whose admission and selection are actually verified. This plan
  implements them; it does not relax them.
- **The isolated-topology asymmetry is the reason this plan exists.** On the
  live network a peer has alternatives and one bad interaction is noise; in
  the lab the peer has none, and its own state dominates. That asymmetry is
  why a lab failure is not a live-network verdict — in either direction.
- **The next lane that runs against a stock peer must use the checker.** The
  fixtures under `tests/integration/ssu2/plan495-eligibility-fixtures/` are
  the starting point; a lane that writes its own ad-hoc eligibility rule
  reintroduces exactly the defect this plan closed.
