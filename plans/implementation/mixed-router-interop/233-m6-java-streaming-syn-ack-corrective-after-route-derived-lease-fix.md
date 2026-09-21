# Plan 233 — M6 Java Streaming SYN-ACK corrective after route-derived lease fix

Status: **superseded-before-execution-by-plan234-final-closure-authority-corrective**

> Supersession note: Plan 233 was never executed. Its narrow SYN-ACK attribution is carried into Plan 234, which also reconciles the still-required Plan-200/201 Java public-client LeaseSet lifecycle rows before any Java-family/M6 closure claim. See `plans/closure/mixed-router-interop/233-status.md`.

## 1. Objective

Attribute and close the exact post-correction Streaming boundary proven by
Plan 232, then continue through the retained Java Streaming qualification
far enough to either:

1. prove bidirectional Java Streaming establishment + digest-matched payload
   delivery + close/EOF + sibling isolation and close the Java second family;
   or
2. stop at the first new exact post-SYN boundary with bounded evidence.

Plan 233 is not a second fixture campaign. Plan 232 closed the
lease-gateway correction (all three sites route-derived, parity proven
live, raw-Destination reverse digest-matched). The only new fact is:

```text
streaming-syn-sent                       true (twice, same SHA, no tuning)
plan199-java-stop                        SYN-ACK never established
                                         (syn_accepted=false established=false pump_error=0)
```

## 2. Registration basis

Plan 232 closed as:

```text
passed-m6-java-route-derived-lease-gateway-fixture-corrective-with-raw-reverse-passed-streaming-boundary
```

Its Outcome B retains the raw-Destination reverse pass and registers
exactly this narrow corrective. The corrected fixture (route-derived
leases, Router B publication target, frozen topology/windows) is the
starting baseline and MUST NOT be relitigated.

## 3. Why ready

Hard dependencies closed: Plans 230 (bootstrap), 231 (attribution), 232
(fixture + raw reverse pass). Interface dependencies stable: the §25
checker invariants, the `p232_*` driver surface, and the exact pins
(Java I2P `2.13.0` @ `9134f808337b401e8e53c73734c81fab04280c9d`, i2pd
`2.61.0` @ `635b013a612ff47278ef02acf8580a28e10e26c5`).

## 4. Current implementation evidence

- `p232-streaming-lease-route stage=initial` parity true (two live runs).
- `ls2-publication-tunnel cells=1`, `lease-lookup-completed leases=1`.
- `streaming-syn-sent true` then the exact stop above.
- Refresh lease path corrected in code, unit-locked, never reached live.

## 5. Invariants

1. Plan-232 corrected leases, parity rows, publication separation, and
   §25 checker invariants remain unchanged.
2. Plan-230 C1+C2 topology corrections, Plan-231 observability, frozen
   windows (30 s poll, five-minute ceiling, 45 s reverse, 70 s status),
   exact pins, loopback-only, no-patching rules all stay frozen.
3. Router B remains the publication target; lease gateway stays
   route-derived.
4. Raw logs remain scratch-only. M10 product authority stays closed.
   Plan 205 SAM pivot stays off-path.

## 6. Scope

In: read-only attribution of the SYN → SYN-ACK leg (Java streaming
service destination state, helper `ReferenceStreamingService` readiness,
i2pr Streaming responder/pump observations, SYN retransmit/scheduling
facts), the minimal corrective the attribution justifies, and the
retained Direction A + Direction B Streaming qualification.

Out: lease-fixture redesign (closed), topology/profile/tunnel-policy
changes, timeout inflation, public I2P, reference patching, production
wire changes to go green, SAM bridge work.

No production `src/` change is authorized unless the corrected path
reaches a new, independently proven i2pr-owned defect. If that occurs,
stop at the exact boundary and register a separate production
corrective.

## 7. Work packages

- **A — SYN-ACK attribution.** Correlate the emitted SYN (message id,
  epoch) through Java-side SYN receipt (read-only helper/diagnostic
  facts only), the helper streaming accept path, and the i2pr responder
  pump. Emit exactly one earliest-stage `P233-*` terminal per counted
  run, mirroring the P231/P232 single-terminal discipline.
- **B — narrow corrective.** Apply only the fix the attribution
  justifies (helper readiness, responder scheduling, retransmit bound,
  or equivalent). No fixture retuning around the stop.
- **C — Streaming requalification.** Rerun retained Direction A
  (establish, payload digest both ways, close/EOF, sibling isolation,
  cleanup) and Direction B (CONNECT → accept, payload digests, close)
  including the live refresh/republication parity row that Plan 232
  never reached.
- **D — evidence contract.** Extend the checker with a §26 section:
  fail when the §25 surface regresses, when a `P233` token appears in
  production `src/`, or when the required unit rows are absent.

## 8. Failure / cancellation / restart semantics

Implementation committed before counted execution. Maximum three
counted external attempts per implementation SHA, no between-attempt
tuning. Runs that fail before the Plan-230 bootstrap gate record the
existing early-stop terminal under existing lane policy. A new
i2pr-owned boundary ends the plan without a production fix.

## 9. Compatibility and migration

None: test/harness/evidence surfaces only. No wire, config, or API
change.

## 10. Required tests

Unit rows (names may differ if coverage is explicit):

```text
p233_syn_sent_requires_route_parity
p233_syn_ack_missing_maps_to_attribution_not_pass
p233_streaming_refresh_reaches_live_republication
p233_timeout_windows_remain_frozen
p233_no_production_surface_change
```

plus the retained `p232_*`/`p231_*`/`p230_*` rows green.

## 11. Exact verification commands

Routine floor in `AGENTS.md` (fmt, check, test, clippy, doc,
deny, all boundary scripts including the extended
`check-m6-mixed-router-acceptance-evidence.sh`), plus:

```bash
cargo test --locked -p i2pr-daemon --test java_tunnel_external p233_ -- --test-threads=1
cargo test --locked -p i2pr-daemon --test java_tunnel_external p232_ -- --test-threads=1
cargo test --locked -p i2pr-daemon --test java_tunnel_external p231_ -- --test-threads=1
cargo test --locked -p i2pr-daemon --test java_tunnel_external --no-run
bash -n tests/integration/m6-interop/run-java.sh
bash -n scripts/check-m6-mixed-router-acceptance-evidence.sh
javac <all staged Java probes/helpers against exact pinned Java jars>
```

## 12. Documentation updates

Closure record `plans/closure/mixed-router-interop/233-status.md`;
registry + M6 roadmap + Plan-201/Plan-204 tokens updated together at
closure.

## 13. Acceptance criteria

1. SYN-ACK attribution reaches exactly one earliest missing stage.
2. The narrow corrective lands with regression tests/guards.
3. Direction A + Direction B Streaming rows pass digest-matched, or the
   first new exact boundary is recorded.
4. If all mandatory raw + Streaming rows pass, Java second-family M6 is
   marked passed and Plan 204 becomes dependency-ready.
5. No production change merely to close the lane; full exact-head
   verification passes.

## 14. Stop conditions

A new i2pr-owned boundary, fixture parity failure, or bootstrap
stochasticity exhausting the attempt budget each stop the plan at the
exact terminal with bounded evidence.

## 15. Closure evidence required

Implementation SHA(s); exact pins; retained Plan-232 parity rows;
per-attempt SYN epoch facts; exactly one terminal per counted attempt;
full verification; Plan-201/Plan-204 unblock audit; M10-unchanged
confirmation.

## 16. Closure outcomes

- **A — second-family closure:** all retained rows pass →
  `P233-JAVA-SECOND-FAMILY-PASSED`, Plan 201 closed/passed per retained
  authority, Plan 204 dependency-ready. No ceremonial successor.
- **B — new boundary:** record the exact `P233-*` terminal, retain all
  passes, register only the narrow corrective justified.
- **C — i2pr-owned defect:** record the boundary, register a separate
  production corrective; this plan makes no production change.

## 17. Registration disposition

```text
plan_232 = passed-m6-java-route-derived-lease-gateway-fixture-corrective-with-raw-reverse-passed-streaming-boundary
plan_233 = registered-ready-m6-java-streaming-syn-ack-corrective-after-route-derived-lease-fix

plan_201 = blocked-pending-plan233-streaming-syn-ack-corrective-after-plan232-raw-reverse-pass
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan233
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification

next_executable_plan = 233-m6-java-streaming-syn-ack-corrective-after-route-derived-lease-fix
milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed

milestone10_final_acceptance = closed
```

## 18. Handoff notes for smaller-model execution

Do not touch the lease fixture: it is closed and checker-pinned.
Start from the emitted SYN epoch on the corrected tree. Attribute
read-only from helper/diagnostic facts first; change code only for the
stage the attribution proves. Keep every window frozen and every raw
log scratch-only.
