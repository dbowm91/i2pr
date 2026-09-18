# Plan 218 — M6 Java second-family final qualification and closure campaign

Status: **registered-blocked-on-plan217-m6-java-final-qualification**.

## 1. Objective

Run the corrected direct-I2CP Java second-family lane to completion on one exact
repository head and close M6 mixed-router interoperability if the stock Java I2P
2.13.0 reference satisfies the already-defined protocol/application evidence.

This plan deliberately does **not** start by rewriting the helpers to SAM.
Plan 217 must first establish that the direct public Java client path is measured
correctly.

The bounded product outcome is:

```text
stock Java I2P 2.13.0 public Destination
    -> current Standard LeaseSet2
    -> distinct Java publication/floodfill router
    -> ordinary DatabaseLookup/DatabaseStore over real i2pr tunnels
    -> i2pr LS2 validation/install
    -> bidirectional raw Destination delivery
    -> bidirectional Streaming qualification
    -> cleanup/resource baseline
```

On success this plan closes the Java second family and unblocks Plan 204
convergence. On a genuine reference-side boundary it stops cleanly and may
reactivate the retained Plan 205 SAM experiment.

## 2. Why this plan is blocked / readiness gate

Hard dependency: Plan 217 must close with
`passed-m6-java-closure-harness-and-evidence-corrective`.

At that point all required interfaces are stable:

- i2pr authenticated SSU2 and one-hop tunnel path already qualified;
- Java exact pin and controlled topology are frozen;
- direct public Java I2CP helpers exist;
- Branch A standard-I2NP/gzip decode path exists;
- Branch G LS2 lookup boundary counters exist;
- corrected evidence semantics and test isolation come from Plan 217;
- i2pd first-family M6 remains passed via Plan 193.

No unresolved architectural decision requires an ADR. SAM remains a conditional
fallback, not part of this plan's primary path.

## 3. Current implementation evidence to consume

Plan 218 consumes, but does not reinterpret without rerun:

- Plans 193, 196, 197, 200 and 201 landed machinery;
- Plan 216 point-in-time diagnostic;
- Plan 217 corrected driver/evidence/topology behavior;
- `scripts/check-m6-mixed-router-acceptance-evidence.sh`;
- `scripts/check-m6-final-closure-evidence.sh`;
- `.github/workflows/m6-mixed-router-external.yml`.

Protocol-derived evidence outranks Java log heuristics.

## 4. Invariants that must not regress

- exact stock Java I2P 2.13.0 @
  `9134f808337b401e8e53c73734c81fab04280c9d`;
- exact i2pd first-family pin 2.61.0 @
  `635b013a612ff47278ef02acf8580a28e10e26c5`;
- no public I2P participation;
- no Java patching, VMComm, reflection/private NetDB injection, direct LS2 copy,
  fabricated acceptance rows, or fake peer state;
- no production wire change to make a reference implementation accept malformed
  behavior;
- loopback-only/non-advertised test transport posture;
- fail-closed environment-gated test semantics;
- sanitized evidence only;
- all Plan 193 i2pd rows remain passed.

## 5. Scope

### In scope

- exact-head execution of the corrected Java destination and Streaming lanes;
- the smallest test-only/harness adjustment justified by a fresh captured
  boundary;
- evidence aggregation/checker fixes required to consume valid Plan 217 schema;
- closure/status/registry/roadmap updates;
- manual external workflow execution on the closing SHA.

### Explicitly out of scope

- preemptive SAM helper rewrite;
- public-network qualification;
- Java source patches;
- broad M6 harness redesign;
- new transports/protocol capabilities;
- M11 implementation;
- Plan 204's cross-document convergence work beyond unblocking it.

## 6. Ordered work packages

### A. Freeze exact head and preflight

1. Record i2pr SHA, Java pin, i2pd pin, OS and Rust toolchain.
2. Prove Java source tree/cache is unmodified.
3. Run static M6 evidence guards.
4. Start disposable Java A/B/C topology under Plan 217-normalized settings.
5. Record sanitized RouterInfo capabilities and tunnel eligibility.

### B. Destination qualification first

Run the corrected `destination_message_plane_against_java` alone before
Streaming.

Required progression:

1. authenticated sessions to publication and service Java routers;
2. RouterInfo bootstrap via ordinary authenticated I2NP;
3. public Java helper session ready;
4. real i2pr outbound and inbound one-hop tunnels install;
5. remote helper LS2 lookup is sent via the owned outbound tunnel;
6. LS2 response returns via the real inbound tunnel;
7. Standard LS2 key/signature/destination checks pass;
8. local i2pr LS2 publication is sent through the real tunnel;
9. raw payload reaches the Java reference;
10. Java reply reaches the i2pr Destination dispatcher;
11. cleanup returns manager/session/pending counters to baseline.

A failure must preserve the first protocol-authentic boundary and stop; do not
skip forward.

### C. Streaming qualification with isolated identifiers

Run `streaming_through_java` using Plan 217's collision-free identifier
namespace/topology isolation.

Qualify:

- SYN emitted/accepted;
- Direction A established;
- small payload digest;
- multi-packet payload digest;
- reverse small + multi-packet digests;
- sibling connection isolation;
- orderly close;
- Direction B established;
- Direction B forward/reverse data;
- Direction B close;
- reference accepted;
- cleanup baseline.

### D. Full Java and cross-family lanes

After targeted probes pass:

```bash
bash tests/integration/m6-interop/run-java.sh
bash tests/integration/m6-interop/run-m6-mixed-router.sh
```

No mandatory Java row may be blocked, failed, missing, or satisfied by stale
evidence.

### E. Hosted/manual exact-head closure

Run the manual M6 mixed-router external workflow on the same immutable closing
SHA. Consume the produced evidence artifact and verify its Java and i2pd
families correspond to the exact pins.

A second run is required only if the first run is flaky/ambiguous; do not make
repeat execution a substitute for fixing nondeterminism.

## 7. Failure, cancellation, restart, and contention semantics

- External subprocesses are bounded and cleaned after failure.
- A missing helper/router/reference pin is failure, not skip/pass.
- Evidence from another SHA or previous run is rejected.
- Destination and Streaming identifier spaces remain disjoint.
- A protocol failure stops at the first trustworthy boundary.
- Retry only after an identified environmental/transient cause; no blind retry
  loop.
- If stock Java's direct-I2CP path fails after Plan 217 with a reproducible
  Java-side/public-API limitation, Plan 218 records that terminal evidence and
  stops without changing i2pr production behavior.

## 8. Compatibility and migration

No user-facing migration. No support-level expansion beyond the M6 controlled
interop claim already defined by `specs/CONFORMANCE.md`.

On success, documentation may state Java I2P 2.13.0 second-family qualification
only for the exact bounded lane demonstrated here; it must not imply general
public-network compatibility.

## 9. Required tests and exact verification

Routine floor:

```bash
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
cargo deny check advisories bans sources
```

M6 authority:

```bash
bash scripts/check-m6-mixed-router-acceptance-evidence.sh
bash scripts/check-m6-final-closure-evidence.sh
bash tests/integration/m6-interop/run-java.sh
bash tests/integration/m6-interop/run-m6-mixed-router.sh
```

Then execute `.github/workflows/m6-mixed-router-external.yml` manually against
the same immutable SHA and archive/record the resulting evidence identifiers in
the closure record.

## 10. Documentation and authority updates

On successful closure:

- write `plans/closure/mixed-router-interop/218-status.md` with commits,
  command outcomes and requirement/evidence matrix;
- update Plan 201 status to retained/superseded-by-Plan-218 execution history,
  without deleting its earlier findings;
- leave Plan 216 unchanged as a diagnostic snapshot;
- mark Plan 205 retained/deferred-not-needed unless a separate future reason
  reactivates it;
- update `plans/registry.md` and the mixed-router roadmap;
- unblock Plan 204 in the same commit after the required unblock audit;
- update support/conformance claims only if their existing wording requires the
  newly proven Java-family state.

## 11. Acceptance criteria and conditional fallback

Plan 218 passes only when:

1. Plan 217 is closed/passed on the tested ancestor/head;
2. exact Java and i2pd pins are verified clean;
3. Java A/B/C controlled topology is loopback-only and reproducible;
4. publication/service RouterInfo bootstrap proofs pass;
5. real outbound and inbound i2pr tunnels install;
6. public Java helper owns a current Standard LS2;
7. ordinary tunneled DatabaseLookup returns that LS2 from the distinct Java
   publication/floodfill side;
8. i2pr validates the LS2 key, signature and destination;
9. local i2pr LS2 publication traverses the real tunnel path;
10. bidirectional raw Destination payloads match expected digests;
11. Direction A Streaming establishes;
12. Direction A small, multi-packet and reverse payload digests match;
13. sibling isolation passes;
14. Direction A closes cleanly;
15. Direction B establishes;
16. Direction B forward/reverse payload digests match;
17. Direction B closes cleanly;
18. reference-side acceptance evidence is positive;
19. cleanup/resource baselines pass;
20. every mandatory Java evidence row is passed with fresh command-derived
    evidence;
21. all retained i2pd Plan 193 rows remain passed;
22. M6 mixed-router and final closure evidence checkers are green;
23. manual external workflow succeeds on the same immutable SHA;
24. no forbidden shortcut/reference patch/product-wire accommodation was used;
25. closure record contains no unresolved critical/high finding.

**Conditional fallback:** if criteria 6–8 fail after Plan 217 for a reproducible
stock-Java public-client reason, Plan 218 MUST stop and record the exact
boundary. It may transition Plan 205 back from retained/deferred to ready in the
same closure/status commit. Do not execute SAM automatically and do not claim
M6 closure.

## 12. Closure evidence and authority transition

Successful authority transition:

```text
plan_217 = passed-m6-java-closure-harness-and-evidence-corrective
plan_218 = passed-m6-java-second-family-final-qualification
plan_201 = retained-superseded-by-plan218-final-qualification
plan_205 = retained-deferred-not-needed-after-direct-i2cp-closure

milestone6_i2pd_streaming_interop    = passed-via-plan193
milestone6_java_mixed_router_interop = passed-via-plan218
milestone6_interoperable             = passed-via-plan193-and-plan218

plan_204 = ready-cross-milestone-convergence-after-m6-java-closure
```

Plan 204 then owns documentation/evidence-authority convergence. Plan 218 must
not expand into Plan 204 work.

Handoff rule: execute only after Plan 217 status is passed. Start with the
destination probe, not the full lane, and stop at the first trustworthy failure
boundary.
