# Plan 278 — M12 exact-pinned i2pd controlled floodfill qualification

Status at registration:
**registered-m12-i2pd-qualification-blocked-on-plan277**

Classification: external capability qualification.

Hard dependency: Plan 277 passed.

## 1. Objective

Demonstrate the complete M12 controlled floodfill path against unmodified exact-pinned i2pd 2.61.0
at 635b013a612ff47278ef02acf8580a28e10e26c5, using real SSU2 router sessions and the Plan 277
controlled caps=f permit. This is the one-family experimental progression gate, not broad
advertisement authority.

## 2. Evidence principles

- Reference source/binary pin is exact and verified before execution.
- Fresh datadirs/ports per attempt; no public network/reseed fallback.
- No reference source patching, vendoring, fake peer injection, or production protocol relaxation.
- External rows are ignored in ordinary CI and fail closed when explicitly selected without the
  required environment.
- Raw reference logs are diagnostic; retained evidence is sanitized counts, hashes, categorical
  outcomes, and exact command/pin metadata.
- No retry-until-green. Freeze a bounded attempt budget before execution.

## 3. Required qualification matrix

At minimum on one qualification SHA:

A. RouterInfo store:
i2pd publisher/requester -> i2pr floodfill, valid new store with ack; i2pr stores and emits direct
zero-token replication to controlled i2pd floodfill peers.

B. LeaseSet-family store:
exercise every record family ADR 0027 requires in external qualification where i2pd can produce or
consume it. For types not practically generatable by stock client control, retain independent
normative/reference vectors plus a documented external limitation; do not fake a record.

C. RI lookup hit and miss:
i2pd -> i2pr; hit returns DSM, miss returns bounded DSRM.

D. LeaseSet-family lookup hit and miss with current ECIES supplied-key protected reply. i2pd must
successfully consume/decrypt the response.

E. Exploration lookup:
response contains only non-floodfill peers under the ADR behavior.

F. Replication:
i2pr sends direct SSU2 zero-token store to i2pd; receiver accepts; i2pr does not tunnel flood.

G. Daily rollover:
controlled clock/fixture evidence plus external sanity around current key; do not manipulate the
reference clock destructively merely to force midnight.

H. Restart:
persisted allowed records revalidate; role starts without f until eligibility; then controlled
re-activation.

I. Health withdrawal:
force an i2pr-local eligibility loss; observe new RouterInfo without f and no new floodfill
admission.

## 4. Adversarial/resource qualification

Malformed/oversized/pressure tests run against i2pr in isolated local fixtures, not as abuse
traffic against the reference. Required rows include lookup storms, repeated store key, quota
exhaustion, compressed RI bomb, DSRM amplification ceiling, queue saturation, cancellation and
shutdown cleanup.

## 5. Harness scope

Prefer one Rust integration driver plus narrow shell orchestration. Reuse the established exact-pin
SSU2 environment. Do not create a new Python orchestration framework.

Add:
- tests/integration/floodfill/ manifest/evidence schema;
- ignored exact external test(s);
- scripts/check-m12-floodfill-qualification-evidence.sh;
- hosted/manual workflow only if current runner environment can provide the exact reference.

## 6. Failure / cancellation / restart

Every attempt has hard startup, session, request, reply, and shutdown deadlines. Timeout is a
retained categorical failure. Cleanup kills owned reference processes and removes only the fresh
attempt datadir.

## 7. Compatibility and migration

No normal floodfill config activation. Plan 277 controlled permit remains the only activation path.
A pass upgrades experimental progression only.

## 8. Required local verification before external execution

~~~bash
cargo fmt --all --check
cargo test --locked --workspace --all-targets
cargo test --locked --doc
cargo clippy --locked --workspace --all-targets -- -D warnings
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-m12-floodfill-boundaries.sh
bash scripts/check-m12-floodfill-qualification-evidence.sh --self-test
~~~

Then execute the exact ignored external test/runner commands recorded by the implementation and
run the evidence checker on the retained root.

## 9. CI

Exact-head ordinary CI must be green. Manual external workflow status is separate from ordinary CI
and must not be represented as green if zero jobs ran.

## 10. Documentation

Update conformance/support docs to state one-family controlled i2pd M12 qualification only. Public
or normal-daemon floodfill remains unavailable pending Plan 279.

## 11. Acceptance criteria

- Store/lookup/ECIES-reply/replication controlled matrix passes against exact-pinned stock i2pd.
- Zero i2pr semantic failures remain in the accepted evidence set.
- Local adversarial/resource matrix remains within explicit limits.
- Restart and health withdrawal are demonstrated.
- Exact-head ordinary CI green.
- Evidence checker passes and retained artifacts are integrity-indexed.
- No public config/capability claim is enabled.
- No critical/high finding remains.

## 12. Stop conditions

Stop on any reproducible i2pr protocol defect and register a narrow corrective. If the reference
lane itself is unavailable or contradictory, record the exact boundary; do not increase budgets,
patch i2pd, or broaden network access. One-family pass cannot waive Plan 279.

## 13. Closure evidence

Exact source/binary pin, qualification SHA, bounded attempt budget, matrix results, local abuse
results, evidence index/checker, CI run, security/resource review, and unblock audit.

On pass, mark M12 experimental one-family progression passed but broad caps=f still unclaimed, and
move Plan 279 to ready.

## 14. Handoff

Plan 279 adds an independent Java-family qualification and only then may expose normal opt-in
floodfill activation.
